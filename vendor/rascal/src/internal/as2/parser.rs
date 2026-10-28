use crate::internal::as2::lexer::tokens::{QuoteKind, Token, TokenKind};
use crate::internal::span::Spanned;
use std::borrow::Cow;
use winnow::combinator::{alt, repeat};
use winnow::error::{ContextError, ErrMode, ParseError};
use winnow::stream::TokenSlice;
use winnow::token::literal;
use winnow::{ModalResult, Parser};

mod class;
pub(crate) mod document;
mod expression;
mod operator;
mod statement;
#[cfg(test)]
mod tests;

pub(crate) type Tokens<'i> = TokenSlice<'i, Token<'i>>;

use crate::internal::as2::ast::Document;

pub fn parse_document<'i>(
    source: &'i [Token<'i>],
) -> winnow::Result<Document<'i>, ParseError<Tokens<'i>, ContextError>> {
    document::document.parse(TokenSlice::new(source))
}

fn string<'i>(i: &mut Tokens<'i>) -> ModalResult<Spanned<Cow<'i, str>>> {
    skip_newlines(i)?;
    let (raw, span) = alt((
        TokenKind::String(QuoteKind::Double).map(|t| (t.raw, t.span)),
        TokenKind::String(QuoteKind::Single).map(|t| (t.raw, t.span)),
    ))
    .parse_next(i)?;
    if !raw.contains("\\") {
        // SWF strings end at the first NUL, so Flash cuts the literal there.
        let raw = raw.split('\0').next().unwrap_or_default();
        return Ok(Spanned::new(span, Cow::Borrowed(raw)));
    }
    Ok(Spanned::new(span, Cow::Owned(unescape(raw))))
}

/// Decodes the escapes of a string literal the way the Flash 8 compiler does:
///
/// - `\n \r \t \b \f \\ \" \'`;
/// - `\uXXXX` (a UTF-16 unit; a surrogate pair becomes one character);
/// - `\xNN` (U+00NN; Flash 8 maps the byte through the system ANSI code page instead,
///   which no portable compiler can reproduce);
/// - `\0` to `\377`: up to three octal digits;
/// - `\u` or `\x` without enough hex digits: the backslash is dropped (`"\x4"` is `"x4"`);
/// - any other escape keeps the backslash and drops the character (`"\q"` is `"\"`);
/// - the result ends at the first NUL, as SWF strings do (`"a\0b"` is `"a"`).
pub(crate) fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut iter = raw.chars().peekable();
    while let Some(ch) = iter.next() {
        if ch == '\\' {
            match iter.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some('u') => match take_hex_digits(&mut iter, 4) {
                    Some(unit) => push_utf16_unit(&mut out, &mut iter, unit),
                    None => out.push('u'),
                },
                Some('x') => match take_hex_digits(&mut iter, 2) {
                    Some(value) => out.push(char::from(value as u8)),
                    None => out.push('x'),
                },
                Some(first @ '0'..='7') => out.push(take_octal_escape(&mut iter, first)),
                Some(_) | None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    if let Some(nul) = out.find('\0') {
        out.truncate(nul);
    }
    out
}

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// Consumes exactly `count` hex digits, or nothing at all when fewer follow.
fn take_hex_digits(iter: &mut Chars<'_>, count: usize) -> Option<u32> {
    let digits: String = iter.clone().take(count).collect();
    if digits.len() != count || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    for _ in 0..count {
        iter.next();
    }
    u32::from_str_radix(&digits, 16).ok()
}

/// Pushes the UTF-16 code unit of a `\uXXXX` escape. A high surrogate followed by a
/// `\uXXXX` low surrogate becomes one character; a lone surrogate becomes U+FFFD,
/// because SWF strings are UTF-8 and cannot hold it.
fn push_utf16_unit(out: &mut String, iter: &mut Chars<'_>, unit: u32) {
    if (0xD800..0xDC00).contains(&unit) {
        let mut ahead = iter.clone();
        if ahead.next() == Some('\\')
            && ahead.next() == Some('u')
            && let Some(low) = take_hex_digits(&mut ahead, 4)
            && (0xDC00..0xE000).contains(&low)
        {
            *iter = ahead;
            let code = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
            out.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
            return;
        }
    }
    out.push(char::from_u32(unit).unwrap_or(char::REPLACEMENT_CHARACTER));
}

/// An octal escape: up to three digits after the backslash, no greater than `\377`.
fn take_octal_escape(iter: &mut Chars<'_>, first: char) -> char {
    let mut value = first as u32 - '0' as u32;
    for _ in 0..2 {
        match iter.peek() {
            Some(&digit @ '0'..='7') if value * 8 + (digit as u32 - '0' as u32) <= 0o377 => {
                value = value * 8 + (digit as u32 - '0' as u32);
                iter.next();
            }
            _ => break,
        }
    }
    char::from(value as u8)
}

fn identifier<'i>(i: &mut Tokens<'i>) -> ModalResult<Spanned<&'i str>> {
    skip_newlines(i)?;
    let token = TokenKind::Identifier.parse_next(i)?;
    Ok(Spanned::new(token.span, token.raw))
}

pub(crate) fn ignore_newlines<'i: 'i, O, P>(
    mut inner: P,
) -> impl Parser<Tokens<'i>, O, ErrMode<ContextError>>
where
    P: Parser<Tokens<'i>, O, ErrMode<ContextError>>,
{
    move |input: &mut Tokens<'i>| {
        skip_newlines(input)?;
        inner.parse_next(input)
    }
}

pub(crate) fn skip_newlines(i: &mut Tokens<'_>) -> ModalResult<()> {
    repeat(0.., literal(TokenKind::Newline))
        .map(|()| ())
        .parse_next(i)
}
