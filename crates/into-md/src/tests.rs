use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::{Error, Format, Limits, convert};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../test/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn markdown(name: &str, format: Format) -> String {
    let converted = convert(&fixture(name), &Limits::default()).unwrap();
    assert_eq!(converted.format, format);
    converted.markdown
}

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(content).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn docx_keeps_headings_formatting_lists_and_tables() {
    let markdown = markdown("policy.docx", Format::Docx);
    assert!(markdown.starts_with("# Loan Policy\n\nApplicants must be **18+** and *employed*. See [the guide](https://gorules.io/docs)."));
    assert!(markdown.contains("## Eligibility\n\n- Income > 30k\n- No defaults\n\n1. Submit application\n1. Await review"));
    assert!(markdown.contains("| A | 750 | 3.5% |\n| B | 650 | 5.0% |"));
}

#[test]
fn odt_keeps_headings_lists_and_tables() {
    let markdown = markdown("policy.odt", Format::Odt);
    assert!(markdown.starts_with("# Loan Policy\n\nApplicants must be 18+ and employed."));
    assert!(markdown.contains("- Income > 30k\n- No defaults"));
    assert!(markdown.contains("| Grade | Min score | Rate |\n| --- | --- | --- |\n| A | 750 | 3.5% |"));
}

#[test]
fn xlsx_renders_every_non_empty_sheet_with_typed_cells() {
    let markdown = markdown("rates.xlsx", Format::Xlsx);
    assert!(markdown.contains("## Rates\n\n| Grade | Min | Rate | Effective |"));
    assert!(markdown.contains("| A | 750 | 0.035 | 2026-01-15 |"));
    assert!(markdown.contains("## Limits | caps\n\n| Country | Max |\n| --- | --- |\n| US | 100000 |"));
    assert!(!markdown.contains("## Empty"));
}

#[test]
fn ods_renders_sheets() {
    let markdown = markdown("rates.ods", Format::Ods);
    assert_eq!(markdown, "## Rates\n\n| Grade | Min |\n| --- | --- |\n| A | 750 |");
}

#[test]
fn pptx_follows_presentation_order_with_titles_bullets_and_tables() {
    let markdown = markdown("deck.pptx", Format::Pptx);
    assert!(markdown.starts_with(
        "## Slide 1: Eligibility & scope\n\n- Income > 30k\n  - Verified by payslip\n- **No defaults**\n\n## Slide 2: Rates"
    ));
    assert!(markdown.contains("| Grade | Rate |\n| --- | --- |\n| A | 3.5% |\n\nFootnote with [pricing link](https://gorules.io/pricing)\n\n> **Notes:** Mention the 2026 rate review.\n>\n> Keep it short."));
}

#[test]
fn odp_renders_slides() {
    let markdown = markdown("deck.odp", Format::Odp);
    assert_eq!(markdown, "## Slide 1\n\nLoan Policy\n\n- Income above 30k\n\n## Slide 2: Pricing\n\nRates vary");
}

#[cfg(feature = "pdf")]
#[test]
fn pdf_extracts_text() {
    let markdown = markdown("policy.pdf", Format::Pdf);
    assert_eq!(markdown, "Loan Policy\n\nApplicants must be 18 or older.\n\nIncome must exceed 30k.");
}

#[test]
fn rtf_from_officeparser_keeps_headings_lists_and_tables() {
    let markdown = markdown("policy.rtf", Format::Rtf);
    assert!(markdown.starts_with("# Loan Policy\n\nApplicants must be **18+** and *employed*. See"));
    assert!(markdown.contains("## Eligibility"));
    assert!(markdown.contains("- Income > 30k\n- No defaults\n\n1. Submit application\n1. Await review"));
    assert!(markdown.contains("| A | 750 | 3.5% |\n| B | 650 | 5.0% |"));
}

#[test]
fn rtf_from_cocoa_keeps_unicode_nested_lists_and_tables() {
    let markdown = markdown("cocoa.rtf", Format::Rtf);
    assert!(markdown.contains("Café — naïve “quotes” 日本語 €5"));
    assert!(markdown.contains("- Income > 30k\n- No defaults\n  - Nested item\n\n1. Submit application\n1. Await review"));
    assert!(markdown.contains("| **Grade** | **Rate** |\n| --- | --- |\n| A | 3.5% |"));
}

#[test]
fn rtf_word_features() {
    let rtf = r#"{\rtf1\ansi\deff0{\fonttbl{\f0\fswiss\fcharset0 Arial;}{\f1\fnil\fcharset128 MS Mincho;}{\f2\fnil\fcharset0 Arial;}}
{\colortbl;\red0\green0\blue0;}
{\stylesheet{\s0 Normal;}{\s1\outlinelevel0 heading 1;}{\s2 Heading 2;}{\s3 Quote;}}
{\info{\title Secret title}{\author Someone}}
{\header {\pard Page header\par}}
\pard\s1 \uc1\u1055?\u1088?\u1080?\u1074?\u1077?\u1090?\par
\pard\s3 Not a heading\par
\pard\s2 Second {\b bold {\i both}} plain\par
\pard {\f1 \u26085?\u26412?} \uc1\u-10179?\u-8704? {\f2 caf\'e9 \'80} {\field{\*\fldinst HYPERLINK "https://gorules.io"}{\fldrslt GoRules}}\par
\pard {\*\unknowndest hidden}{\pict\pngblip 89504e47}visible\line next\tab col\par
}"#;
    let converted = convert(rtf.as_bytes(), &Limits::default()).unwrap();
    assert_eq!(converted.format, Format::Rtf);
    assert_eq!(
        converted.markdown,
        "# Привет\n\nNot a heading\n\n## Second bold both plain\n\n日本 😀 café € [GoRules](https://gorules.io)\n\nvisible\nnext col"
    );
}

#[test]
fn rtf_limits_and_garbage() {
    let deep = format!("{{\\rtf1 {}x{}}}", "{".repeat(5000), "}".repeat(5000));
    assert!(matches!(convert(deep.as_bytes(), &Limits::default()), Err(Error::Malformed(_))));
    assert!(convert(br"{\rtf1 unterminated {\b group", &Limits::default()).is_ok());
    let many = format!("{{\\rtf1 {}}}", "\\par ".repeat(50));
    let limits = Limits { max_xml_elements: 5, ..Limits::default() };
    assert_eq!(convert(many.as_bytes(), &limits).unwrap_err(), Error::TooLarge("rtf tokens"));
}

#[test]
fn rejects_unknown_input() {
    assert_eq!(convert(b"hello", &Limits::default()).unwrap_err(), Error::Unsupported);
    let unknown_zip = archive(&[("readme.txt", b"hi")]);
    assert_eq!(convert(&unknown_zip, &Limits::default()).unwrap_err(), Error::Unsupported);
}

#[test]
fn rejects_zip_bombs_before_parsing() {
    let zeros = vec![b' '; 30 * 1024 * 1024];
    let bomb = archive(&[("word/document.xml", &zeros)]);
    assert!(bomb.len() < 100 * 1024);
    assert_eq!(convert(&bomb, &Limits::default()).unwrap_err(), Error::TooLarge("uncompressed size"));
}

#[test]
fn rejects_too_many_entries() {
    let names: Vec<String> = (0..20).map(|index| format!("f{index}.xml")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|name| (name.as_str(), &b"x"[..])).collect();
    let limits = Limits { max_zip_entries: 10, ..Limits::default() };
    assert_eq!(convert(&archive(&entries), &limits).unwrap_err(), Error::TooLarge("zip entries"));
}

#[test]
fn enforces_xml_element_and_markdown_limits() {
    let docx = fixture("policy.docx");
    let few_elements = Limits { max_xml_elements: 50, ..Limits::default() };
    assert_eq!(convert(&docx, &few_elements).unwrap_err(), Error::TooLarge("xml elements"));
    let short = Limits { max_markdown_bytes: 20, ..Limits::default() };
    assert_eq!(convert(&docx, &short).unwrap_err(), Error::TooLarge("markdown"));
}

#[test]
fn enforces_table_cell_limit() {
    let limits = Limits { max_table_cells: 5, ..Limits::default() };
    assert_eq!(convert(&fixture("rates.xlsx"), &limits).unwrap_err(), Error::TooLarge("table cells"));
}

#[test]
fn malformed_documents_error_instead_of_panicking() {
    let broken = archive(&[("word/document.xml", b"<w:document><w:body><w:p>")]);
    assert!(convert(&broken, &Limits::default()).is_ok());
    let truncated = &fixture("policy.docx")[..2000];
    assert!(matches!(convert(truncated, &Limits::default()), Err(Error::Malformed(_))));
}

fn record(kind: u16, data: &[u8]) -> Vec<u8> {
    let mut out = kind.to_le_bytes().to_vec();
    out.extend_from_slice(&(data.len() as u16).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn cell(row: u16, column: u16, style: u16, rest: &[u8]) -> Vec<u8> {
    [&row.to_le_bytes()[..], &column.to_le_bytes(), &style.to_le_bytes(), rest].concat()
}

fn compound(workbook: &[u8]) -> Vec<u8> {
    let mut stream = workbook.to_vec();
    stream.resize(stream.len().max(4096).div_ceil(512) * 512, 0);
    let sectors = stream.len() / 512;
    let mut header = vec![0u8; 512];
    header[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    header[0x18..0x1A].copy_from_slice(&0x3Eu16.to_le_bytes());
    header[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes());
    header[0x1C..0x1E].copy_from_slice(&0xFFFEu16.to_le_bytes());
    header[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes());
    header[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
    header[0x2C..0x30].copy_from_slice(&1u32.to_le_bytes());
    header[0x30..0x34].copy_from_slice(&1u32.to_le_bytes());
    header[0x38..0x3C].copy_from_slice(&4096u32.to_le_bytes());
    header[0x3C..0x40].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
    header[0x44..0x48].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
    for index in 0..109 {
        let value: u32 = if index == 0 { 0 } else { 0xFFFF_FFFF };
        header[0x4C + index * 4..0x50 + index * 4].copy_from_slice(&value.to_le_bytes());
    }
    let mut fat = vec![0xFFFF_FFFDu32, 0xFFFF_FFFE];
    for sector in 0..sectors {
        fat.push(if sector + 1 == sectors { 0xFFFF_FFFE } else { (sector + 3) as u32 });
    }
    fat.resize(128, 0xFFFF_FFFF);
    let entry = |name: &str, kind: u8, start: u32, size: u32, child: u32| {
        let mut entry = vec![0u8; 128];
        let units: Vec<u8> = name.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        entry[..units.len()].copy_from_slice(&units);
        entry[0x40..0x42].copy_from_slice(&(units.len() as u16).to_le_bytes());
        entry[0x42] = kind;
        entry[0x44..0x48].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        entry[0x48..0x4C].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        entry[0x4C..0x50].copy_from_slice(&child.to_le_bytes());
        entry[0x74..0x78].copy_from_slice(&start.to_le_bytes());
        entry[0x78..0x7C].copy_from_slice(&size.to_le_bytes());
        entry
    };
    let mut directory = entry("Root Entry", 5, 0xFFFF_FFFE, 0, 1);
    directory.extend(entry("Workbook", 2, 2, stream.len() as u32, 0xFFFF_FFFF));
    directory.resize(512, 0);
    [header, fat.iter().flat_map(|value| value.to_le_bytes()).collect(), directory, stream].concat()
}

fn workbook(globals_extra: &[u8], biff_version: u16) -> Vec<u8> {
    let bof = |kind: u16| record(0x0809, &[&biff_version.to_le_bytes()[..], &kind.to_le_bytes(), &[0; 12]].concat());
    let xf = |format: u16| record(0x00E0, &[&0u16.to_le_bytes()[..], &format.to_le_bytes(), &[0; 16]].concat());
    let mut sst = [&4u32.to_le_bytes()[..], &3u32.to_le_bytes()].concat();
    sst.extend([&5u16.to_le_bytes()[..], &[0], b"Grade"].concat());
    sst.extend([&2u16.to_le_bytes()[..], &[1], &"日本".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>()].concat());
    sst.extend([&20u16.to_le_bytes()[..], &[0], b"SplitAcros"].concat());
    let continued = [&[1u8][..], &"sContinueX".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>()].concat();
    let format = [&164u16.to_le_bytes()[..], &10u16.to_le_bytes(), &[0], b"yyyy-mm-dd"].concat();
    let sheet_name = [&[6u8, 0][..], b"Sheet1"].concat();

    let globals_without_sheet = [bof(0x0005), globals_extra.to_vec(), record(0x041E, &format), xf(0), xf(164), xf(10), record(0x00FC, &sst), record(0x003C, &continued)].concat();
    let boundsheet_len = 4 + 6 + sheet_name.len();
    let eof = record(0x000A, &[]);
    let sheet_offset = (globals_without_sheet.len() + boundsheet_len + eof.len()) as u32;
    let boundsheet = record(0x0085, &[&sheet_offset.to_le_bytes()[..], &[0, 0], &sheet_name].concat());

    let rk = |value: i32| ((value << 2) | 0x02) as u32;
    let sheet = [
        bof(0x0010),
        record(0x00FD, &cell(0, 0, 0, &0u32.to_le_bytes())),
        record(0x00FD, &cell(0, 1, 0, &1u32.to_le_bytes())),
        record(0x00FD, &cell(0, 2, 0, &2u32.to_le_bytes())),
        record(0x0203, &cell(1, 0, 0, &750f64.to_le_bytes())),
        record(0x0203, &cell(1, 1, 2, &0.035f64.to_le_bytes())),
        record(0x0203, &cell(1, 2, 1, &46037f64.to_le_bytes())),
        record(0x00BD, &[&2u16.to_le_bytes()[..], &0u16.to_le_bytes(), &0u16.to_le_bytes(), &rk(1).to_le_bytes(), &0u16.to_le_bytes(), &rk(2).to_le_bytes(), &1u16.to_le_bytes()].concat()),
        record(0x0006, &cell(2, 2, 0, &[0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0])),
        record(0x0207, &[&8u16.to_le_bytes()[..], &[0], b"computed"].concat()),
        record(0x0205, &cell(3, 0, 0, &[1, 0])),
        record(0x0205, &cell(3, 1, 0, &[0x07, 1])),
        eof.clone(),
    ]
    .concat();
    [globals_without_sheet, boundsheet, eof, sheet].concat()
}

#[test]
fn xls_reads_biff8_workbooks() {
    let converted = convert(&compound(&workbook(&[], 0x0600)), &Limits::default()).unwrap();
    assert_eq!(converted.format, Format::Xls);
    assert_eq!(
        converted.markdown,
        "## Sheet1\n\n| Grade | 日本 | SplitAcrossContinueX |\n| --- | --- | --- |\n| 750 | 3.50% | 2026-01-15 |\n| 1 | 2 | computed |\n| TRUE | #DIV/0! |  |"
    );
}

#[test]
fn xls_reports_encryption_old_formats_and_damage() {
    let encrypted = compound(&workbook(&record(0x002F, &[0, 0]), 0x0600));
    assert_eq!(convert(&encrypted, &Limits::default()).unwrap_err(), Error::Encrypted);
    let excel95 = compound(&workbook(&[], 0x0500));
    assert_eq!(convert(&excel95, &Limits::default()).unwrap_err(), Error::Unsupported);
    let valid = compound(&workbook(&[], 0x0600));
    assert!(matches!(convert(&valid[..700], &Limits::default()), Err(Error::Malformed(_))));
    let limits = Limits { max_table_cells: 3, ..Limits::default() };
    assert_eq!(convert(&valid, &limits).unwrap_err(), Error::TooLarge("table cells"));
}
