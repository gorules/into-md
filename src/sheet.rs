use std::fmt::Display;
use std::io::Cursor;

use calamine::{Data, Reader, Xls, XlsError, open_workbook_from_rs};

use crate::markdown::Document;
use crate::{Error, Limits, malformed};

type Source<'a> = Cursor<&'a [u8]>;

pub fn convert(bytes: &[u8], limits: &Limits) -> Result<String, Error> {
    match open_workbook_from_rs::<Xls<Source<'_>>, _>(Cursor::new(bytes)) {
        Ok(workbook) => render(workbook, limits),
        Err(XlsError::Password) => Err(Error::Encrypted),
        Err(_) => Err(Error::Unsupported),
    }
}

fn render<'a, R>(mut workbook: R, limits: &Limits) -> Result<String, Error>
where
    R: Reader<Source<'a>>,
    R::Error: Display,
{
    let mut document = Document::new(limits);

    for sheet in workbook.sheet_names() {
        let range = workbook.worksheet_range(&sheet).map_err(malformed)?;
        if range.is_empty() {
            continue;
        }
        let (height, width) = range.get_size();
        if height.saturating_mul(width) > limits.max_table_cells {
            return Err(Error::TooLarge("table cells"));
        }
        let rows = range.rows().map(|row| row.iter().map(cell).collect()).collect();
        document.heading(2, sheet);
        document.table(rows)?;
    }

    Ok(document.render())
}

fn cell(data: &Data) -> String {
    match data {
        Data::Int(value) => value.to_string(),
        Data::Float(value) if value.fract() == 0.0 && value.abs() < 1e15 => (*value as i64).to_string(),
        Data::Float(value) => value.to_string(),
        Data::String(value) | Data::DateTimeIso(value) | Data::DurationIso(value) => value.clone(),
        Data::Bool(value) => if *value { "TRUE" } else { "FALSE" }.to_string(),
        Data::DateTime(value) if value.is_datetime() => {
            let (year, month, day, hour, minute, second, _) = value.to_ymd_hms_milli();
            if (hour, minute, second) == (0, 0, 0) {
                format!("{year:04}-{month:02}-{day:02}")
            } else {
                format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
            }
        }
        Data::DateTime(value) => value.as_f64().to_string(),
        Data::Error(error) => format!("#{error:?}"),
        Data::Empty => String::new(),
    }
}
