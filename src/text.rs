const WINDOWS_1252_HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F,
    0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

pub fn decode(code_page: u16, bytes: &[u8]) -> String {
    if bytes.is_ascii() {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    match code_page {
        1252 | 0 => bytes.iter().map(|&byte| windows_1252(byte)).collect(),
        28591 => bytes.iter().map(|&byte| byte as char).collect(),
        65001 => String::from_utf8_lossy(bytes).into_owned(),
        _ => host_decode(code_page, bytes).unwrap_or_else(|| bytes.iter().map(|&byte| windows_1252(byte)).collect()),
    }
}

fn windows_1252(byte: u8) -> char {
    match byte {
        0x80..=0x9F => char::from_u32(WINDOWS_1252_HIGH[(byte - 0x80) as usize] as u32).unwrap_or('\u{FFFD}'),
        _ => byte as char,
    }
}

#[cfg(target_arch = "wasm32")]
fn host_decode(code_page: u16, bytes: &[u8]) -> Option<String> {
    #[link(wasm_import_module = "into_md")]
    unsafe extern "C" {
        fn decode(code_page: u32, source: *const u8, source_len: usize, target: *mut u8, target_capacity: usize) -> isize;
    }
    let mut target = vec![0u8; bytes.len() * 3];
    let written = unsafe { decode(code_page as u32, bytes.as_ptr(), bytes.len(), target.as_mut_ptr(), target.len()) };
    if written < 0 {
        return None;
    }
    target.truncate(written as usize);
    String::from_utf8(target).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn host_decode(code_page: u16, bytes: &[u8]) -> Option<String> {
    let encoding = codepage::to_encoding(code_page)?;
    Some(encoding.decode_without_bom_handling(bytes).0.into_owned())
}
