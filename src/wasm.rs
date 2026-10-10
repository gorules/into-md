use std::cell::RefCell;

use crate::{Error, Format, Limits, convert};

thread_local! {
    static OUTPUT: RefCell<(u32, Vec<u8>)> = const { RefCell::new((0, Vec::new())) };
}

#[cfg(feature = "pdf")]
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(_dest: *mut u8, _len: usize) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

#[unsafe(no_mangle)]
pub extern "C" fn into_md_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn into_md_free(pointer: *mut u8, len: usize) {
    drop(unsafe { Vec::from_raw_parts(pointer, 0, len) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn into_md_convert(
    pointer: *const u8,
    len: usize,
    max_uncompressed_bytes: u32,
    max_zip_entries: u32,
    max_xml_elements: u32,
    max_table_cells: u32,
    max_markdown_bytes: u32,
) -> u32 {
    let bytes = unsafe { std::slice::from_raw_parts(pointer, len) };
    let limits = Limits {
        max_input_bytes: usize::MAX,
        max_uncompressed_bytes: max_uncompressed_bytes as u64,
        max_zip_entries: max_zip_entries as usize,
        max_xml_elements: max_xml_elements as usize,
        max_table_cells: max_table_cells as usize,
        max_markdown_bytes: max_markdown_bytes as usize,
    };
    let (status, format, output) = match convert(bytes, &limits) {
        Ok(converted) => (0, format_code(converted.format), converted.markdown),
        Err(error @ Error::Unsupported) => (1, 0, error.to_string()),
        Err(error @ Error::TooLarge(_)) => (2, 0, error.to_string()),
        Err(error @ Error::Malformed(_)) => (3, 0, error.to_string()),
        Err(error @ Error::Encrypted) => (4, 0, error.to_string()),
    };
    OUTPUT.with_borrow_mut(|slot| *slot = (format, output.into_bytes()));
    status
}

#[unsafe(no_mangle)]
pub extern "C" fn into_md_format() -> u32 {
    OUTPUT.with_borrow(|slot| slot.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn into_md_output_ptr() -> *const u8 {
    OUTPUT.with_borrow(|slot| slot.1.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn into_md_output_len() -> usize {
    OUTPUT.with_borrow(|slot| slot.1.len())
}

fn format_code(format: Format) -> u32 {
    match format {
        Format::Docx => 1,
        Format::Pptx => 2,
        Format::Xlsx => 3,
        Format::Xls => 4,
        Format::Ods => 5,
        Format::Odt => 6,
        Format::Odp => 7,
        Format::Pdf => 8,
        Format::Rtf => 9,
    }
}
