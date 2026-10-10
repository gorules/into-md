use std::collections::BTreeMap;

use crate::cells::{Cells, NumberFormat, builtin, classify, grid, number};
use crate::markdown::Document;
use crate::{Error, Limits, malformed};

const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FREE_SECTOR: u32 = 0xFFFF_FFFF;

const BOF: u16 = 0x0809;
const EOF: u16 = 0x000A;
const FILEPASS: u16 = 0x002F;
const DATEMODE: u16 = 0x0022;
const BOUNDSHEET: u16 = 0x0085;
const SST: u16 = 0x00FC;
const CONTINUE: u16 = 0x003C;
const FORMAT: u16 = 0x041E;
const XF: u16 = 0x00E0;
const LABELSST: u16 = 0x00FD;
const LABEL: u16 = 0x0204;
const NUMBER: u16 = 0x0203;
const RK: u16 = 0x027E;
const MULRK: u16 = 0x00BD;
const BOOLERR: u16 = 0x0205;
const FORMULA: u16 = 0x0006;
const STRING: u16 = 0x0207;

pub fn convert(bytes: &[u8], limits: &Limits) -> Result<String, Error> {
    let compound = Compound::open(bytes)?;
    let workbook = compound.stream("Workbook")?.ok_or(Error::Unsupported)?;
    Workbook::parse(&workbook, limits)
}

struct Compound<'a> {
    bytes: &'a [u8],
    sector_size: usize,
    mini_sector_size: usize,
    mini_cutoff: u64,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    entries: Vec<Entry>,
}

struct Entry {
    name: String,
    kind: u8,
    start: u32,
    size: u64,
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    bytes.get(at..at + 2).map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at + 4).map(|quad| u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]))
}

fn read_f64(bytes: &[u8], at: usize) -> Option<f64> {
    bytes.get(at..at + 8).map(|octet| f64::from_le_bytes(octet.try_into().unwrap_or([0; 8])))
}

fn truncated() -> Error {
    malformed("truncated compound file")
}

impl<'a> Compound<'a> {
    fn open(bytes: &'a [u8]) -> Result<Self, Error> {
        let shift = read_u16(bytes, 0x1E).ok_or_else(truncated)?;
        let mini_shift = read_u16(bytes, 0x20).ok_or_else(truncated)?;
        if !(7..=16).contains(&shift) || mini_shift >= shift {
            return Err(malformed("invalid compound file sector size"));
        }
        let mut compound = Compound {
            bytes,
            sector_size: 1 << shift,
            mini_sector_size: 1 << mini_shift,
            mini_cutoff: read_u32(bytes, 0x38).ok_or_else(truncated)? as u64,
            fat: Vec::new(),
            mini_fat: Vec::new(),
            entries: Vec::new(),
        };

        let fat_sectors = read_u32(bytes, 0x2C).ok_or_else(truncated)? as usize;
        let mut difat: Vec<u32> = (0..109).filter_map(|index| read_u32(bytes, 0x4C + index * 4)).collect();
        let mut next = read_u32(bytes, 0x44).ok_or_else(truncated)?;
        let per_sector = compound.sector_size / 4;
        let mut guard = 0;
        while next != END_OF_CHAIN && next != FREE_SECTOR && guard < compound.sector_count() {
            let sector = compound.sector(next)?;
            difat.extend((0..per_sector - 1).filter_map(|index| read_u32(sector, index * 4)));
            next = read_u32(sector, (per_sector - 1) * 4).unwrap_or(END_OF_CHAIN);
            guard += 1;
        }
        for &sector in difat.iter().filter(|&&sector| sector != FREE_SECTOR).take(fat_sectors) {
            let data = compound.sector(sector)?;
            compound.fat.extend((0..per_sector).filter_map(|index| read_u32(data, index * 4)));
        }

        let directory = compound.chain(read_u32(bytes, 0x30).ok_or_else(truncated)?, None)?;
        compound.entries = directory
            .as_chunks::<128>()
            .0
            .iter()
            .map(|entry| {
                let length = (read_u16(entry, 0x40).unwrap_or(0) as usize).min(64);
                let units: Vec<u16> = entry[..length.saturating_sub(2)].as_chunks::<2>().0.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
                Entry {
                    name: String::from_utf16_lossy(&units),
                    kind: entry[0x42],
                    start: read_u32(entry, 0x74).unwrap_or(END_OF_CHAIN),
                    size: read_u32(entry, 0x78).unwrap_or(0) as u64,
                }
            })
            .collect();

        let mini_fat_start = read_u32(bytes, 0x3C).ok_or_else(truncated)?;
        if mini_fat_start != END_OF_CHAIN {
            let data = compound.chain(mini_fat_start, None)?;
            compound.mini_fat = data.as_chunks::<4>().0.iter().map(|&quad| u32::from_le_bytes(quad)).collect();
        }
        Ok(compound)
    }

    fn sector_count(&self) -> usize {
        self.bytes.len() / self.sector_size
    }

    fn sector(&self, index: u32) -> Result<&'a [u8], Error> {
        let start = (index as usize + 1).checked_mul(self.sector_size).ok_or_else(truncated)?;
        self.bytes.get(start..start + self.sector_size).ok_or_else(truncated)
    }

    fn chain(&self, start: u32, size: Option<u64>) -> Result<Vec<u8>, Error> {
        let mut data = Vec::new();
        let mut current = start;
        let mut steps = 0;
        while current != END_OF_CHAIN && current != FREE_SECTOR {
            if steps > self.sector_count() {
                return Err(malformed("compound file sector chain loops"));
            }
            data.extend_from_slice(self.sector(current)?);
            current = *self.fat.get(current as usize).ok_or_else(truncated)?;
            steps += 1;
        }
        if let Some(size) = size {
            data.truncate(size as usize);
        }
        Ok(data)
    }

    fn mini_chain(&self, start: u32, size: u64) -> Result<Vec<u8>, Error> {
        let root = self.entries.iter().find(|entry| entry.kind == 5).ok_or_else(|| malformed("missing root entry"))?;
        let mini_stream = self.chain(root.start, Some(root.size))?;
        let mut data = Vec::new();
        let mut current = start;
        let mut steps = 0;
        while current != END_OF_CHAIN && current != FREE_SECTOR {
            if steps > self.mini_fat.len() {
                return Err(malformed("compound file mini chain loops"));
            }
            let begin = current as usize * self.mini_sector_size;
            data.extend_from_slice(mini_stream.get(begin..begin + self.mini_sector_size).ok_or_else(truncated)?);
            current = *self.mini_fat.get(current as usize).ok_or_else(truncated)?;
            steps += 1;
        }
        data.truncate(size as usize);
        Ok(data)
    }

    fn stream(&self, name: &str) -> Result<Option<Vec<u8>>, Error> {
        let Some(entry) = self.entries.iter().find(|entry| entry.kind == 2 && entry.name.eq_ignore_ascii_case(name)) else {
            return Ok(None);
        };
        if entry.size < self.mini_cutoff {
            self.mini_chain(entry.start, entry.size).map(Some)
        } else {
            self.chain(entry.start, Some(entry.size)).map(Some)
        }
    }
}

struct Record<'a> {
    kind: u16,
    data: &'a [u8],
}

fn records(stream: &[u8], start: usize) -> impl Iterator<Item = Record<'_>> {
    let mut at = start;
    std::iter::from_fn(move || {
        let kind = read_u16(stream, at)?;
        let length = read_u16(stream, at + 2)? as usize;
        let data = stream.get(at + 4..at + 4 + length)?;
        let record = Record { kind, data };
        at += 4 + length;
        Some(record)
    })
}

struct Segments<'a> {
    parts: Vec<&'a [u8]>,
    part: usize,
    at: usize,
}

impl<'a> Segments<'a> {
    fn remaining(&self) -> usize {
        self.parts.get(self.part).map_or(0, |part| part.len().saturating_sub(self.at))
    }

    fn advance_part(&mut self) -> bool {
        if self.part + 1 >= self.parts.len() {
            return false;
        }
        self.part += 1;
        self.at = 0;
        true
    }

    fn byte(&mut self) -> Option<u8> {
        while self.remaining() == 0 {
            if !self.advance_part() {
                return None;
            }
        }
        let value = self.parts[self.part][self.at];
        self.at += 1;
        Some(value)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes([self.byte()?, self.byte()?]))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes([self.byte()?, self.byte()?, self.byte()?, self.byte()?]))
    }

    fn skip(&mut self, mut count: usize) -> Option<()> {
        while count > 0 {
            if self.remaining() == 0 && !self.advance_part() {
                return None;
            }
            let step = count.min(self.remaining());
            self.at += step;
            count -= step;
        }
        Some(())
    }

    fn characters(&mut self, count: usize, mut wide: bool) -> Option<String> {
        let mut units = Vec::with_capacity(count);
        while units.len() < count {
            if self.remaining() == 0 {
                if !self.advance_part() {
                    return None;
                }
                wide = self.byte()? & 1 == 1;
                continue;
            }
            units.push(if wide { self.u16()? } else { self.byte()? as u16 });
        }
        Some(String::from_utf16_lossy(&units))
    }

    fn rich_string(&mut self) -> Option<String> {
        let count = self.u16()? as usize;
        let flags = self.byte()?;
        let runs = if flags & 0x08 != 0 { self.u16()? as usize } else { 0 };
        let extended = if flags & 0x04 != 0 { self.u32()? as usize } else { 0 };
        let text = self.characters(count, flags & 0x01 != 0)?;
        self.skip(runs * 4)?;
        self.skip(extended)?;
        Some(text)
    }
}

fn unicode_string(data: &[u8], at: usize, long_count: bool) -> Option<String> {
    let (count, flags_at) = if long_count { (read_u16(data, at)? as usize, at + 2) } else { (*data.get(at)? as usize, at + 1) };
    let flags = *data.get(flags_at)?;
    let body = flags_at + 1;
    if flags & 0x01 != 0 {
        let units: Vec<u16> = data.get(body..body + count * 2)?.as_chunks::<2>().0.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
        Some(String::from_utf16_lossy(&units))
    } else {
        Some(data.get(body..body + count)?.iter().map(|&byte| byte as char).collect())
    }
}

fn rk_value(rk: u32) -> f64 {
    let value = if rk & 0x02 != 0 { ((rk as i32) >> 2) as f64 } else { f64::from_bits(((rk & 0xFFFF_FFFC) as u64) << 32) };
    if rk & 0x01 != 0 { value / 100.0 } else { value }
}

struct Workbook {
    strings: Vec<String>,
    formats: BTreeMap<u16, NumberFormat>,
    styles: Vec<u16>,
    date1904: bool,
    sheets: Vec<(String, usize)>,
}

impl Workbook {
    fn parse(stream: &[u8], limits: &Limits) -> Result<String, Error> {
        let mut workbook = Workbook { strings: Vec::new(), formats: BTreeMap::new(), styles: Vec::new(), date1904: false, sheets: Vec::new() };
        let mut iterator = records(stream, 0).peekable();
        let first = iterator.next().ok_or_else(|| malformed("empty workbook stream"))?;
        if first.kind != BOF || read_u16(first.data, 0) != Some(0x0600) {
            return Err(Error::Unsupported);
        }

        let mut count = 0usize;
        while let Some(record) = iterator.next() {
            count += 1;
            if count > limits.max_xml_elements {
                return Err(Error::TooLarge("xls records"));
            }
            match record.kind {
                EOF => break,
                FILEPASS => return Err(Error::Encrypted),
                DATEMODE => workbook.date1904 = read_u16(record.data, 0) == Some(1),
                BOUNDSHEET => {
                    let position = read_u32(record.data, 0).unwrap_or(0) as usize;
                    if record.data.get(5) == Some(&0)
                        && let Some(name) = unicode_string(record.data, 6, false)
                    {
                        workbook.sheets.push((name, position));
                    }
                }
                FORMAT => {
                    if let (Some(id), Some(code)) = (read_u16(record.data, 0), unicode_string(record.data, 2, true)) {
                        workbook.formats.insert(id, classify(&code));
                    }
                }
                XF => workbook.styles.push(read_u16(record.data, 2).unwrap_or(0)),
                SST => {
                    let mut parts = vec![record.data.get(8..).unwrap_or_default()];
                    while let Some(next) = iterator.next_if(|next| next.kind == CONTINUE) {
                        parts.push(next.data);
                    }
                    let unique = read_u32(record.data, 4).unwrap_or(0) as usize;
                    let mut segments = Segments { parts, part: 0, at: 0 };
                    for _ in 0..unique.min(limits.max_table_cells) {
                        match segments.rich_string() {
                            Some(text) => workbook.strings.push(text),
                            None => break,
                        }
                    }
                }
                _ => {}
            }
        }

        let mut document = Document::new(limits);
        for (name, position) in &workbook.sheets {
            let rows = grid(workbook.sheet(stream, *position, limits)?, limits)?;
            if rows.is_empty() {
                continue;
            }
            document.heading(2, name.clone());
            document.table(rows)?;
        }
        Ok(document.render())
    }

    fn format(&self, style: u16, value: f64) -> String {
        let format = self.styles.get(style as usize).map_or(NumberFormat::General, |id| {
            self.formats.get(id).copied().unwrap_or_else(|| builtin(*id as u32))
        });
        number(&value.to_string(), format, self.date1904)
    }

    fn sheet(&self, stream: &[u8], position: usize, limits: &Limits) -> Result<Cells, Error> {
        let mut cells = Cells::new();
        let mut count = 0usize;
        let mut pending: Option<(usize, usize)> = None;
        let mut put = |cells: &mut Cells, row: usize, column: usize, text: String| -> Result<(), Error> {
            if text.trim().is_empty() {
                return Ok(());
            }
            count += 1;
            if count > limits.max_table_cells {
                return Err(Error::TooLarge("table cells"));
            }
            cells.entry(row).or_default().push((column, text));
            Ok(())
        };

        let mut iterator = records(stream, position);
        match iterator.next() {
            Some(record) if record.kind == BOF => {}
            _ => return Ok(cells),
        }
        for (index, record) in iterator.enumerate() {
            if index > limits.max_xml_elements {
                return Err(Error::TooLarge("xls records"));
            }
            let data = record.data;
            let (row, column, style) = (
                read_u16(data, 0).unwrap_or(0) as usize,
                read_u16(data, 2).unwrap_or(0) as usize + 1,
                read_u16(data, 4).unwrap_or(0),
            );
            match record.kind {
                EOF | BOF => break,
                LABELSST => {
                    let text = read_u32(data, 6).and_then(|index| self.strings.get(index as usize)).cloned().unwrap_or_default();
                    put(&mut cells, row, column, text)?;
                }
                LABEL => put(&mut cells, row, column, unicode_string(data, 6, true).unwrap_or_default())?,
                NUMBER => {
                    if let Some(value) = read_f64(data, 6) {
                        put(&mut cells, row, column, self.format(style, value))?;
                    }
                }
                RK => {
                    if let Some(rk) = read_u32(data, 6) {
                        put(&mut cells, row, column, self.format(style, rk_value(rk)))?;
                    }
                }
                MULRK => {
                    let entries = data.len().saturating_sub(6) / 6;
                    for entry in 0..entries {
                        let at = 4 + entry * 6;
                        if let (Some(style), Some(rk)) = (read_u16(data, at), read_u32(data, at + 2)) {
                            put(&mut cells, row, column + entry, self.format(style, rk_value(rk)))?;
                        }
                    }
                }
                BOOLERR => {
                    let (value, is_error) = (data.get(6).copied().unwrap_or(0), data.get(7).copied().unwrap_or(0));
                    let text = match (is_error, value) {
                        (0, 0) => "FALSE".to_string(),
                        (0, _) => "TRUE".to_string(),
                        (_, code) => error_text(code).to_string(),
                    };
                    put(&mut cells, row, column, text)?;
                }
                FORMULA => {
                    let result = data.get(6..14).unwrap_or_default();
                    if result.len() == 8 && result[6] == 0xFF && result[7] == 0xFF {
                        match result[0] {
                            0 => pending = Some((row, column)),
                            1 => put(&mut cells, row, column, if result[2] != 0 { "TRUE" } else { "FALSE" }.to_string())?,
                            2 => put(&mut cells, row, column, error_text(result[2]).to_string())?,
                            _ => {}
                        }
                    } else if let Some(value) = read_f64(data, 6) {
                        put(&mut cells, row, column, self.format(style, value))?;
                    }
                }
                STRING => {
                    if let Some((row, column)) = pending.take() {
                        put(&mut cells, row, column, unicode_string(data, 0, true).unwrap_or_default())?;
                    }
                }
                _ => {}
            }
        }
        Ok(cells)
    }
}

fn error_text(code: u8) -> &'static str {
    match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        0x2A => "#N/A",
        _ => "#ERROR",
    }
}
