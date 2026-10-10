use std::borrow::Cow;

#[derive(Debug, PartialEq, Eq)]
pub struct Encoding {
    name: &'static str,
    big_endian: bool,
}

pub static UTF_16BE: &Encoding = &Encoding { name: "UTF-16BE", big_endian: true };
pub static UTF_16LE: &Encoding = &Encoding { name: "UTF-16LE", big_endian: false };

impl Encoding {
    pub fn name(&'static self) -> &'static str {
        self.name
    }

    pub fn decode<'a>(&'static self, bytes: &'a [u8]) -> (Cow<'a, str>, &'static Encoding, bool) {
        if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
            let text = String::from_utf8_lossy(rest);
            let malformed = matches!(text, Cow::Owned(_));
            return (Cow::Owned(text.into_owned()), self, malformed);
        }
        let (encoding, body) = match bytes {
            [0xFE, 0xFF, rest @ ..] => (UTF_16BE, rest),
            [0xFF, 0xFE, rest @ ..] => (UTF_16LE, rest),
            _ => (self, bytes),
        };
        let (text, malformed) = encoding.decode_units(body, true);
        (Cow::Owned(text.unwrap_or_default()), encoding, malformed)
    }

    pub fn decode_without_bom_handling_and_without_replacement<'a>(&'static self, bytes: &'a [u8]) -> Option<Cow<'a, str>> {
        match self.decode_units(bytes, false) {
            (Some(text), false) => Some(Cow::Owned(text)),
            _ => None,
        }
    }

    fn decode_units(&self, bytes: &[u8], replace: bool) -> (Option<String>, bool) {
        let mut malformed = bytes.len() % 2 == 1;
        let units = bytes.as_chunks::<2>().0.iter().map(|&pair| if self.big_endian { u16::from_be_bytes(pair) } else { u16::from_le_bytes(pair) });
        let mut text = String::with_capacity(bytes.len() / 2);
        for unit in char::decode_utf16(units) {
            match unit {
                Ok(ch) => text.push(ch),
                Err(_) if replace => {
                    malformed = true;
                    text.push('\u{FFFD}');
                }
                Err(_) => return (None, true),
            }
        }
        if malformed && replace && bytes.len() % 2 == 1 {
            text.push('\u{FFFD}');
        }
        (Some(text), malformed)
    }
}
