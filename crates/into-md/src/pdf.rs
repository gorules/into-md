use crate::{Error, malformed};

pub fn convert(bytes: &[u8]) -> Result<String, Error> {
    let text = expand_ligatures(&into_md_pdf::extract_text_from_mem(bytes).map_err(malformed)?);
    let mut out = String::with_capacity(text.len());
    for paragraph in text.split("\n\n") {
        let joined = paragraph.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n");
        if joined.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&joined);
    }
    Ok(out)
}

fn expand_ligatures(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\u{FB00}' => out.push_str("ff"),
            '\u{FB01}' => out.push_str("fi"),
            '\u{FB02}' => out.push_str("fl"),
            '\u{FB03}' => out.push_str("ffi"),
            '\u{FB04}' => out.push_str("ffl"),
            '\u{FB05}' | '\u{FB06}' => out.push_str("st"),
            '\0' => {}
            other => out.push(other),
        }
    }
    out
}
