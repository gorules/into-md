use std::collections::HashMap;

use crate::archive::{Archive, join_path as join};
use crate::markdown::{Document, Inline, Tables};
use crate::xml::{Node, attr, name, prefixed_attr, relationships, walk};
use crate::{Error, Limits};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Title,
    Body,
    Text,
}

pub fn convert(archive: &mut Archive<'_>, limits: &Limits) -> Result<String, Error> {
    let presentation = archive.require("ppt/presentation.xml")?;
    let rels = match archive.read("ppt/_rels/presentation.xml.rels")? {
        Some(xml) => relationships(&xml, limits)?,
        None => Default::default(),
    };

    let mut slide_ids = Vec::new();
    walk(&presentation, limits, |node| {
        if let Node::Open(start) = node
            && name(start) == b"sldId"
            && let Some(id) = prefixed_attr(start, b"id")
        {
            slide_ids.push(id);
        }
        Ok(())
    })?;

    let mut document = Document::new(limits);
    for (index, id) in slide_ids.iter().enumerate() {
        let Some(target) = rels.get(id) else { continue };
        let path = join("ppt/presentation.xml", target);
        let Some(xml) = archive.read(&path)? else { continue };
        let slide_rels = match archive.read(&rels_path(&path))? {
            Some(rels) => relationships(&rels, limits)?,
            None => Default::default(),
        };
        slide(&xml, &slide_rels, index + 1, limits, &mut document)?;
        let notes_path = slide_rels.values().find(|target| target.contains("notesSlide")).map(|target| join(&path, target));
        if let Some(xml) = notes_path.map(|path| archive.read(&path)).transpose()?.flatten() {
            let paragraphs = notes(&xml, limits)?;
            if !paragraphs.is_empty() {
                document.paragraph(format!("> **Notes:** {}", paragraphs.join("\n>\n> ")));
            }
        }
    }
    Ok(document.render())
}

fn notes(xml: &[u8], limits: &Limits) -> Result<Vec<String>, Error> {
    let mut paragraphs = Vec::new();
    let mut in_body = false;
    let mut in_text = false;
    let mut current: Option<Inline> = None;
    walk(xml, limits, |node| {
        match node {
            Node::Open(start) => match name(start) {
                b"sp" => in_body = false,
                b"ph" => in_body = attr(start, b"type").as_deref() == Some("body"),
                b"p" if in_body => current = Some(Inline::default()),
                b"t" => in_text = true,
                _ => {}
            },
            Node::Close(b"t") => in_text = false,
            Node::Close(b"p") => {
                if let Some(inline) = current.take()
                    && !inline.is_blank()
                {
                    paragraphs.push(inline.plain());
                }
            }
            Node::Text(text) if in_text => {
                if let Some(inline) = current.as_mut() {
                    inline.push(text, false, false);
                }
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok(paragraphs)
}

fn rels_path(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((directory, file)) => format!("{directory}/_rels/{file}.rels"),
        None => format!("_rels/{path}.rels"),
    }
}

fn slide(xml: &[u8], rels: &HashMap<String, String>, number: usize, limits: &Limits, document: &mut Document) -> Result<(), Error> {
    let start_at = document.len();
    let mut title = Inline::default();
    let mut tables = Tables::default();
    let mut shape = Shape::Text;
    let mut paragraph: Option<(Inline, usize)> = None;
    let (mut bold, mut italic, mut in_text) = (false, false, false);
    let mut link: Option<(usize, String)> = None;

    walk(xml, limits, |node| {
        match node {
            Node::Open(start) => match name(start) {
                b"sp" => shape = Shape::Text,
                b"ph" => {
                    shape = match attr(start, b"type").as_deref() {
                        Some("title" | "ctrTitle") => Shape::Title,
                        Some("body" | "obj") | None => Shape::Body,
                        _ => Shape::Text,
                    }
                }
                b"p" => paragraph = Some((Inline::default(), 0)),
                b"pPr" => {
                    if let Some((_, level)) = paragraph.as_mut() {
                        *level = attr(start, b"lvl").and_then(|v| v.parse().ok()).unwrap_or(0);
                    }
                }
                b"rPr" => {
                    bold = matches!(attr(start, b"b").as_deref(), Some("1" | "true"));
                    italic = matches!(attr(start, b"i").as_deref(), Some("1" | "true"));
                }
                b"r" | b"fld" => {
                    bold = false;
                    italic = false;
                }
                b"hlinkClick" => {
                    if let (Some((inline, _)), Some(url)) =
                        (paragraph.as_mut(), prefixed_attr(start, b"id").and_then(|id| rels.get(&id).cloned()))
                    {
                        link = Some((inline.begin_link(), url));
                    }
                }
                b"t" => in_text = true,
                b"br" => {
                    if let Some((inline, _)) = paragraph.as_mut() {
                        inline.push("\n", false, false);
                    }
                }
                b"tbl" => tables.open_table(),
                b"tr" => tables.open_row(),
                b"tc" => tables.open_cell(),
                _ => {}
            },
            Node::Close(local) => match local {
                b"t" => in_text = false,
                b"r" | b"fld" => {
                    if let (Some((start, url)), Some((inline, _))) = (link.take(), paragraph.as_mut()) {
                        inline.end_link(start, &url);
                    }
                }
                b"sp" => shape = Shape::Text,
                b"p" => {
                    if let Some((inline, level)) = paragraph.take()
                        && !inline.is_blank()
                    {
                        if tables.active() {
                            tables.append(&inline.render());
                        } else {
                            match shape {
                                Shape::Title => {
                                    if !title.is_blank() {
                                        title.push(" ", false, false);
                                    }
                                    title.push(&inline.plain(), false, false);
                                }
                                Shape::Body => document.item(level, false, inline.render()),
                                Shape::Text => document.paragraph(inline.render()),
                            }
                        }
                    }
                }
                b"tbl" => {
                    if let Some(rows) = tables.close_table() {
                        document.table(rows)?;
                    }
                }
                _ => {}
            },
            Node::Text(text) => {
                if in_text && let Some((inline, _)) = paragraph.as_mut() {
                    inline.push(text, bold, italic);
                }
            }
        }
        Ok(())
    })?;

    let heading = if title.is_blank() { format!("Slide {number}") } else { format!("Slide {number}: {}", title.plain()) };
    document.insert_heading(start_at, 2, heading);
    Ok(())
}
