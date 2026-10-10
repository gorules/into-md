use std::collections::HashMap;

use crate::archive::Archive;
use crate::markdown::{Document, Inline, Tables};
use crate::xml::{Node, attr, is_on, name, prefixed_attr, relationships, walk};
use crate::{Error, Limits};

#[derive(Default)]
struct Paragraph {
    inline: Inline,
    style: Option<String>,
    outline: Option<usize>,
    num_id: Option<String>,
    level: usize,
}

#[derive(Default)]
struct Numbering {
    ordered: HashMap<(String, usize), bool>,
    abstract_of: HashMap<String, String>,
}

impl Numbering {
    fn ordered(&self, num_id: &str, level: usize) -> bool {
        self.abstract_of
            .get(num_id)
            .and_then(|abstract_id| self.ordered.get(&(abstract_id.clone(), level)))
            .copied()
            .unwrap_or(false)
    }
}

pub fn convert(archive: &mut Archive<'_>, limits: &Limits) -> Result<String, Error> {
    let headings = match archive.read("word/styles.xml")? {
        Some(xml) => heading_styles(&xml, limits)?,
        None => HashMap::new(),
    };
    let numbering = match archive.read("word/numbering.xml")? {
        Some(xml) => numbering(&xml, limits)?,
        None => Numbering::default(),
    };
    let rels = match archive.read("word/_rels/document.xml.rels")? {
        Some(xml) => relationships(&xml, limits)?,
        None => HashMap::new(),
    };
    let xml = archive.require("word/document.xml")?;

    let mut document = Document::new(limits);
    let mut tables = Tables::default();
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let (mut bold, mut italic, mut in_run, mut in_text) = (false, false, false, false);
    let mut fallback_depth = 0usize;
    let mut links: Vec<Option<(usize, String)>> = Vec::new();

    walk(&xml, limits, |node| {
        if fallback_depth > 0 {
            match node {
                Node::Open(start) if name(start) == b"Fallback" => fallback_depth += 1,
                Node::Close(b"Fallback") => fallback_depth -= 1,
                _ => {}
            }
            return Ok(());
        }
        match node {
            Node::Open(start) => match name(start) {
                b"Fallback" => fallback_depth = 1,
                b"p" => paragraphs.push(Paragraph::default()),
                b"pStyle" => set(&mut paragraphs, |p| p.style = attr(start, b"val")),
                b"outlineLvl" => set(&mut paragraphs, |p| {
                    p.outline = attr(start, b"val").and_then(|v| v.parse::<usize>().ok()).filter(|v| *v < 9).map(|v| v + 1)
                }),
                b"numId" => set(&mut paragraphs, |p| p.num_id = attr(start, b"val").filter(|v| v != "0")),
                b"ilvl" => set(&mut paragraphs, |p| p.level = attr(start, b"val").and_then(|v| v.parse().ok()).unwrap_or(0)),
                b"r" => {
                    in_run = true;
                    bold = false;
                    italic = false;
                }
                b"b" if in_run => bold = is_on(start),
                b"i" if in_run => italic = is_on(start),
                b"t" if in_run => in_text = true,
                b"tab" if in_run => push_text(&mut paragraphs, " ", bold, italic),
                b"br" | b"cr" if in_run => push_text(&mut paragraphs, "\n", bold, italic),
                b"hyperlink" => {
                    let url = prefixed_attr(start, b"id").and_then(|id| rels.get(&id).cloned());
                    links.push(url.and_then(|url| paragraphs.last_mut().map(|p| (p.inline.begin_link(), url))));
                }
                b"tbl" => tables.open_table(),
                b"tr" => tables.open_row(),
                b"tc" => tables.open_cell(),
                _ => {}
            },
            Node::Close(local) => match local {
                b"t" => in_text = false,
                b"r" => in_run = false,
                b"hyperlink" => {
                    if let Some(Some((start, url))) = links.pop() {
                        set(&mut paragraphs, |p| p.inline.end_link(start, &url));
                    }
                }
                b"p" => {
                    if let Some(paragraph) = paragraphs.pop() {
                        emit(paragraph, &headings, &numbering, &mut tables, &mut document);
                    }
                }
                b"tbl" => {
                    if let Some(rows) = tables.close_table() {
                        document.table(rows)?;
                    }
                }
                _ => {}
            },
            Node::Text(text) if in_text => push_text(&mut paragraphs, text, bold, italic),
            Node::Text(_) => {}
        }
        Ok(())
    })?;

    Ok(document.render())
}

fn set(paragraphs: &mut [Paragraph], update: impl FnOnce(&mut Paragraph)) {
    if let Some(paragraph) = paragraphs.last_mut() {
        update(paragraph);
    }
}

fn push_text(paragraphs: &mut [Paragraph], text: &str, bold: bool, italic: bool) {
    set(paragraphs, |p| p.inline.push(text, bold, italic));
}

fn emit(paragraph: Paragraph, headings: &HashMap<String, usize>, numbering: &Numbering, tables: &mut Tables, document: &mut Document) {
    if paragraph.inline.is_blank() {
        return;
    }
    if tables.active() {
        tables.append(&paragraph.inline.render());
        return;
    }
    let heading = paragraph.outline.or_else(|| paragraph.style.as_ref().and_then(|style| headings.get(style).copied()));
    if let Some(level) = heading {
        document.heading(level, paragraph.inline.plain());
    } else if let Some(num_id) = &paragraph.num_id {
        document.item(paragraph.level, numbering.ordered(num_id, paragraph.level), paragraph.inline.render());
    } else {
        document.paragraph(paragraph.inline.render());
    }
}

fn heading_styles(xml: &[u8], limits: &Limits) -> Result<HashMap<String, usize>, Error> {
    let mut headings = HashMap::new();
    let mut current: Option<(String, Option<usize>)> = None;
    walk(xml, limits, |node| {
        match node {
            Node::Open(start) => match name(start) {
                b"style" => current = attr(start, b"styleId").map(|id| (id, None)),
                b"name" => {
                    if let Some((_, level)) = current.as_mut() {
                        let style_name = attr(start, b"val").unwrap_or_default().to_ascii_lowercase();
                        if style_name == "title" {
                            *level = Some(1);
                        } else if let Some(n) = style_name.strip_prefix("heading ").and_then(|n| n.trim().parse::<usize>().ok()) {
                            *level = Some(n);
                        }
                    }
                }
                b"outlineLvl" => {
                    if let Some((_, level)) = current.as_mut()
                        && level.is_none()
                    {
                        *level = attr(start, b"val").and_then(|v| v.parse::<usize>().ok()).filter(|v| *v < 9).map(|v| v + 1);
                    }
                }
                _ => {}
            },
            Node::Close(b"style") => {
                if let Some((id, Some(level))) = current.take() {
                    headings.insert(id, level);
                }
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok(headings)
}

fn numbering(xml: &[u8], limits: &Limits) -> Result<Numbering, Error> {
    let mut numbering = Numbering::default();
    let (mut abstract_id, mut level, mut num_id): (Option<String>, usize, Option<String>) = (None, 0, None);
    walk(xml, limits, |node| {
        if let Node::Open(start) = node {
            match name(start) {
                b"abstractNum" => abstract_id = attr(start, b"abstractNumId"),
                b"lvl" => level = attr(start, b"ilvl").and_then(|v| v.parse().ok()).unwrap_or(0),
                b"numFmt" => {
                    if let Some(id) = &abstract_id {
                        let format = attr(start, b"val").unwrap_or_default();
                        numbering.ordered.insert((id.clone(), level), !matches!(format.as_str(), "bullet" | "none" | ""));
                    }
                }
                b"num" => {
                    abstract_id = None;
                    num_id = attr(start, b"numId");
                }
                b"abstractNumId" => {
                    if let (Some(id), Some(target)) = (&num_id, attr(start, b"val")) {
                        numbering.abstract_of.insert(id.clone(), target);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })?;
    Ok(numbering)
}
