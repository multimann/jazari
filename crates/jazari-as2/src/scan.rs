//! A small ActionScript tokenizer for the front-end checks.
//!
//! It knows just enough of the language to find handler blocks and stray
//! characters: it skips white space and comments, keeps string literals whole
//! and nests brackets. Everything else is left to Rascal.

/// The kind of a [`Token`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// An identifier or keyword (`[A-Za-z_$][A-Za-z0-9_$]*`).
    Ident,
    /// A string literal, quotes included (it may be unterminated).
    Str,
    /// One ASCII punctuation character.
    Punct(u8),
    /// `@`, which is not ActionScript and which Rascal reads as PCode.
    At,
    /// Anything else: numbers, non-ASCII characters.
    Other,
}

/// A token with its byte range in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl Token {
    /// The token's text.
    pub(crate) fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }

    /// Whether this is the punctuation character `c`.
    pub(crate) fn is(&self, c: u8) -> bool {
        self.kind == TokenKind::Punct(c)
    }
}

/// Splits `src` into tokens, without comments and white space.
pub(crate) fn tokenize(src: &str) -> Vec<Token> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let b = bytes[i];
        let kind = match b {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c => {
                i += 1;
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = memchr_newline(bytes, i + 2);
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = find(bytes, i + 2, b"*/").map_or(bytes.len(), |end| end + 2);
                continue;
            }
            b'"' | b'\'' => {
                i = string_end(bytes, i);
                TokenKind::Str
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => {
                i += 1;
                while i < bytes.len() && is_ident_byte(bytes[i]) {
                    i += 1;
                }
                TokenKind::Ident
            }
            b'0'..=b'9' => {
                i += 1;
                while i < bytes.len() && (is_ident_byte(bytes[i]) || bytes[i] == b'.') {
                    i += 1;
                }
                TokenKind::Other
            }
            b'@' => {
                i += 1;
                TokenKind::At
            }
            _ if b.is_ascii_punctuation() => {
                i += 1;
                TokenKind::Punct(b)
            }
            _ => {
                // One whole character (possibly several bytes).
                i += src[i..].chars().next().map_or(1, char::len_utf8);
                TokenKind::Other
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: i,
        });
    }
    tokens
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn memchr_newline(bytes: &[u8], from: usize) -> usize {
    bytes[from.min(bytes.len())..]
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .map_or(bytes.len(), |p| from + p)
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

/// End of the string literal starting at `start` (after the closing quote).
/// Like Rascal, an unterminated literal stops at the end of the line.
fn string_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' | b'\r' => return i,
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Deepest nesting of `(`, `[` and `{`, with the offset where it is reached.
pub(crate) fn max_nesting(tokens: &[Token]) -> (usize, usize) {
    let mut depth = 0usize;
    let mut deepest = (0, 0);
    for token in tokens {
        match token.kind {
            TokenKind::Punct(b'(' | b'[' | b'{') => {
                depth += 1;
                if depth > deepest.0 {
                    deepest = (depth, token.start);
                }
            }
            TokenKind::Punct(b')' | b']' | b'}') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// Index of the token closing the bracket opened at `tokens[open]`, if any.
pub(crate) fn matching_close(tokens: &[Token], open: usize) -> Option<usize> {
    let (opening, closing) = match tokens.get(open)?.kind {
        TokenKind::Punct(b'(') => (b'(', b')'),
        TokenKind::Punct(b'[') => (b'[', b']'),
        TokenKind::Punct(b'{') => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        if token.is(opening) {
            depth += 1;
        } else if token.is(closing) {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(TokenKind, &str)> {
        tokenize(src)
            .into_iter()
            .map(|t| (t.kind, t.text(src)))
            .collect()
    }

    #[test]
    fn comments_and_space_are_skipped() {
        assert_eq!(
            kinds("a // } x\n/* { */ b"),
            vec![(TokenKind::Ident, "a"), (TokenKind::Ident, "b")]
        );
    }

    #[test]
    fn strings_keep_braces_and_escaped_quotes() {
        assert_eq!(
            kinds(r#"x("}{", 'it\'s }')"#),
            vec![
                (TokenKind::Ident, "x"),
                (TokenKind::Punct(b'('), "("),
                (TokenKind::Str, r#""}{""#),
                (TokenKind::Punct(b','), ","),
                (TokenKind::Str, r"'it\'s }'"),
                (TokenKind::Punct(b')'), ")"),
            ]
        );
    }

    #[test]
    fn unterminated_string_stops_at_line_end() {
        assert_eq!(
            kinds("\"abc\n}"),
            vec![(TokenKind::Str, "\"abc"), (TokenKind::Punct(b'}'), "}")]
        );
    }

    #[test]
    fn at_sign_numbers_and_unicode() {
        assert_eq!(
            kinds("@x 1.5e3 م"),
            vec![
                (TokenKind::At, "@"),
                (TokenKind::Ident, "x"),
                (TokenKind::Other, "1.5e3"),
                (TokenKind::Other, "م"),
            ]
        );
    }

    #[test]
    fn unterminated_block_comment_runs_to_the_end() {
        assert_eq!(kinds("a /* b"), vec![(TokenKind::Ident, "a")]);
    }

    #[test]
    fn brackets_match_and_nest() {
        let src = "{ ( [ ] ) { } }";
        let tokens = tokenize(src);
        assert_eq!(matching_close(&tokens, 0), Some(7));
        assert_eq!(matching_close(&tokens, 1), Some(4));
        assert_eq!(max_nesting(&tokens), (3, 4));
        assert_eq!(matching_close(&tokenize("{ {"), 0), None);
    }
}
