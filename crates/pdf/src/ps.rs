use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Integer(i64),
    Real,
    Name(Vec<u8>),
    String(Vec<u8>),
    Operator(Vec<u8>),
    ArrayStart,
    ArrayEnd,
    Other,
}

fn is_delimiter(byte: u8) -> bool {
    matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b'\x0C' | b'\0')
}

fn hex_value(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|digit| digit as u8)
}

pub fn tokenize(input: &[u8], stop_at: Option<&[u8]>) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < input.len() {
        let byte = input[at];
        if is_space(byte) {
            at += 1;
            continue;
        }
        match byte {
            b'%' => {
                while at < input.len() && input[at] != b'\n' && input[at] != b'\r' {
                    at += 1;
                }
            }
            b'[' => {
                tokens.push(Token::ArrayStart);
                at += 1;
            }
            b']' => {
                tokens.push(Token::ArrayEnd);
                at += 1;
            }
            b'{' | b'}' | b')' | b'>' => {
                tokens.push(Token::Other);
                at += 1;
            }
            b'<' if input.get(at + 1) == Some(&b'<') => {
                tokens.push(Token::Other);
                at += 2;
            }
            b'<' => {
                at += 1;
                let mut digits = Vec::new();
                while at < input.len() && input[at] != b'>' {
                    if let Some(value) = hex_value(input[at]) {
                        digits.push(value);
                    }
                    at += 1;
                }
                at += 1;
                if digits.len() % 2 == 1 {
                    digits.push(0);
                }
                tokens.push(Token::String(digits.chunks(2).map(|pair| pair[0] << 4 | pair[1]).collect()));
            }
            b'(' => {
                at += 1;
                let mut depth = 1;
                let mut bytes = Vec::new();
                while at < input.len() {
                    let current = input[at];
                    at += 1;
                    match current {
                        b'\\' if at < input.len() => {
                            let escaped = input[at];
                            at += 1;
                            match escaped {
                                b'n' => bytes.push(b'\n'),
                                b'r' => bytes.push(b'\r'),
                                b't' => bytes.push(b'\t'),
                                b'b' => bytes.push(0x08),
                                b'f' => bytes.push(0x0C),
                                b'0'..=b'7' => {
                                    let mut value = (escaped - b'0') as u32;
                                    for _ in 0..2 {
                                        match input.get(at) {
                                            Some(digit @ b'0'..=b'7') => {
                                                value = value * 8 + (digit - b'0') as u32;
                                                at += 1;
                                            }
                                            _ => break,
                                        }
                                    }
                                    bytes.push(value as u8);
                                }
                                b'\r' | b'\n' => {}
                                other => bytes.push(other),
                            }
                        }
                        b'(' => {
                            depth += 1;
                            bytes.push(current);
                        }
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            bytes.push(current);
                        }
                        _ => bytes.push(current),
                    }
                }
                tokens.push(Token::String(bytes));
            }
            b'/' => {
                at += 1;
                let start = at;
                while at < input.len() && !is_space(input[at]) && !is_delimiter(input[at]) {
                    at += 1;
                }
                tokens.push(Token::Name(input[start..at].to_vec()));
            }
            _ => {
                let start = at;
                while at < input.len() && !is_space(input[at]) && !is_delimiter(input[at]) {
                    at += 1;
                }
                if at == start {
                    at += 1;
                    tokens.push(Token::Other);
                    continue;
                }
                let word = &input[start..at];
                let text = std::str::from_utf8(word).unwrap_or("");
                if let Ok(value) = text.parse::<i64>() {
                    tokens.push(Token::Integer(value));
                } else if text.parse::<f64>().is_ok() {
                    tokens.push(Token::Real);
                } else {
                    if stop_at == Some(word) {
                        break;
                    }
                    tokens.push(Token::Operator(word.to_vec()));
                }
            }
        }
    }
    tokens
}

fn is_operator(token: &Token, name: &[u8]) -> bool {
    matches!(token, Token::Operator(operator) if operator == name)
}

fn as_code(bytes: &[u8]) -> u32 {
    bytes.iter().take(4).fold(0u32, |code, byte| code << 8 | *byte as u32)
}

fn sections<'a>(tokens: &'a [Token], begin: &[u8], end: &[u8]) -> Vec<&'a [Token]> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        if is_operator(&tokens[at], begin) {
            let start = at + 1;
            let stop = tokens[start..].iter().position(|token| is_operator(token, end)).map_or(tokens.len(), |offset| start + offset);
            found.push(&tokens[start..stop]);
            at = stop;
        }
        at += 1;
    }
    found
}

fn increment(start: &[u8], offset: u32) -> Vec<u8> {
    if start.len() >= 4 && start.len() % 2 == 0 {
        let split = start.len() - 4;
        let mut out = start[..split].to_vec();
        out.extend_from_slice(&as_code(&start[split..]).wrapping_add(offset).to_be_bytes());
        return out;
    }
    if start.len() >= 2 {
        let split = start.len() - 2;
        let mut out = start[..split].to_vec();
        out.extend_from_slice(&(as_code(&start[split..]) as u16).wrapping_add(offset as u16).to_be_bytes());
        return out;
    }
    vec![start.first().copied().unwrap_or(0).wrapping_add(offset as u8)]
}

const MAX_RANGE: u32 = 0x10000;

pub fn get_unicode_map(input: &[u8]) -> HashMap<u32, Vec<u8>> {
    let tokens = tokenize(input, None);
    let mut map = HashMap::new();
    for section in sections(&tokens, b"beginbfchar", b"endbfchar") {
        for pair in section.chunks_exact(2) {
            if let [Token::String(code), Token::String(value)] = pair {
                map.insert(as_code(code), value.clone());
            }
        }
    }
    for section in sections(&tokens, b"beginbfrange", b"endbfrange") {
        let mut at = 0;
        while at + 2 < section.len() {
            let (low, high) = match (&section[at], &section[at + 1]) {
                (Token::String(low), Token::String(high)) => (as_code(low), as_code(high)),
                _ => {
                    at += 1;
                    continue;
                }
            };
            match &section[at + 2] {
                Token::String(start) => {
                    for code in low..=high.min(low.saturating_add(MAX_RANGE)) {
                        map.insert(code, increment(start, code - low));
                    }
                    at += 3;
                }
                Token::ArrayStart => {
                    let mut index = at + 3;
                    let mut code = low;
                    while index < section.len() && section[index] != Token::ArrayEnd {
                        if let Token::String(value) = &section[index] {
                            if code <= high {
                                map.insert(code, value.clone());
                                code += 1;
                            }
                        }
                        index += 1;
                    }
                    at = index + 1;
                }
                _ => at += 3,
            }
        }
    }
    map
}

#[derive(Debug)]
pub struct CodeRange {
    pub width: u32,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug)]
pub struct CIDRange {
    pub src_code_lo: u32,
    pub src_code_hi: u32,
    #[allow(non_snake_case)]
    pub dst_CID_lo: u32,
}

#[derive(Debug)]
pub struct ByteMapping {
    pub codespace: Vec<CodeRange>,
    pub cid: Vec<CIDRange>,
}

pub fn get_byte_mapping(input: &[u8]) -> ByteMapping {
    let tokens = tokenize(input, None);
    let mut mapping = ByteMapping { codespace: Vec::new(), cid: Vec::new() };
    for section in sections(&tokens, b"begincodespacerange", b"endcodespacerange") {
        for pair in section.chunks_exact(2) {
            if let [Token::String(start), Token::String(end)] = pair {
                mapping.codespace.push(CodeRange { width: start.len().clamp(1, 4) as u32, start: as_code(start), end: as_code(end) });
            }
        }
    }
    for section in sections(&tokens, b"begincidrange", b"endcidrange") {
        for triple in section.chunks_exact(3) {
            if let [Token::String(low), Token::String(high), Token::Integer(cid)] = triple {
                mapping.cid.push(CIDRange { src_code_lo: as_code(low), src_code_hi: as_code(high), dst_CID_lo: *cid as u32 });
            }
        }
    }
    for section in sections(&tokens, b"begincidchar", b"endcidchar") {
        for pair in section.chunks_exact(2) {
            if let [Token::String(code), Token::Integer(cid)] = pair {
                let code = as_code(code);
                mapping.cid.push(CIDRange { src_code_lo: code, src_code_hi: code, dst_CID_lo: *cid as u32 });
            }
        }
    }
    mapping
}

pub fn get_type1_encoding(input: &[u8]) -> HashMap<u32, Vec<u8>> {
    let tokens = tokenize(input, Some(b"eexec"));
    let mut map = HashMap::new();
    let start = match tokens.windows(3).position(|window| {
        matches!(window, [Token::Name(name), Token::Integer(_), Token::Operator(operator)] if name == b"Encoding" && operator == b"array")
    }) {
        Some(start) => start,
        None => return map,
    };
    for window in tokens[start + 3..].windows(3) {
        if is_operator(&window[0], b"def") {
            break;
        }
        if let [Token::Integer(code), Token::Name(name), Token::Operator(operator)] = window {
            if operator == b"put" && (0..256).contains(code) {
                map.insert(*code as u32, name.clone());
            }
        }
    }
    map
}
