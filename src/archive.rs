use std::io::{self, Cursor, Read};

use zip::ZipArchive;

use crate::{Error, Format, Limits, malformed};

pub struct Archive<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
}

impl<'a> Archive<'a> {
    pub fn open(bytes: &'a [u8], limits: &Limits) -> Result<Self, Error> {
        let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(malformed)?;
        if zip.len() > limits.max_zip_entries {
            return Err(Error::TooLarge("zip entries"));
        }

        let mut budget = limits.max_uncompressed_bytes;
        for index in 0..zip.len() {
            let entry = zip.by_index(index).map_err(malformed)?;
            if entry.size() > budget {
                return Err(Error::TooLarge("uncompressed size"));
            }
            let inflated = io::copy(&mut entry.take(budget + 1), &mut io::sink()).map_err(malformed)?;
            if inflated > budget {
                return Err(Error::TooLarge("uncompressed size"));
            }
            budget -= inflated;
        }

        Ok(Archive { zip })
    }

    pub fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>, Error> {
        let mut entry = match self.zip.by_name(name) {
            Ok(entry) => entry,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(err) => return Err(malformed(err)),
        };
        let mut buffer = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buffer).map_err(malformed)?;
        Ok(Some(buffer))
    }

    pub fn require(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        self.read(name)?.ok_or_else(|| malformed(format!("missing {name}")))
    }

    fn contains(&self, name: &str) -> bool {
        self.zip.index_for_name(name).is_some()
    }

    pub fn is_encrypted(&mut self) -> Result<bool, Error> {
        let manifest = self.read("META-INF/manifest.xml")?.unwrap_or_default();
        Ok(manifest.windows(15).any(|window| window == b"encryption-data"))
    }

    pub fn detect(&mut self) -> Result<Format, Error> {
        if self.contains("word/document.xml") {
            return Ok(Format::Docx);
        }
        if self.contains("ppt/presentation.xml") {
            return Ok(Format::Pptx);
        }
        if self.contains("xl/workbook.xml") {
            return Ok(Format::Xlsx);
        }
        let mimetype = self.read("mimetype")?.unwrap_or_default();
        match mimetype.trim_ascii() {
            b"application/vnd.oasis.opendocument.text" => Ok(Format::Odt),
            b"application/vnd.oasis.opendocument.spreadsheet" => Ok(Format::Ods),
            b"application/vnd.oasis.opendocument.presentation" => Ok(Format::Odp),
            _ => Err(Error::Unsupported),
        }
    }
}

pub fn join_path(from: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_string();
    }
    let mut segments: Vec<&str> = from.split('/').collect();
    segments.pop();
    for segment in target.split('/') {
        match segment {
            ".." => {
                segments.pop();
            }
            "." | "" => {}
            _ => segments.push(segment),
        }
    }
    segments.join("/")
}
