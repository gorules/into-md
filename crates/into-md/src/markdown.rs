use crate::{Error, Limits};

#[derive(Default)]
pub struct Inline {
    runs: Vec<Run>,
    links: Vec<(usize, usize, String)>,
    split: bool,
}

struct Run {
    text: String,
    bold: bool,
    italic: bool,
}

impl Inline {
    pub fn push(&mut self, text: &str, bold: bool, italic: bool) {
        if text.is_empty() {
            return;
        }
        match self.runs.last_mut() {
            Some(last) if !self.split && last.bold == bold && last.italic == italic => last.text.push_str(text),
            _ => self.runs.push(Run { text: text.to_string(), bold, italic }),
        }
        self.split = false;
    }

    pub fn begin_link(&mut self) -> usize {
        self.split = true;
        self.runs.len()
    }

    pub fn end_link(&mut self, start: usize, url: &str) {
        let end = self.runs.len();
        self.split = true;
        let url = url.trim();
        if url.is_empty() || start >= end || self.runs[start..end].iter().all(|run| run.text.trim().is_empty()) {
            return;
        }
        self.links.push((start, end, link_target(url)));
    }

    pub fn is_blank(&self) -> bool {
        self.runs.iter().all(|run| run.text.trim().is_empty())
    }

    pub fn plain(&self) -> String {
        collapse(&self.runs.iter().map(|run| run.text.as_str()).collect::<String>())
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (index, run) in self.runs.iter().enumerate() {
            if self.links.iter().any(|(start, _, _)| *start == index) {
                out.push('[');
            }
            render_run(run, &mut out);
            for (_, _, url) in self.links.iter().filter(|(_, end, _)| *end == index + 1) {
                out.push_str("](");
                out.push_str(url);
                out.push(')');
            }
        }
        collapse(&out)
    }
}

fn render_run(run: &Run, out: &mut String) {
    let core = run.text.trim();
    let marker = match (run.bold, run.italic) {
        _ if core.is_empty() => "",
        (true, true) => "***",
        (true, false) => "**",
        (false, true) => "*",
        (false, false) => "",
    };
    if marker.is_empty() {
        out.push_str(&run.text);
        return;
    }
    let lead = &run.text[..run.text.len() - run.text.trim_start().len()];
    let trail = &run.text[run.text.trim_end().len()..];
    out.push_str(lead);
    out.push_str(marker);
    out.push_str(core);
    out.push_str(marker);
    out.push_str(trail);
}

fn link_target(url: &str) -> String {
    url.replace(' ', "%20").replace('(', "%28").replace(')', "%29")
}

fn collapse(text: &str) -> String {
    text.split('\n').map(|line| line.split_whitespace().collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n").trim().to_string()
}

enum Block {
    Paragraph(String),
    Heading(usize, String),
    Item { depth: usize, ordered: bool, text: String },
    Table(Vec<Vec<String>>),
}

pub struct Document {
    blocks: Vec<Block>,
    cells: usize,
    max_cells: usize,
}

impl Document {
    pub fn new(limits: &Limits) -> Self {
        Document { blocks: Vec::new(), cells: 0, max_cells: limits.max_table_cells }
    }

    pub fn paragraph(&mut self, text: String) {
        if !text.trim().is_empty() {
            self.blocks.push(Block::Paragraph(text));
        }
    }

    pub fn heading(&mut self, level: usize, text: String) {
        if !text.trim().is_empty() {
            self.blocks.push(Block::Heading(level.clamp(1, 6), text));
        }
    }

    pub fn item(&mut self, depth: usize, ordered: bool, text: String) {
        if !text.trim().is_empty() {
            self.blocks.push(Block::Item { depth, ordered, text });
        }
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn insert_heading(&mut self, at: usize, level: usize, text: String) {
        self.blocks.insert(at, Block::Heading(level.clamp(1, 6), text));
    }

    pub fn table(&mut self, rows: Vec<Vec<String>>) -> Result<(), Error> {
        let rows: Vec<Vec<String>> = rows.into_iter().filter(|row| row.iter().any(|cell| !cell.trim().is_empty())).collect();
        if rows.is_empty() {
            return Ok(());
        }
        self.count_cells(rows.iter().map(Vec::len).sum())?;
        self.blocks.push(Block::Table(rows));
        Ok(())
    }

    pub fn count_cells(&mut self, cells: usize) -> Result<(), Error> {
        self.cells += cells;
        if self.cells > self.max_cells {
            return Err(Error::TooLarge("table cells"));
        }
        Ok(())
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        let mut previous: Option<bool> = None;
        for block in &self.blocks {
            let continues_list = match (block, previous) {
                (Block::Item { depth, ordered, .. }, Some(previous_ordered)) => *depth > 0 || *ordered == previous_ordered,
                _ => false,
            };
            if !out.is_empty() {
                out.push_str(if continues_list { "\n" } else { "\n\n" });
            }
            previous = match block {
                Block::Item { depth: 0, ordered, .. } => Some(*ordered),
                Block::Item { .. } => previous,
                _ => None,
            };
            match block {
                Block::Paragraph(text) => out.push_str(text),
                Block::Heading(level, text) => {
                    out.push_str(&"#".repeat(*level));
                    out.push(' ');
                    out.push_str(text);
                }
                Block::Item { depth, ordered, text } => {
                    out.push_str(&"  ".repeat(*depth));
                    out.push_str(if *ordered { "1. " } else { "- " });
                    out.push_str(&text.replace('\n', " "));
                }
                Block::Table(rows) => out.push_str(&table(rows)),
            }
        }
        out
    }
}

pub fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let line = |row: &[String]| {
        let cells: Vec<String> = (0..columns).map(|index| row.get(index).map(|cell| cell_text(cell)).unwrap_or_default()).collect();
        format!("| {} |", cells.join(" | "))
    };
    let mut out = vec![line(&rows[0]), format!("|{}", " --- |".repeat(columns))];
    out.extend(rows[1..].iter().map(|row| line(row)));
    out.join("\n")
}

fn cell_text(cell: &str) -> String {
    cell.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "\\|")
}

pub fn finish(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut blank = 0;
    for line in markdown.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        blank = 0;
        out.push_str(line);
    }
    out
}

const MAX_REPEAT: usize = 256;

#[derive(Default)]
pub struct Tables {
    stack: Vec<Vec<Vec<String>>>,
}

impl Tables {
    pub fn active(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn open_table(&mut self) {
        self.stack.push(Vec::new());
    }

    pub fn open_row(&mut self) {
        if let Some(table) = self.stack.last_mut() {
            table.push(Vec::new());
        }
    }

    pub fn open_cell(&mut self) {
        if let Some(row) = self.stack.last_mut().and_then(|table| table.last_mut()) {
            row.push(String::new());
        }
    }

    pub fn repeat_cell(&mut self, times: usize) {
        if let Some(row) = self.stack.last_mut().and_then(|table| table.last_mut())
            && let Some(cell) = row.last().cloned()
            && !cell.is_empty()
        {
            row.extend(std::iter::repeat_n(cell, times.clamp(1, MAX_REPEAT) - 1));
        }
    }

    pub fn append(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        if let Some(cell) = self.stack.last_mut().and_then(|table| table.last_mut()).and_then(|row| row.last_mut()) {
            if !cell.is_empty() {
                cell.push(' ');
            }
            cell.push_str(text.trim());
        }
    }

    pub fn close_table(&mut self) -> Option<Vec<Vec<String>>> {
        let rows = self.stack.pop()?;
        if self.stack.is_empty() {
            return Some(rows);
        }
        let flattened = rows.iter().map(|row| row.join(" ")).collect::<Vec<_>>().join(" ");
        self.append(&flattened);
        None
    }
}
