use crate::archive::Archive;
use crate::markdown::{Document, Inline, Tables};
use crate::xml::{Node, attr, name, walk};
use crate::{Error, Limits};

enum Kind {
    Paragraph,
    Heading(usize),
}

pub fn convert(archive: &mut Archive<'_>, spreadsheet: bool, limits: &Limits) -> Result<String, Error> {
    let xml = archive.require("content.xml")?;
    let mut document = Document::new(limits);
    let mut tables = Tables::default();
    let mut paragraphs: Vec<(Kind, Inline)> = Vec::new();
    let mut list_depth = 0usize;
    let mut slide = 0usize;
    let mut cell_repeat = 1usize;
    let mut skip_depth = 0usize;
    let mut links: Vec<Option<(usize, String)>> = Vec::new();
    let mut sheet_names: Vec<Option<String>> = Vec::new();

    walk(&xml, limits, |node| {
        if skip_depth > 0 {
            match node {
                Node::Open(_) => skip_depth += 1,
                Node::Close(_) => skip_depth -= 1,
                Node::Text(_) => {}
            }
            return Ok(());
        }
        match node {
            Node::Open(start) => match name(start) {
                b"annotation" | b"notes" | b"tracked-changes" => skip_depth = 1,
                b"page" => {
                    slide += 1;
                    let title = attr(start, b"name").filter(|n| !n.starts_with("page") && !n.starts_with("Slide"));
                    let heading = match title {
                        Some(title) => format!("Slide {slide}: {title}"),
                        None => format!("Slide {slide}"),
                    };
                    document.heading(2, heading);
                }
                b"h" => {
                    let level = attr(start, b"outline-level").and_then(|v| v.parse().ok()).unwrap_or(1);
                    paragraphs.push((Kind::Heading(level), Inline::default()));
                }
                b"p" => paragraphs.push((Kind::Paragraph, Inline::default())),
                b"a" => {
                    let url = attr(start, b"href").filter(|href| !href.starts_with('#'));
                    links.push(url.and_then(|url| paragraphs.last_mut().map(|(_, inline)| (inline.begin_link(), url))));
                }
                b"list" => list_depth += 1,
                b"s" => {
                    let count = attr(start, b"c").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(16);
                    push_text(&mut paragraphs, &" ".repeat(count));
                }
                b"tab" => push_text(&mut paragraphs, " "),
                b"line-break" => push_text(&mut paragraphs, "\n"),
                b"table" => {
                    sheet_names.push(attr(start, b"name").filter(|_| spreadsheet));
                    tables.open_table();
                }
                b"table-row" => tables.open_row(),
                b"table-cell" | b"covered-table-cell" => {
                    cell_repeat = attr(start, b"number-columns-repeated").and_then(|v| v.parse().ok()).unwrap_or(1);
                    tables.open_cell();
                }
                _ => {}
            },
            Node::Close(local) => match local {
                b"list" => list_depth = list_depth.saturating_sub(1),
                b"a" => {
                    if let (Some(Some((start, url))), Some((_, inline))) = (links.pop(), paragraphs.last_mut()) {
                        inline.end_link(start, &url);
                    }
                }
                b"table-cell" | b"covered-table-cell" => tables.repeat_cell(cell_repeat),
                b"table" => {
                    let sheet_name = sheet_names.pop().flatten();
                    if let Some(rows) = tables.close_table() {
                        let has_content = rows.iter().flatten().any(|cell| !cell.trim().is_empty());
                        if let Some(sheet_name) = sheet_name.filter(|_| has_content) {
                            document.heading(2, sheet_name);
                        }
                        document.table(rows)?;
                    }
                }
                b"h" | b"p" => {
                    if let Some((kind, inline)) = paragraphs.pop()
                        && !inline.is_blank()
                    {
                        if tables.active() {
                            tables.append(&inline.render());
                        } else {
                            match kind {
                                Kind::Heading(level) => document.heading(level, inline.plain()),
                                Kind::Paragraph if list_depth > 0 => document.item(list_depth - 1, false, inline.render()),
                                Kind::Paragraph => document.paragraph(inline.render()),
                            }
                        }
                    }
                }
                _ => {}
            },
            Node::Text(text) => push_text(&mut paragraphs, text),
        }
        Ok(())
    })?;

    Ok(document.render())
}

fn push_text(paragraphs: &mut [(Kind, Inline)], text: &str) {
    if let Some((_, inline)) = paragraphs.last_mut() {
        inline.push(text, false, false);
    }
}
