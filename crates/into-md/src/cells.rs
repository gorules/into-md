use std::collections::BTreeMap;

use crate::{Error, Limits};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NumberFormat {
    General,
    Date,
    Duration,
    Percent(usize),
}

pub fn builtin(id: u32) -> NumberFormat {
    match id {
        9 => NumberFormat::Percent(0),
        10 => NumberFormat::Percent(2),
        14..=22 | 27..=36 | 45 | 47 | 50..=58 => NumberFormat::Date,
        46 => NumberFormat::Duration,
        _ => NumberFormat::General,
    }
}

pub fn classify(code: &str) -> NumberFormat {
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

pub fn number(raw: &str, format: NumberFormat, date1904: bool) -> String {
    let raw = raw.trim();
    let Ok(value) = raw.parse::<f64>() else { return raw.to_string() };
    match format {
        NumberFormat::Date => date(value, date1904).unwrap_or_else(|| general(value)),
        NumberFormat::Percent(decimals) => format!("{:.*}%", decimals, value * 100.0),
        NumberFormat::Duration | NumberFormat::General => general(value),
    }
}

pub fn general(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return (value as i64).to_string();
    }
    let rounded: f64 = format!("{value:.14e}").parse().unwrap_or(value);
    rounded.to_string()
}

pub fn date(serial: f64, date1904: bool) -> Option<String> {
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

pub type Cells = BTreeMap<usize, Vec<(usize, String)>>;

pub fn grid(cells: Cells, limits: &Limits) -> Result<Vec<Vec<String>>, Error> {
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
