//! Source text helpers: the byte order mark, positions and blanking.

use crate::scan::{self, TokenKind};
use std::borrow::Cow;
use std::ops::Range;

/// Removes a leading UTF-8 byte order mark.
///
/// Rascal strips it too, but then reports positions against the unstripped
/// text, three bytes off; stripping it first keeps every position true.
pub(crate) fn strip_bom(src: &str) -> &str {
    src.strip_prefix('\u{feff}').unwrap_or(src)
}

/// Maps byte offsets to 1-based lines and 1-based columns counted in
/// characters. `\n`, `\r\n` and a lone `\r` each end a line, as in Rascal's
/// lexer.
#[derive(Debug)]
pub(crate) struct LineIndex<'a> {
    text: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let bytes = text.as_bytes();
        let mut line_starts = vec![0];
        for (i, &b) in bytes.iter().enumerate() {
            let ends_line = match b {
                b'\n' => true,
                b'\r' => bytes.get(i + 1) != Some(&b'\n'),
                _ => false,
            };
            if ends_line {
                line_starts.push(i + 1);
            }
        }
        Self { text, line_starts }
    }

    /// Line and column of a byte offset (clamped to the text).
    pub(crate) fn position(&self, offset: usize) -> (u32, u32) {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.line_starts.partition_point(|&start| start <= offset);
        let start = self.line_starts[line - 1];
        let column = self.text[start..offset].chars().count() + 1;
        (to_u32(line), to_u32(column))
    }

    /// Position just after the last character that is not white space, for
    /// errors at the end of the input.
    pub(crate) fn end_position(&self) -> (u32, u32) {
        self.position(self.text.trim_end().len())
    }
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Replaces every character outside `keep` (a byte range of `src`) by one
/// space, except line breaks, so lines and character columns stay the same.
///
/// One space per character, not per byte: diagnostics count columns in
/// characters, so a multi-byte character before the body on the same line
/// (an Arabic comment, say) must still count as one column. Byte offsets in
/// the result therefore differ from `src` when such characters are blanked.
pub(crate) fn blank_outside(src: &str, keep: Range<usize>) -> String {
    let mut out = String::with_capacity(src.len());
    for (i, c) in src.char_indices() {
        if keep.contains(&i) || c == '\n' || c == '\r' {
            out.push(c);
        } else {
            out.push(' ');
        }
    }
    out
}

/// Prepares a script or a file it loads for Rascal, keeping every byte
/// offset, line and column: lone `\r` line breaks become `\n` (see
/// [`normalize_line_breaks`]) and `#include` paths get forward slashes (see
/// [`normalize_include_paths`]).
pub(crate) fn prepare_for_rascal(src: &str) -> Cow<'_, str> {
    match normalize_line_breaks(src) {
        Cow::Borrowed(src) => normalize_include_paths(src),
        Cow::Owned(text) => Cow::Owned(normalize_include_paths(&text).into_owned()),
    }
}

/// Turns every `\r` that is not followed by `\n` into `\n`.
///
/// Flash 8 and old Mac files end lines with a lone `\r`, and XML readers that
/// do not normalise line ends pass it on. Rascal's lexer ends a `//` comment
/// only at `\n`, so with lone `\r` breaks everything after the first line
/// comment would silently vanish. Both bytes end a line for [`LineIndex`],
/// so positions do not change.
pub(crate) fn normalize_line_breaks(src: &str) -> Cow<'_, str> {
    let bytes = src.as_bytes();
    let lone_cr = |i: usize| bytes[i] == b'\r' && bytes.get(i + 1) != Some(&b'\n');
    if !(0..bytes.len()).any(lone_cr) {
        return Cow::Borrowed(src);
    }
    let mut out = String::with_capacity(src.len());
    let mut last = 0;
    for i in (0..bytes.len()).filter(|&i| lone_cr(i)) {
        out.push_str(&src[last..i]);
        out.push('\n');
        last = i + 1;
    }
    out.push_str(&src[last..]);
    Cow::Owned(out)
}

/// Turns the backslashes in `#include "..."` paths into slashes, keeping
/// every byte offset.
///
/// Flash-era scripts written on Windows use paths such as
/// `#include "lib\util.as"`. Rascal reads the path as a string literal and
/// decodes escapes in it (`\u`, `\t`, ...), which would mangle it, and a
/// backslash is no separator on Linux anyway. A `\` before the closing quote
/// is left alone so the literal keeps its extent.
pub(crate) fn normalize_include_paths(src: &str) -> Cow<'_, str> {
    let tokens = scan::tokenize(src);
    let mut bytes: Option<Vec<u8>> = None;
    for window in tokens.windows(3) {
        let [hash, directive, path] = window else {
            continue;
        };
        if !hash.is(b'#')
            || directive.kind != TokenKind::Ident
            || directive.text(src) != "include"
            || path.kind != TokenKind::Str
        {
            continue;
        }
        let literal = path.text(src).as_bytes();
        let quote = literal[0];
        for (i, &b) in literal.iter().enumerate().skip(1) {
            if b == b'\\' && literal.get(i + 1) != Some(&quote) {
                bytes.get_or_insert_with(|| src.as_bytes().to_vec())[path.start + i] = b'/';
            }
        }
    }
    match bytes {
        // Only ASCII `\` bytes were replaced by ASCII `/`, so this stays UTF-8.
        Some(bytes) => Cow::Owned(String::from_utf8(bytes).unwrap_or_else(|_| src.to_owned())),
        None => Cow::Borrowed(src),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lone_carriage_returns_become_line_feeds() {
        assert_eq!(normalize_line_breaks("a\rb\r\nc\n\r"), "a\nb\r\nc\n\n");
        assert!(matches!(
            normalize_line_breaks("a\r\nb\n"),
            Cow::Borrowed(_)
        ));
        let text = "// م\rx = ;";
        let normalized = normalize_line_breaks(text);
        assert_eq!(normalized.len(), text.len());
        assert_eq!(
            LineIndex::new(&normalized).position(9),
            LineIndex::new(text).position(9)
        );
        assert_eq!(
            prepare_for_rascal("// c\r#include \"a\\b.as\""),
            "// c\n#include \"a/b.as\""
        );
    }

    #[test]
    fn include_paths_use_slashes() {
        assert_eq!(
            normalize_include_paths(r#"#include "lib\util.as" // a\b"#),
            r#"#include "lib/util.as" // a\b"#
        );
        assert_eq!(
            normalize_include_paths(r#"# include 'a\\b.as'; s = "x\ty";"#),
            r#"# include 'a//b.as'; s = "x\ty";"#
        );
        assert_eq!(
            normalize_include_paths(r#"#include "odd\""#),
            r#"#include "odd\""#
        );
        assert!(matches!(
            normalize_include_paths("trace(\"a\\b\");"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn bom_is_stripped_once() {
        assert_eq!(strip_bom("\u{feff}trace(1);"), "trace(1);");
        assert_eq!(strip_bom("trace(1);"), "trace(1);");
    }

    #[test]
    fn positions_count_lines_and_characters() {
        let text = "ab\ncd\r\nef\rgh";
        let index = LineIndex::new(text);
        assert_eq!(index.position(0), (1, 1));
        assert_eq!(index.position(1), (1, 2));
        assert_eq!(index.position(3), (2, 1));
        assert_eq!(index.position(7), (3, 1));
        assert_eq!(index.position(10), (4, 1));
        assert_eq!(index.position(99), (4, 3));
    }

    #[test]
    fn columns_are_characters_not_bytes() {
        // "مرحبا" is five characters and ten bytes.
        let text = "x = \"مرحبا\"; y";
        let index = LineIndex::new(text);
        let y = text.find('y').expect("y");
        assert_eq!(index.position(y), (1, 14));
    }

    #[test]
    fn end_position_skips_trailing_space() {
        let index = LineIndex::new("a\nbc  \n\n");
        assert_eq!(index.end_position(), (2, 3));
    }

    #[test]
    fn blanking_keeps_offsets_and_line_breaks() {
        let text = "on (press) {\r\n\tمر();\n}";
        let open = text.find('{').expect("{") + 1;
        let close = text.rfind('}').expect("}");
        let blanked = blank_outside(text, open..close);
        assert_eq!(blanked.len(), text.len());
        assert_eq!(blanked, "            \r\n\tمر();\n ");
    }

    #[test]
    fn blanking_keeps_character_columns() {
        // "زر" is two characters and four bytes: it must blank to two spaces.
        let text = "/* زر */ on (press) { x; }";
        let open = text.find('{').expect("{") + 1;
        let close = text.rfind('}').expect("}");
        let blanked = blank_outside(text, open..close);
        let x = text.find('x').expect("x");
        let blanked_x = blanked.find('x').expect("x");
        assert_eq!(
            LineIndex::new(&blanked).position(blanked_x),
            LineIndex::new(text).position(x)
        );
        assert_eq!(blanked, format!("{} x; {}", " ".repeat(21), " "));
    }
}
