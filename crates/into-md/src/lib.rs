mod archive;
mod cells;
mod docx;
mod markdown;
mod odf;
#[cfg(feature = "pdf")]
mod pdf;
mod pptx;
mod rtf;
#[cfg(test)]
mod tests;
mod text;
mod xls;
mod xlsx;
mod xml;

#[cfg(target_arch = "wasm32")]
mod wasm;

use std::fmt;

use archive::Archive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Docx,
    Pptx,
    Xlsx,
    Xls,
    Ods,
    Odt,
    Odp,
    Pdf,
    Rtf,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Docx => "docx",
            Format::Pptx => "pptx",
            Format::Xlsx => "xlsx",
            Format::Xls => "xls",
            Format::Ods => "ods",
            Format::Odt => "odt",
            Format::Odp => "odp",
            Format::Pdf => "pdf",
            Format::Rtf => "rtf",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_uncompressed_bytes: u64,
    pub max_zip_entries: usize,
    pub max_xml_elements: usize,
    pub max_table_cells: usize,
    pub max_markdown_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_input_bytes: 25 * 1024 * 1024,
            max_uncompressed_bytes: 20 * 1024 * 1024,
            max_zip_entries: 2000,
            max_xml_elements: 500_000,
            max_table_cells: 200_000,
            max_markdown_bytes: 2 * 1024 * 1024,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Unsupported,
    Encrypted,
    TooLarge(&'static str),
    Malformed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unsupported => write!(f, "unsupported file type"),
            Error::Encrypted => write!(f, "document is password-protected"),
            Error::TooLarge(what) => write!(f, "{what} limit exceeded"),
            Error::Malformed(reason) => write!(f, "malformed document: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

pub(crate) fn malformed(reason: impl fmt::Display) -> Error {
    Error::Malformed(reason.to_string())
}

#[derive(Debug)]
pub struct Converted {
    pub format: Format,
    pub markdown: String,
}

const PDF_MAGIC: &[u8] = b"%PDF-";
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
const RTF_MAGIC: &[u8] = b"{\\rtf";
const CFB_MAGIC: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

pub fn convert(bytes: &[u8], limits: &Limits) -> Result<Converted, Error> {
    if bytes.len() > limits.max_input_bytes {
        return Err(Error::TooLarge("input"));
    }

    let (format, markdown) = if bytes.starts_with(PDF_MAGIC) {
        (Format::Pdf, convert_pdf(bytes)?)
    } else if bytes.starts_with(RTF_MAGIC) {
        (Format::Rtf, rtf::convert(bytes, limits)?)
    } else if bytes.starts_with(CFB_MAGIC) {
        if contains_utf16(bytes, "EncryptedPackage") {
            return Err(Error::Encrypted);
        }
        (Format::Xls, xls::convert(bytes, limits)?)
    } else if bytes.starts_with(ZIP_MAGIC) {
        let mut archive = Archive::open(bytes, limits)?;
        let format = archive.detect()?;
        if archive.is_encrypted()? {
            return Err(Error::Encrypted);
        }
        let markdown = match format {
            Format::Docx => docx::convert(&mut archive, limits)?,
            Format::Pptx => pptx::convert(&mut archive, limits)?,
            Format::Odt | Format::Odp | Format::Ods => odf::convert(&mut archive, format == Format::Ods, limits)?,
            Format::Xlsx => xlsx::convert(&mut archive, limits)?,
            Format::Xls | Format::Pdf | Format::Rtf => return Err(Error::Unsupported),
        };
        (format, markdown)
    } else {
        return Err(Error::Unsupported);
    };

    let markdown = markdown::finish(&markdown);
    if markdown.len() > limits.max_markdown_bytes {
        return Err(Error::TooLarge("markdown"));
    }
    Ok(Converted { format, markdown })
}

fn contains_utf16(bytes: &[u8], needle: &str) -> bool {
    let encoded: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.windows(encoded.len()).any(|window| window == encoded)
}

#[cfg(feature = "pdf")]
fn convert_pdf(bytes: &[u8]) -> Result<String, Error> {
    pdf::convert(bytes)
}

#[cfg(not(feature = "pdf"))]
fn convert_pdf(_bytes: &[u8]) -> Result<String, Error> {
    Err(Error::Unsupported)
}
