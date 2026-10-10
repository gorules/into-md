use quick_xml::{Reader, XmlVersion};
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};

use crate::{Error, Limits, malformed};

pub enum Node<'e, 'x> {
    Open(&'e BytesStart<'x>),
    Close(&'e [u8]),
    Text(&'e str),
}

pub fn walk(xml: &[u8], limits: &Limits, mut visit: impl FnMut(Node<'_, '_>) -> Result<(), Error>) -> Result<(), Error> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().expand_empty_elements = true;
    let mut elements = 0usize;

    loop {
        match reader.read_event().map_err(malformed)? {
            Event::Start(start) => {
                elements += 1;
                if elements > limits.max_xml_elements {
                    return Err(Error::TooLarge("xml elements"));
                }
                visit(Node::Open(&start))?;
            }
            Event::End(end) => visit(Node::Close(end.local_name().as_ref()))?,
            Event::Text(text) => visit(Node::Text(&text.xml10_content().map_err(malformed)?))?,
            Event::CData(data) => visit(Node::Text(&data.decode().map_err(malformed)?))?,
            Event::GeneralRef(reference) => {
                if let Some(ch) = reference.resolve_char_ref().map_err(malformed)? {
                    visit(Node::Text(ch.encode_utf8(&mut [0; 4])))?;
                } else if let Some(text) = resolve_predefined_entity(&reference.decode().map_err(malformed)?) {
                    visit(Node::Text(text))?;
                }
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

pub fn attr(start: &BytesStart<'_>, local: &[u8]) -> Option<String> {
    start
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == local)
        .and_then(|attribute| attribute.normalized_value(XmlVersion::Implicit1_0).ok().map(|value| value.into_owned()))
}

pub fn prefixed_attr(start: &BytesStart<'_>, local: &[u8]) -> Option<String> {
    start
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.prefix().is_some() && attribute.key.local_name().as_ref() == local)
        .and_then(|attribute| attribute.normalized_value(XmlVersion::Implicit1_0).ok().map(|value| value.into_owned()))
}

pub fn is_on(start: &BytesStart<'_>) -> bool {
    !matches!(attr(start, b"val").as_deref(), Some("0" | "false" | "off" | "none"))
}

pub fn relationships(xml: &[u8], limits: &Limits) -> Result<std::collections::HashMap<String, String>, Error> {
    let mut map = std::collections::HashMap::new();
    walk(xml, limits, |node| {
        if let Node::Open(start) = node
            && name(start) == b"Relationship"
            && let (Some(id), Some(target)) = (attr(start, b"Id"), attr(start, b"Target"))
        {
            map.insert(id, target);
        }
        Ok(())
    })?;
    Ok(map)
}

pub fn name<'a>(start: &'a BytesStart<'_>) -> &'a [u8] {
    start.local_name().into_inner()
}
