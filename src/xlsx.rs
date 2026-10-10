use std::collections::BTreeMap;

use crate::archive::{Archive, join_path};
use crate::markdown::{Document, Inline};
use crate::xml::{Node, attr, name, prefixed_attr, relationships, walk};
use crate::{Error, Limits};

const WORKBOOK: &str = "xl/workbook.xml";

#[derive(Clone, Copy, PartialEq, Eq)]
enum NumberFormat {
    General,
    Date,
    Duration,
    Percent(usize),
}

struct Sheet {
    name: String,
    path: String,
}

pub fn convert(archive: &mut Archive<'_>, limits: &Limits) -> Result<String, Error> {
    let rels = match archive.read("xl/_rels/workbook.xml.rels")? {
        Some(xml) => relationships(&xml, limits)?,
        None => Default::default(),
    };
    let target = |needle: &str, fallback: &str| {
        rels.values().find(|target| target.contains(needle)).map(|target| join_path(WORKBOOK, target)).unwrap_or_else(|| fallback.to_string())
    };
    let strings_path = target("sharedStrings", "xl/sharedStrings.xml");
    let styles_path = target("styles", "xl/styles.xml");

    let workbook = archive.require(WORKBOOK)?;
    let mut sheets = Vec::new();
    let mut date1904 = false;
    walk(&workbook, limits, |node| {
        if let Node::Open(start) = node {
            match name(start) {
                b"sheet" => {
                    if let (Some(sheet_name), Some(target)) =
                        (attr(start, b"name"), prefixed_attr(start, b"id").and_then(|id| rels.get(&id)))
                    {
                        sheets.push(Sheet { name: sheet_name, path: join_path(WORKBOOK, target) });
                    }
                }
                b"workbookPr" => date1904 = matches!(attr(start, b"date1904").as_deref(), Some("1" | "true")),
                _ => {}
            }
        }
        Ok(())
    })?;

    let strings = match archive.read(&strings_path)? {
        Some(xml) => shared_strings(&xml, limits)?,
        None => Vec::new(),
    };
    let formats = match archive.read(&styles_path)? {
        Some(xml) => cell_formats(&xml, limits)?,
        None => Vec::new(),
    };

    let mut document = Document::new(limits);
    for sheet in sheets {
        let Some(xml) = archive.read(&sheet.path)? else { continue };
        let rows = worksheet(&xml, &strings, &formats, date1904, limits)?;
        if rows.is_empty() {
            continue;
        }
        document.heading(2, sheet.name);
        document.table(rows)?;
    }
    Ok(document.render())
}

fn shared_strings(xml: &[u8], limits: &Limits) -> Result<Vec<String>, Error> {
    let mut strings = Vec::new();
    let mut current: Option<Inline> = None;
    let (mut in_text, mut phonetic) = (false, false);
    walk(xml, limits, |node| {
        match node {
            Node::Open(start) => match name(start) {
                b"si" => current = Some(Inline::default()),
                b"rPh" => phonetic = true,
                b"t" if !phonetic => in_text = true,
                _ => {}
            },
            Node::Close(b"t") => in_text = false,
            Node::Close(b"rPh") => phonetic = false,
            Node::Close(b"si") => strings.push(current.take().map(|inline| inline.plain()).unwrap_or_default()),
            Node::Text(text) if in_text => {
                if let Some(inline) = current.as_mut() {
                    inline.push(text, false, false);
                }
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok(strings)
}

fn cell_formats(xml: &[u8], limits: &Limits) -> Result<Vec<NumberFormat>, Error> {
    let mut custom: BTreeMap<u32, NumberFormat> = BTreeMap::new();
    let mut ids = Vec::new();
    let mut in_cell_xfs = false;
    walk(xml, limits, |node| {
        match node {
            Node::Open(start) => match name(start) {
                b"numFmt" => {
                    if let (Some(id), Some(code)) = (attr(start, b"numFmtId").and_then(|v| v.parse().ok()), attr(start, b"formatCode")) {
                        custom.insert(id, classify(&code));
                    }
                }
                b"cellXfs" => in_cell_xfs = true,
                b"xf" if in_cell_xfs => ids.push(attr(start, b"numFmtId").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0)),
                _ => {}
            },
            Node::Close(b"cellXfs") => in_cell_xfs = false,
            _ => {}
        }
        Ok(())
    })?;
    Ok(ids.into_iter().map(|id| custom.get(&id).copied().unwrap_or_else(|| builtin(id))).collect())
}

fn builtin(id: u32) -> NumberFormat {
    match id {
        9 => NumberFormat::Percent(0),
        10 => NumberFormat::Percent(2),
        14..=22 | 27..=36 | 45 | 47 | 50..=58 => NumberFormat::Date,
        46 => NumberFormat::Duration,
        _ => NumberFormat::General,
    }
}

fn classify(code: &str) -> NumberFormat {
    let mut visible = String::new();
    let mut chars = code.chars();
    let mut elapsed = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                for inner in chars.by_ref() {
                    if inner == '"' {
                        break;
                    }
                }
            }
            '\\' | '_' | '*' => {
                chars.next();
            }
            '[' => {
                let bracket: String = chars.by_ref().take_while(|inner| *inner != ']').collect();
                if matches!(bracket.to_ascii_lowercase().chars().next(), Some('h' | 'm' | 's')) {
                    elapsed = true;
                }
            }
            ';' => break,
            _ => visible.push(ch.to_ascii_lowercase()),
        }
    }
    if elapsed {
        return NumberFormat::Duration;
    }
    if visible.contains('%') {
        let decimals = visible.split('.').nth(1).map_or(0, |fraction| fraction.chars().take_while(|ch| *ch == '0').count());
        return NumberFormat::Percent(decimals);
    }
    if visible != "general" && visible.chars().any(|ch| matches!(ch, 'd' | 'm' | 'y' | 'h' | 's')) {
        return NumberFormat::Date;
    }
    NumberFormat::General
}

fn worksheet(
    xml: &[u8],
    strings: &[String],
    formats: &[NumberFormat],
    date1904: bool,
    limits: &Limits,
) -> Result<Vec<Vec<String>>, Error> {
    let mut cells: BTreeMap<usize, Vec<(usize, String)>> = BTreeMap::new();
    let mut count = 0usize;
    let (mut row, mut column) = (0usize, 0usize);
    let mut kind = String::new();
    let mut style = 0usize;
    let mut value = String::new();
    let mut inline = Inline::default();
    let (mut in_value, mut in_inline_text) = (false, false);

    walk(xml, limits, |node| {
        match node {
            Node::Open(start) => match name(start) {
                b"row" => {
                    row = attr(start, b"r").and_then(|v| v.parse::<usize>().ok()).unwrap_or(row + 1);
                    column = 0;
                }
                b"c" => {
                    column = attr(start, b"r").and_then(|reference| column_index(&reference)).unwrap_or(column + 1);
                    kind = attr(start, b"t").unwrap_or_default();
                    style = attr(start, b"s").and_then(|v| v.parse().ok()).unwrap_or(0);
                    value.clear();
                    inline = Inline::default();
                }
                b"v" => in_value = true,
                b"t" => in_inline_text = true,
                _ => {}
            },
            Node::Close(local) => match local {
                b"v" => in_value = false,
                b"t" => in_inline_text = false,
                b"c" => {
                    let text = match kind.as_str() {
                        "s" => value.trim().parse::<usize>().ok().and_then(|index| strings.get(index)).cloned().unwrap_or_default(),
                        "inlineStr" => inline.plain(),
                        "b" => if value.trim() == "1" { "TRUE" } else { "FALSE" }.to_string(),
                        "str" | "e" | "d" => value.trim().to_string(),
                        _ => number(&value, formats.get(style).copied().unwrap_or(NumberFormat::General), date1904),
                    };
                    if !text.trim().is_empty() {
                        count += 1;
                        if count > limits.max_table_cells {
                            return Err(Error::TooLarge("table cells"));
                        }
                        cells.entry(row).or_default().push((column, text));
                    }
                }
                _ => {}
            },
            Node::Text(text) => {
                if in_value {
                    value.push_str(text);
                } else if in_inline_text {
                    inline.push(text, false, false);
                }
            }
        }
        Ok(())
    })?;

    let first = cells.values().flatten().map(|(column, _)| *column).min().unwrap_or(1);
    let last = cells.values().flatten().map(|(column, _)| *column).max().unwrap_or(1);
    let width = last - first + 1;
    if width.saturating_mul(cells.len()) > limits.max_table_cells {
        return Err(Error::TooLarge("table cells"));
    }
    Ok(cells
        .into_values()
        .map(|row| {
            let mut line = vec![String::new(); width];
            for (column, text) in row {
                line[column - first] = text;
            }
            line
        })
        .collect())
}

fn column_index(reference: &str) -> Option<usize> {
    let letters: Vec<u8> = reference.bytes().take_while(u8::is_ascii_alphabetic).collect();
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    Some(letters.iter().fold(0usize, |index, letter| index * 26 + (letter.to_ascii_uppercase() - b'A' + 1) as usize))
}

fn number(raw: &str, format: NumberFormat, date1904: bool) -> String {
    let raw = raw.trim();
    let Ok(value) = raw.parse::<f64>() else { return raw.to_string() };
    match format {
        NumberFormat::Date => date(value, date1904).unwrap_or_else(|| general(value)),
        NumberFormat::Percent(decimals) => format!("{:.*}%", decimals, value * 100.0),
        NumberFormat::Duration | NumberFormat::General => general(value),
    }
}

fn general(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return (value as i64).to_string();
    }
    let rounded: f64 = format!("{value:.14e}").parse().unwrap_or(value);
    rounded.to_string()
}

fn date(serial: f64, date1904: bool) -> Option<String> {
    if !(0.0..2_958_466.0).contains(&serial) {
        return None;
    }
    let mut days = serial.floor() as i64;
    let mut seconds = ((serial - serial.floor()) * 86_400.0).round() as i64;
    if seconds >= 86_400 {
        days += 1;
        seconds = 0;
    }
    if date1904 {
        days += 1462;
    } else if days < 60 {
        days += 1;
    }
    let (year, month, day) = civil(days - 25_569);
    let (hour, minute, second) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if serial < 1.0 && !date1904 {
        return Some(format!("{hour:02}:{minute:02}:{second:02}"));
    }
    if seconds == 0 {
        Some(format!("{year:04}-{month:02}-{day:02}"))
    } else {
        Some(format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"))
    }
}

fn civil(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
