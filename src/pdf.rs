use crate::{Error, malformed};

pub fn convert(bytes: &[u8]) -> Result<String, Error> {
    let text = pdf_extract::extract_text_from_mem(bytes).map_err(malformed)?;
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
