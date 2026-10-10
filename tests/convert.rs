use std::io::{Cursor, Write};

use into_md::{Error, Format, Limits, convert};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/test/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
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
    let rtf = r#"{\rtf1\ansi\ansicpg1251\deff0{\fonttbl{\f0\fswiss\fcharset204 Arial;}{\f1\fnil\fcharset128 MS Mincho;}}
{\colortbl;\red0\green0\blue0;}
{\stylesheet{\s0 Normal;}{\s1\outlinelevel0 heading 1;}{\s2 Heading 2;}{\s3 Quote;}}
{\info{\title Secret title}{\author Someone}}
{\header {\pard Page header\par}}
\pard\s1 \'cf\'f0\'e8\'e2\'e5\'f2\par
\pard\s3 Not a heading\par
\pard\s2 Second {\b bold {\i both}} plain\par
\pard {\f1 \'93\'fa\'96\'7b} \uc1\u-10179?\u-8704? \'88 {\field{\*\fldinst HYPERLINK "https://gorules.io"}{\fldrslt GoRules}}\par
\pard {\*\unknowndest hidden}{\pict\pngblip 89504e47}visible\line next\tab col\par
}"#;
    let converted = convert(rtf.as_bytes(), &Limits::default()).unwrap();
    assert_eq!(converted.format, Format::Rtf);
    assert_eq!(
        converted.markdown,
        "# Привет\n\nNot a heading\n\n## Second bold both plain\n\n日本 😀 € [GoRules](https://gorules.io)\n\nvisible\nnext col"
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
