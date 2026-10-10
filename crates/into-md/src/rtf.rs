use std::collections::HashMap;

use crate::markdown::{Document, Inline};
use crate::{Error, Limits, malformed};

const MAX_DEPTH: usize = 1024;

enum Token<'a> {
    Open,
    Close,
    Word(&'a [u8], Option<i32>),
    Symbol(u8),
    Byte(u8),
    Text(&'a [u8]),
}

struct Lexer<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Lexer<'a> {
    fn next(&mut self) -> Option<Token<'a>> {
        loop {
            let byte = *self.bytes.get(self.at)?;
            match byte {
                b'{' => {
                    self.at += 1;
                    return Some(Token::Open);
                }
                b'}' => {
                    self.at += 1;
                    return Some(Token::Close);
                }
                b'\r' | b'\n' => self.at += 1,
                b'\\' => return Some(self.control()),
                _ => {
                    let start = self.at;
                    while self.at < self.bytes.len() && !matches!(self.bytes[self.at], b'\\' | b'{' | b'}' | b'\r' | b'\n') {
                        self.at += 1;
                    }
                    return Some(Token::Text(&self.bytes[start..self.at]));
                }
            }
        }
    }

    fn control(&mut self) -> Token<'a> {
        self.at += 1;
        let Some(&first) = self.bytes.get(self.at) else { return Token::Symbol(b'\\') };
        if first == b'\'' {
            let hex = self.bytes.get(self.at + 1..self.at + 3).and_then(|pair| std::str::from_utf8(pair).ok());
            if let Some(value) = hex.and_then(|pair| u8::from_str_radix(pair, 16).ok()) {
                self.at += 3;
                return Token::Byte(value);
            }
            self.at += 1;
            return Token::Symbol(b'\'');
        }
        if !first.is_ascii_alphabetic() {
            self.at += 1;
            return Token::Symbol(first);
        }
        let start = self.at;
        while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_alphabetic() && self.at - start < 32 {
            self.at += 1;
        }
        let word = &self.bytes[start..self.at];
        let number_start = self.at;
        if self.bytes.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_digit() && self.at - number_start < 11 {
            self.at += 1;
        }
        let param = std::str::from_utf8(&self.bytes[number_start..self.at]).ok().and_then(|digits| digits.parse::<i32>().ok());
        if param.is_none() {
            self.at = number_start;
        }
        if self.bytes.get(self.at) == Some(&b' ') {
            self.at += 1;
        }
        if word == b"bin" {
            self.at = self.at.saturating_add(param.unwrap_or(0).max(0) as usize).min(self.bytes.len());
        }
        Token::Word(word, param)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Destination {
    Body,
    Skip,
    FontTable,
    Stylesheet,
    ListTable,
    ListOverrides,
    FieldInstruction,
}

#[derive(Clone, Copy)]
struct Group {
    destination: Destination,
    bold: bool,
    italic: bool,
    font: i32,
    unicode_skip: usize,
}

#[derive(Default)]
struct Paragraph {
    inline: Inline,
    style: Option<i32>,
    outline: Option<usize>,
    list: Option<(i32, usize)>,
    numbered: Option<bool>,
    in_table: bool,
}

impl Paragraph {
    fn reset(&mut self) {
        let inline = std::mem::take(&mut self.inline);
        *self = Paragraph { inline, ..Paragraph::default() };
    }
}

#[derive(Default)]
struct Tables {
    fonts: HashMap<i32, u16>,
    styles: HashMap<i32, usize>,
    style_names: usize,
    lists: HashMap<i32, Vec<i32>>,
    overrides: HashMap<i32, i32>,
}

struct Reader {
    groups: Vec<Group>,
    group_start: bool,
    ignorable: bool,
    default_code_page: u16,
    tables: Tables,
    pending: Vec<u8>,
    pending_code_page: u16,
    skip_chars: usize,
    high_surrogate: Option<u16>,
    paragraph: Paragraph,
    cell: String,
    row: Vec<String>,
    rows: Vec<Vec<String>>,
    document: Document,
    entry_number: Option<i32>,
    entry_text: String,
    entry_outline: Option<usize>,
    list_id: Option<i32>,
    list_levels: Vec<i32>,
    override_list: Option<i32>,
    field_instruction: String,
    link: Option<(usize, usize, String)>,
}

pub fn convert(bytes: &[u8], limits: &Limits) -> Result<String, Error> {
    let mut lexer = Lexer { bytes, at: 0 };
    let mut reader = Reader {
        groups: vec![Group { destination: Destination::Body, bold: false, italic: false, font: -1, unicode_skip: 1 }],
        group_start: false,
        ignorable: false,
        default_code_page: 1252,
        tables: Tables::default(),
        pending: Vec::new(),
        pending_code_page: 1252,
        skip_chars: 0,
        high_surrogate: None,
        paragraph: Paragraph::default(),
        cell: String::new(),
        row: Vec::new(),
        rows: Vec::new(),
        document: Document::new(limits),
        entry_number: None,
        entry_text: String::new(),
        entry_outline: None,
        list_id: None,
        list_levels: Vec::new(),
        override_list: None,
        field_instruction: String::new(),
        link: None,
    };
    let mut tokens = 0usize;
    while let Some(token) = lexer.next() {
        tokens += 1;
        if tokens > limits.max_xml_elements.saturating_mul(8) {
            return Err(Error::TooLarge("rtf tokens"));
        }
        reader.token(token)?;
    }
    reader.flush_text();
    reader.end_paragraph()?;
    reader.flush_table()?;
    Ok(reader.document.render())
}

impl Reader {
    fn group(&self) -> &Group {
        self.groups.last().expect("root group")
    }

    fn group_mut(&mut self) -> &mut Group {
        self.groups.last_mut().expect("root group")
    }

    fn token(&mut self, token: Token<'_>) -> Result<(), Error> {
        let at_group_start = std::mem::take(&mut self.group_start);
        match token {
            Token::Open => {
                self.flush_text();
                if self.groups.len() >= MAX_DEPTH {
                    return Err(malformed("rtf nesting too deep"));
                }
                let group = *self.group();
                self.groups.push(group);
                self.group_start = true;
                self.ignorable = false;
            }
            Token::Close => {
                self.flush_text();
                if let Some((_, start, url)) = self.link.take_if(|(depth, _, _)| *depth == self.groups.len()) {
                    self.paragraph.inline.end_link(start, &url);
                }
                self.close_group();
                if self.groups.len() > 1 {
                    self.groups.pop();
                }
            }
            Token::Symbol(b'*') if at_group_start => {
                self.ignorable = true;
                self.group_start = true;
            }
            Token::Word(word, param) => {
                if at_group_start && self.enter_destination(word) {
                    return Ok(());
                }
                if at_group_start && self.ignorable {
                    self.group_mut().destination = Destination::Skip;
                    return Ok(());
                }
                self.word(word, param)?;
            }
            Token::Symbol(symbol) => self.symbol(symbol)?,
            Token::Byte(_) | Token::Text(_) if self.group().destination == Destination::Skip => {}
            Token::Byte(byte) => {
                if self.skip_chars > 0 {
                    self.skip_chars -= 1;
                } else {
                    self.push_byte(byte);
                }
            }
            Token::Text(text) => {
                let skipped = self.skip_chars.min(text.len());
                self.skip_chars -= skipped;
                for &byte in &text[skipped..] {
                    self.push_byte(byte);
                }
            }
        }
        Ok(())
    }

    fn enter_destination(&mut self, word: &[u8]) -> bool {
        let destination = match word {
            b"fonttbl" => Destination::FontTable,
            b"fldinst" => Destination::FieldInstruction,
            b"fldrslt" => {
                if let Some(url) = hyperlink_target(&self.field_instruction) {
                    let start = self.paragraph.inline.begin_link();
                    self.link = Some((self.groups.len(), start, url));
                }
                return false;
            }
            b"stylesheet" => Destination::Stylesheet,
            b"listtable" => Destination::ListTable,
            b"listoverridetable" => Destination::ListOverrides,
            b"colortbl" | b"info" | b"pict" | b"object" | b"header" | b"headerl" | b"headerr" | b"headerf" | b"footer" | b"footerl"
            | b"footerr" | b"footerf" | b"listtext" | b"pntext" | b"pntxta" | b"pntxtb" | b"footnote" | b"annotation"
            | b"atnauthor" | b"atnid" | b"themedata" | b"colorschememapping" | b"datastore" | b"latentstyles" | b"rsidtbl"
            | b"generator" | b"xmlnstbl" | b"filetbl" | b"revtbl" | b"pgdsctbl" | b"nonshppict" | b"shpinst" | b"bkmkstart"
            | b"bkmkend" | b"template" | b"userprops" | b"docvar" | b"listpicture" => Destination::Skip,
            _ => return false,
        };
        let current = self.group().destination;
        self.group_mut().destination = if current == Destination::Skip { Destination::Skip } else { destination };
        true
    }

    fn close_group(&mut self) {
        match self.group().destination {
            Destination::Stylesheet if self.groups.len() >= 2 => {
                if let Some(number) = self.entry_number.take() {
                    let name = std::mem::take(&mut self.entry_text);
                    let name = name.trim().trim_end_matches(';').trim().to_ascii_lowercase();
                    self.tables.style_names += 1;
                    let level = if name == "title" {
                        Some(1)
                    } else {
                        name.strip_prefix("heading ").and_then(|n| n.trim().parse::<usize>().ok()).or(self.entry_outline)
                    };
                    if let Some(level) = level {
                        self.tables.styles.insert(number, level);
                    }
                }
                self.entry_text.clear();
                self.entry_outline = None;
            }
            Destination::ListTable => {
                if let Some(id) = self.list_id.take() {
                    self.tables.lists.insert(id, std::mem::take(&mut self.list_levels));
                }
            }
            Destination::ListOverrides => {
                if let (Some(list), Some(number)) = (self.override_list, self.entry_number) {
                    self.tables.overrides.insert(number, list);
                    self.override_list = None;
                    self.entry_number = None;
                }
            }
            _ => {}
        }
    }

    fn word(&mut self, word: &[u8], param: Option<i32>) -> Result<(), Error> {
        let destination = self.group().destination;
        match destination {
            Destination::Skip | Destination::FieldInstruction => return Ok(()),
            Destination::FontTable => {
                match word {
                    b"f" => self.entry_number = param,
                    b"fcharset" => {
                        if let (Some(font), Some(code_page)) = (self.entry_number, param.and_then(charset_code_page)) {
                            self.tables.fonts.insert(font, code_page);
                        }
                    }
                    b"cpg" => {
                        if let (Some(font), Some(code_page)) = (self.entry_number, param.and_then(|cp| u16::try_from(cp).ok())) {
                            self.tables.fonts.insert(font, code_page);
                        }
                    }
                    _ => {}
                }
                return Ok(());
            }
            Destination::Stylesheet => {
                match word {
                    b"s" => self.entry_number = param,
                    b"cs" | b"ds" | b"ts" => self.entry_number = None,
                    b"outlinelevel" => self.entry_outline = param.filter(|level| (0..9).contains(level)).map(|level| level as usize + 1),
                    _ => {}
                }
                return Ok(());
            }
            Destination::ListTable => {
                match word {
                    b"listid" => self.list_id = param,
                    b"levelnfc" => self.list_levels.push(param.unwrap_or(0)),
                    _ => {}
                }
                return Ok(());
            }
            Destination::ListOverrides => {
                match word {
                    b"listid" => self.override_list = param,
                    b"ls" => self.entry_number = param,
                    _ => {}
                }
                return Ok(());
            }
            Destination::Body => {}
        }

        match word {
            b"field" => self.field_instruction.clear(),
            b"ansicpg" => {
                if let Some(code_page) = param.and_then(|cp| u16::try_from(cp).ok()) {
                    self.default_code_page = code_page;
                }
            }
            b"f" | b"deff" => {
                self.flush_text();
                self.group_mut().font = param.unwrap_or(-1);
            }
            b"uc" => self.group_mut().unicode_skip = param.unwrap_or(1).max(0) as usize,
            b"u" => {
                self.flush_text();
                let value = param.unwrap_or(0);
                let unit = if value < 0 { (value + 65536) as u16 } else { value as u16 };
                self.push_unit(unit);
                self.skip_chars = self.group().unicode_skip;
            }
            b"b" => self.set_bold(param != Some(0)),
            b"i" => self.set_italic(param != Some(0)),
            b"plain" => {
                self.set_bold(false);
                self.set_italic(false);
            }
            b"par" | b"sect" | b"page" => {
                self.flush_text();
                self.end_paragraph()?;
            }
            b"pard" => {
                self.flush_text();
                self.paragraph.reset();
            }
            b"s" => self.paragraph.style = param,
            b"outlinelevel" => self.paragraph.outline = param.filter(|level| (0..9).contains(level)).map(|level| level as usize + 1),
            b"ls" => self.paragraph.list = param.map(|list| (list, self.paragraph.list.map_or(0, |(_, level)| level))),
            b"ilvl" => {
                let level = param.unwrap_or(0).clamp(0, 8) as usize;
                self.paragraph.list = Some((self.paragraph.list.map_or(0, |(list, _)| list), level));
            }
            b"pnbullet" => self.paragraph.numbered = Some(false),
            b"pndec" | b"pnucrm" | b"pnlcrm" | b"pnucltr" | b"pnlcltr" => self.paragraph.numbered = Some(true),
            b"intbl" => self.paragraph.in_table = true,
            b"cell" => {
                self.flush_text();
                self.end_cell();
            }
            b"row" => {
                self.flush_text();
                self.end_cell_if_open();
                self.rows.push(std::mem::take(&mut self.row));
            }
            b"nestcell" | b"nestrow" => self.text(" "),
            b"tab" => self.text(" "),
            b"line" => self.text("\n"),
            b"emdash" => self.text("—"),
            b"endash" => self.text("–"),
            b"lquote" => self.text("‘"),
            b"rquote" => self.text("’"),
            b"ldblquote" => self.text("“"),
            b"rdblquote" => self.text("”"),
            b"bullet" => self.text("•"),
            b"emspace" | b"enspace" | b"qmspace" => self.text(" "),
            _ => {}
        }
        Ok(())
    }

    fn symbol(&mut self, symbol: u8) -> Result<(), Error> {
        if self.skip_chars > 0 {
            self.skip_chars -= 1;
            return Ok(());
        }
        match symbol {
            b'\\' | b'{' | b'}' => self.push_byte(symbol),
            b'~' => self.text("\u{a0}"),
            b'_' => self.text("-"),
            b'\n' | b'\r' if self.group().destination == Destination::Body => {
                self.flush_text();
                self.end_paragraph()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn set_bold(&mut self, value: bool) {
        self.flush_text();
        self.group_mut().bold = value;
    }

    fn set_italic(&mut self, value: bool) {
        self.flush_text();
        self.group_mut().italic = value;
    }

    fn current_code_page(&self) -> u16 {
        self.tables.fonts.get(&self.group().font).copied().unwrap_or(self.default_code_page)
    }

    fn push_byte(&mut self, byte: u8) {
        let code_page = self.current_code_page();
        if !self.pending.is_empty() && self.pending_code_page != code_page {
            self.flush_text();
        }
        self.pending_code_page = code_page;
        self.pending.push(byte);
    }

    fn push_unit(&mut self, unit: u16) {
        if (0xD800..0xDC00).contains(&unit) {
            self.high_surrogate = Some(unit);
            return;
        }
        let decoded = match self.high_surrogate.take() {
            Some(high) if (0xDC00..0xE000).contains(&unit) => {
                char::from_u32(0x10000 + (((high as u32) - 0xD800) << 10) + ((unit as u32) - 0xDC00))
            }
            _ => char::from_u32(unit as u32),
        };
        if let Some(ch) = decoded {
            self.text(ch.encode_utf8(&mut [0; 4]));
        }
    }

    fn flush_text(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.pending);
        let decoded = crate::text::decode(self.pending_code_page, &bytes);
        self.append(&decoded);
    }

    fn text(&mut self, text: &str) {
        self.flush_text();
        self.append(text);
    }

    fn append(&mut self, text: &str) {
        match self.group().destination {
            Destination::Body => {
                let (bold, italic) = (self.group().bold, self.group().italic);
                self.paragraph.inline.push(text, bold, italic);
            }
            Destination::Stylesheet => self.entry_text.push_str(text),
            Destination::FieldInstruction => self.field_instruction.push_str(text),
            _ => {}
        }
    }

    fn end_cell(&mut self) {
        let text = std::mem::take(&mut self.paragraph.inline).render();
        if !text.is_empty() {
            if !self.cell.is_empty() {
                self.cell.push(' ');
            }
            self.cell.push_str(&text);
        }
        self.row.push(std::mem::take(&mut self.cell));
    }

    fn end_cell_if_open(&mut self) {
        if !self.paragraph.inline.is_blank() || !self.cell.is_empty() {
            self.end_cell();
        }
    }

    fn flush_table(&mut self) -> Result<(), Error> {
        if !self.row.is_empty() {
            self.rows.push(std::mem::take(&mut self.row));
        }
        if self.rows.is_empty() {
            return Ok(());
        }
        let rows = std::mem::take(&mut self.rows);
        self.document.table(rows)
    }

    fn end_paragraph(&mut self) -> Result<(), Error> {
        let inline = std::mem::take(&mut self.paragraph.inline);
        if self.paragraph.in_table {
            let text = inline.render();
            if !text.is_empty() {
                if !self.cell.is_empty() {
                    self.cell.push(' ');
                }
                self.cell.push_str(&text);
            }
            return Ok(());
        }
        if inline.is_blank() {
            return Ok(());
        }
        self.flush_table()?;

        if let Some(level) = self.heading_level() {
            self.document.heading(level, inline.plain());
        } else if let Some((list, level)) = self.paragraph.list.filter(|(list, _)| *list > 0) {
            let numbered = self.list_numbered(list, level);
            self.document.item(level, numbered, inline.render());
        } else if let Some(numbered) = self.paragraph.numbered {
            self.document.item(0, numbered, strip_marker(&inline.render()));
        } else {
            self.document.paragraph(inline.render());
        }
        Ok(())
    }

    fn heading_level(&self) -> Option<usize> {
        if let Some(level) = self.paragraph.outline {
            return Some(level);
        }
        let style = self.paragraph.style?;
        if self.tables.style_names > 0 {
            return self.tables.styles.get(&style).copied();
        }
        (1..=9).contains(&style).then_some(style as usize)
    }

    fn list_numbered(&self, list: i32, level: usize) -> bool {
        let format = self
            .tables
            .overrides
            .get(&list)
            .and_then(|id| self.tables.lists.get(id))
            .and_then(|levels| levels.get(level).or(levels.first()))
            .copied();
        match format {
            Some(23 | 255) => false,
            Some(_) => true,
            None => self.paragraph.numbered.unwrap_or(false),
        }
    }
}

fn hyperlink_target(instruction: &str) -> Option<String> {
    let rest = instruction.trim_start().strip_prefix("HYPERLINK")?.trim_start();
    let url = match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => rest.split_whitespace().next()?,
    };
    (!url.is_empty() && !url.starts_with('\\')).then(|| url.to_string())
}

fn strip_marker(text: &str) -> String {
    let trimmed = text.trim_start();
    let without_bullet = trimmed.strip_prefix(['•', '·', '◦', '▪', '-', '*']).map(str::trim_start);
    if let Some(rest) = without_bullet {
        return rest.to_string();
    }
    let digits = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits > 0 && digits <= 3 {
        let rest = &trimmed[digits..];
        if let Some(rest) = rest.strip_prefix(['.', ')']) {
            return rest.trim_start().to_string();
        }
    }
    trimmed.to_string()
}

fn charset_code_page(charset: i32) -> Option<u16> {
    let code_page = match charset {
        0 => 1252,
        128 => 932,
        129 => 949,
        134 => 936,
        136 => 950,
        161 => 1253,
        162 => 1254,
        163 => 1258,
        177 => 1255,
        178 => 1256,
        186 => 1257,
        204 => 1251,
        222 => 874,
        238 => 1250,
        _ => return None,
    };
    Some(code_page)
}
