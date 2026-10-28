//! A small owned XML tree built with quick-xml.
//!
//! XFL files are small enough to hold in memory, and a tree makes the
//! "attributes and children in any order" rule easy to follow. Element and
//! attribute names are stored as **local names**: namespace prefixes are
//! dropped, so `<xfl:DOMDocument>` and `<DOMDocument>` read the same.
//! Namespace declarations (`xmlns`, `xmlns:*`) are not kept as attributes.

use std::borrow::Cow;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// Deepest element nesting accepted. Real XFL nests about 20 levels; the
/// limit keeps hostile files from exhausting the stack later.
const MAX_DEPTH: usize = 256;

/// An XML element with its attributes, child elements and text.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Node {
    /// Local name of the element.
    pub name: String,
    /// Attributes in document order: local name and unescaped value.
    pub attrs: Vec<(String, String)>,
    /// Child elements in document order.
    pub children: Vec<Node>,
    /// The element's own text and CDATA, concatenated and unescaped. Text that
    /// is only indentation between child elements is dropped.
    pub text: String,
}

impl Node {
    /// The value of an attribute, by local name. The first one wins when an
    /// attribute is repeated.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Whether the attribute is present.
    pub fn has_attr(&self, name: &str) -> bool {
        self.attr(name).is_some()
    }

    /// The first child element with this local name.
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// All child elements with this local name.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// The first descendant (depth first, document order) with this name.
    pub fn descendant(&self, name: &str) -> Option<&Node> {
        for c in &self.children {
            if c.name == name {
                return Some(c);
            }
            if let Some(d) = c.descendant(name) {
                return Some(d);
            }
        }
        None
    }
}

/// A well-formedness error.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct XmlError {
    /// Byte offset in the decoded text where the error was found.
    pub position: u64,
    /// What went wrong.
    pub message: String,
}

impl XmlError {
    fn new(position: u64, message: impl Into<String>) -> Self {
        Self {
            position,
            message: message.into(),
        }
    }
}

/// Parses a whole XML document and returns its root element.
///
/// The bytes may start with a UTF-8 or UTF-16 byte order mark. Anything that is
/// not valid UTF-8 is replaced with U+FFFD rather than rejected.
pub(crate) fn parse(bytes: &[u8]) -> Result<Node, XmlError> {
    let text = decode(bytes);
    parse_str(&text)
}

/// Parses an XML document held in a string.
pub(crate) fn parse_str(text: &str) -> Result<Node, XmlError> {
    let mut reader = Reader::from_str(text);
    {
        let config = reader.config_mut();
        config.trim_text(false);
        config.expand_empty_elements = false;
        config.check_end_names = true;
    }

    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| XmlError::new(reader.error_position(), e.to_string()))?;
        match event {
            Event::Start(start) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(XmlError::new(
                        reader.buffer_position(),
                        format!("elements nested deeper than {MAX_DEPTH} levels"),
                    ));
                }
                let node = start_node(&start, reader.buffer_position())?;
                stack.push(node);
            }
            Event::Empty(start) => {
                let node = start_node(&start, reader.buffer_position())?;
                attach(&mut stack, &mut root, node);
            }
            Event::End(_) => {
                let node = stack.pop().ok_or_else(|| {
                    XmlError::new(
                        reader.buffer_position(),
                        "closing tag without an opening tag",
                    )
                })?;
                attach(&mut stack, &mut root, node);
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    let raw = String::from_utf8_lossy(&t);
                    push_text(top, &unescape(&raw));
                }
            }
            Event::CData(c) => {
                if let Some(top) = stack.last_mut() {
                    push_text(top, &String::from_utf8_lossy(&c));
                }
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    if let Some(open) = stack.last() {
        return Err(XmlError::new(
            reader.buffer_position(),
            format!("unexpected end of file inside <{}>", open.name),
        ));
    }
    root.ok_or_else(|| XmlError::new(0, "the file has no root element"))
}

/// Decodes the raw bytes of an XML file to text.
fn decode(bytes: &[u8]) -> Cow<'_, str> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest);
    }
    let utf16 = |rest: &[u8], le: bool| -> Cow<'_, str> {
        let units = rest.as_chunks::<2>().0.iter().map(|p| {
            if le {
                u16::from_le_bytes([p[0], p[1]])
            } else {
                u16::from_be_bytes([p[0], p[1]])
            }
        });
        Cow::Owned(
            char::decode_utf16(units)
                .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect(),
        )
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    String::from_utf8_lossy(bytes)
}

/// Builds a node from a start (or empty-element) tag.
fn start_node(start: &BytesStart<'_>, position: u64) -> Result<Node, XmlError> {
    let name = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
    let mut attrs = Vec::new();
    for attr in start.attributes().with_checks(false) {
        let attr = attr.map_err(|e| XmlError::new(position, format!("in <{name}>: {e}")))?;
        let key = attr.key;
        if key.as_ref() == b"xmlns" || key.prefix().is_some_and(|p| p.as_ref() == b"xmlns") {
            continue;
        }
        let local = String::from_utf8_lossy(key.local_name().as_ref()).into_owned();
        let raw = String::from_utf8_lossy(&attr.value);
        attrs.push((local, unescape(&raw).into_owned()));
    }
    Ok(Node {
        name,
        attrs,
        children: Vec::new(),
        text: String::new(),
    })
}

/// Adds a finished node to its parent, or makes it the root.
fn attach(stack: &mut [Node], root: &mut Option<Node>, node: Node) {
    match stack.last_mut() {
        Some(parent) => {
            if parent.text.trim().is_empty() {
                // Indentation before the first child is not content.
                parent.text.clear();
            }
            parent.children.push(node);
        }
        // A second root element is not well-formed; ignore it rather than
        // lose the document.
        None if root.is_none() => *root = Some(node),
        None => {}
    }
}

/// Appends text to a node, skipping indentation between child elements.
fn push_text(node: &mut Node, text: &str) {
    if !node.children.is_empty() && text.trim().is_empty() {
        return;
    }
    node.text.push_str(text);
}

/// Replaces the five predefined entities and numeric character references.
///
/// Unlike a strict parser, unknown entities (`&nbsp;`) and stray `&` are kept
/// as written instead of failing the whole file.
pub(crate) fn unescape(raw: &str) -> Cow<'_, str> {
    if !raw.contains('&') {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let decoded = after
            .find(';')
            .filter(|&semi| semi <= 12)
            .and_then(|semi| decode_entity(&after[..semi]).map(|c| (c, semi)));
        match decoded {
            Some((c, semi)) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Decodes the name of an entity reference (between `&` and `;`).
fn decode_entity(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => {
            let num = name.strip_prefix('#')?;
            let code = match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => num.parse::<u32>().ok()?,
            };
            char::from_u32(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_namespaced_and_plain_documents_alike() {
        let a = parse_str(
            r#"<?xml version="1.0"?><DOMDocument xmlns="http://ns.adobe.com/xfl/2008/" width="10"><folders/></DOMDocument>"#,
        )
        .unwrap();
        let b = parse_str(
            r#"<x:DOMDocument xmlns:x="http://ns.adobe.com/xfl/2008/" x:width="10"><x:folders/></x:DOMDocument>"#,
        )
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.name, "DOMDocument");
        assert_eq!(a.attr("width"), Some("10"));
        assert!(a.child("folders").is_some());
    }

    #[test]
    fn keeps_text_cdata_and_character_references() {
        let n = parse_str(
            "<a><characters>x&#13;y &amp; &#x645;</characters><script><![CDATA[if (a < b) {}]]></script></a>",
        )
        .unwrap();
        assert_eq!(n.child("characters").unwrap().text, "x\ry & \u{645}");
        assert_eq!(n.child("script").unwrap().text, "if (a < b) {}");
        assert_eq!(n.text, "", "indentation-free container has no text");
    }

    #[test]
    fn drops_indentation_but_keeps_significant_spaces() {
        let n = parse_str("<a>\n  <b> </b>\n  <c/>\n</a>").unwrap();
        assert_eq!(n.text, "");
        assert_eq!(n.child("b").unwrap().text, " ");
    }

    #[test]
    fn tolerates_unknown_entities_and_stray_ampersands() {
        assert_eq!(unescape("a &nbsp; b & c"), "a &nbsp; b & c");
        assert_eq!(unescape("&lt;&gt;&quot;&apos;"), "<>\"'");
        assert_eq!(
            unescape("&#xD800;"),
            "&#xD800;",
            "a surrogate is not a char"
        );
    }

    #[test]
    fn reports_malformed_documents() {
        assert!(parse_str("<a><b></a>").is_err());
        assert!(parse_str("<a>").is_err());
        assert!(parse_str("").is_err());
        let deep = "<a>".repeat(MAX_DEPTH + 1);
        assert!(parse_str(&deep).is_err());
    }

    #[test]
    fn decodes_byte_order_marks() {
        let mut utf8 = vec![0xEF, 0xBB, 0xBF];
        utf8.extend_from_slice(b"<a v='1'/>");
        assert_eq!(parse(&utf8).unwrap().attr("v"), Some("1"));

        let mut utf16 = vec![0xFF, 0xFE];
        for u in "<a v='\u{645}'/>".encode_utf16() {
            utf16.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(parse(&utf16).unwrap().attr("v"), Some("\u{645}"));
    }

    #[test]
    fn finds_descendants_depth_first() {
        let n = parse_str("<a><b><c id='1'/></b><c id='2'/></a>").unwrap();
        assert_eq!(n.descendant("c").and_then(|c| c.attr("id")), Some("1"));
        assert_eq!(n.children_named("c").count(), 1);
    }
}
