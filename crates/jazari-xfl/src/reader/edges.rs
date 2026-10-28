//! The `Edge@edges` path grammar and its number format.
//!
//! Grammar (`docs/research/xfl-format.md`, F11-F12):
//!
//! ```text
//! path    := (moveto | lineto | quadto | close | select)*
//! moveto  := '!' num num
//! lineto  := ('|' | '/') num num
//! quadto  := ('[' | ']') num num num num      control point, then end point
//! close   := '/'                              a '/' not followed by numbers
//! select  := 'S' digit*                       editor selection hint, skipped
//! num     := decimal | '#' hex24 ['.' hex2]   absolute twips
//! ```
//!
//! A `#` number is signed 24.8 fixed point: the integer digits are left-padded
//! to 6 hex digits and the fraction right-padded to 2, the 8 digits are read as
//! a two's-complement `i32`, and the result is divided by 256. So `#3E8.80` is
//! 1000.5 twips and `#FFFF38.40` is −199.75 twips (the fraction is *added*).

use crate::model::{PathCmd, Pt};

/// A syntax error in an `edges` string. The commands read before the error
/// are kept in `partial`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EdgeError {
    /// Byte offset of the error in the string.
    pub offset: usize,
    /// What was expected.
    pub message: String,
    /// The commands parsed before the error.
    pub partial: Vec<PathCmd>,
}

/// Parses one edge number (decimal or `#` fixed point) into twips.
pub(crate) fn edge_number(token: &str) -> Option<f64> {
    match token.strip_prefix('#') {
        Some(hex) => {
            let (int_part, frac_part) = hex.split_once('.').unwrap_or((hex, ""));
            hex_fixed(int_part, frac_part)
        }
        None => token.parse::<f64>().ok().filter(|v| v.is_finite()),
    }
}

/// Reads `int_part.frac_part` (hex digits) as signed 24.8 fixed point.
fn hex_fixed(int_part: &str, frac_part: &str) -> Option<f64> {
    let hex = |s: &str| -> Option<u32> {
        if s.is_empty() {
            Some(0)
        } else if s.bytes().all(|b| b.is_ascii_hexdigit()) {
            u32::from_str_radix(s, 16).ok()
        } else {
            None
        }
    };
    if frac_part.len() > 2 || int_part.len() > 8 || (int_part.is_empty() && frac_part.is_empty()) {
        return None;
    }
    let frac = hex(frac_part)? << (4 * (2 - frac_part.len() as u32));
    let int = hex(int_part)?;
    if int_part.len() <= 6 {
        // int < 2^24, so the shift stays within 32 bits; the cast to i32
        // reinterprets the 32-bit pattern as two's complement on purpose.
        let bits = (int << 8) | frac;
        Some(f64::from(bits as i32) / 256.0)
    } else {
        // Longer integer parts are not written by Flash or JPEXS; read them
        // as a full 32-bit signed integer plus the fraction.
        Some(f64::from(int as i32) + f64::from(frac) / 256.0)
    }
}

/// A cursor over the bytes of an `edges` string.
struct Lexer<'a> {
    s: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn bytes(&self) -> &'a [u8] {
        self.s.as_bytes()
    }

    /// Skips whitespace and commas.
    fn skip_separators(&mut self) {
        while let Some(&b) = self.bytes().get(self.pos) {
            if b.is_ascii_whitespace() || b == b',' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Whether a number starts at the next non-separator byte.
    fn at_number(&mut self) -> bool {
        self.skip_separators();
        matches!(
            self.bytes().get(self.pos),
            Some(b'#' | b'-' | b'+' | b'.' | b'0'..=b'9')
        )
    }

    /// Reads one number.
    fn number(&mut self) -> Result<f64, (usize, String)> {
        self.skip_separators();
        let start = self.pos;
        let b = self.bytes();
        if b.get(self.pos) == Some(&b'#') {
            self.pos += 1;
            while self
                .bytes()
                .get(self.pos)
                .is_some_and(|c| c.is_ascii_hexdigit() || *c == b'.')
            {
                self.pos += 1;
            }
        } else {
            if matches!(b.get(self.pos), Some(b'-' | b'+')) {
                self.pos += 1;
            }
            while self
                .bytes()
                .get(self.pos)
                .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
            {
                self.pos += 1;
            }
            // Optional exponent, only when digits follow it.
            if matches!(self.bytes().get(self.pos), Some(b'e' | b'E')) {
                let mut look = self.pos + 1;
                if matches!(self.bytes().get(look), Some(b'-' | b'+')) {
                    look += 1;
                }
                if self.bytes().get(look).is_some_and(u8::is_ascii_digit) {
                    self.pos = look;
                    while self.bytes().get(self.pos).is_some_and(u8::is_ascii_digit) {
                        self.pos += 1;
                    }
                }
            }
        }
        let token = &self.s[start..self.pos];
        if token.is_empty() {
            return Err((start, "expected a number".to_owned()));
        }
        edge_number(token).ok_or_else(|| (start, format!("'{token}' is not a valid number")))
    }

    fn point(&mut self) -> Result<Pt, (usize, String)> {
        let x = self.number()?;
        let y = self.number()?;
        Ok(Pt::new(x, y))
    }
}

/// Parses an `edges` attribute into absolute path commands in twips.
///
/// A bare `/` closes the current subpath with a line back to its start, and a
/// path that draws before its first `!` starts at (0, 0), as SWF does.
pub(crate) fn parse_edges(s: &str) -> Result<Vec<PathCmd>, EdgeError> {
    let mut lexer = Lexer { s, pos: 0 };
    let mut out: Vec<PathCmd> = Vec::new();
    let mut start: Option<Pt> = None;
    let mut pen = Pt::ORIGIN;

    let fail = |out: Vec<PathCmd>, (offset, message): (usize, String)| EdgeError {
        offset,
        message,
        partial: out,
    };

    // Starts an implicit subpath at the origin before a drawing command.
    fn ensure_start(out: &mut Vec<PathCmd>, start: &mut Option<Pt>, pen: Pt) {
        if start.is_none() {
            out.push(PathCmd::MoveTo(pen));
            *start = Some(pen);
        }
    }

    loop {
        lexer.skip_separators();
        let Some(&c) = s.as_bytes().get(lexer.pos) else {
            break;
        };
        let at = lexer.pos;
        lexer.pos += 1;
        match c {
            b'!' => match lexer.point() {
                Ok(p) => {
                    out.push(PathCmd::MoveTo(p));
                    start = Some(p);
                    pen = p;
                }
                Err(e) => return Err(fail(out, e)),
            },
            b'|' => match lexer.point() {
                Ok(p) => {
                    ensure_start(&mut out, &mut start, pen);
                    out.push(PathCmd::LineTo(p));
                    pen = p;
                }
                Err(e) => return Err(fail(out, e)),
            },
            b'/' => {
                if lexer.at_number() {
                    match lexer.point() {
                        Ok(p) => {
                            ensure_start(&mut out, &mut start, pen);
                            out.push(PathCmd::LineTo(p));
                            pen = p;
                        }
                        Err(e) => return Err(fail(out, e)),
                    }
                } else if let Some(first) = start {
                    // Close path: a line back to the subpath's start.
                    if pen != first {
                        out.push(PathCmd::LineTo(first));
                        pen = first;
                    }
                }
            }
            b'[' | b']' => {
                let quad = match lexer.point() {
                    Ok(ctrl) => lexer.point().map(|to| (ctrl, to)),
                    Err(e) => Err(e),
                };
                match quad {
                    Ok((ctrl, to)) => {
                        ensure_start(&mut out, &mut start, pen);
                        out.push(PathCmd::QuadTo(ctrl, to));
                        pen = to;
                    }
                    Err(e) => return Err(fail(out, e)),
                }
            }
            b'S' => {
                while s.as_bytes().get(lexer.pos).is_some_and(u8::is_ascii_digit) {
                    lexer.pos += 1;
                }
            }
            _ => {
                let shown = s[at..].chars().next().unwrap_or('?');
                return Err(fail(out, (at, format!("unexpected '{shown}'"))));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(x: f64, y: f64) -> PathCmd {
        PathCmd::MoveTo(Pt::new(x, y))
    }
    fn ln(x: f64, y: f64) -> PathCmd {
        PathCmd::LineTo(Pt::new(x, y))
    }
    fn qd(cx: f64, cy: f64, x: f64, y: f64) -> PathCmd {
        PathCmd::QuadTo(Pt::new(cx, cy), Pt::new(x, y))
    }

    #[test]
    fn decimal_numbers_are_twips() {
        assert_eq!(edge_number("1000"), Some(1000.0));
        assert_eq!(edge_number("-12.5"), Some(-12.5));
        assert_eq!(edge_number("-257.90625"), Some(-257.90625));
        assert_eq!(edge_number("abc"), None);
    }

    #[test]
    fn hex_numbers_are_signed_24_8_fixed_point() {
        assert_eq!(edge_number("#3E8.80"), Some(1000.5));
        assert_eq!(edge_number("#7D0.40"), Some(2000.25));
        // The fraction is added to the negative integer: -200 + 0.25.
        assert_eq!(edge_number("#FFFF38.40"), Some(-199.75));
        assert_eq!(edge_number("#ffff38.40"), Some(-199.75), "lowercase");
        assert_eq!(edge_number("#3E8"), Some(1000.0), "no fraction");
        assert_eq!(edge_number("#D0.3A"), Some(208.0 + 58.0 / 256.0));
        assert_eq!(
            edge_number("#1.8"),
            Some(1.5),
            "one fraction digit is the high nibble"
        );
        assert_eq!(edge_number("#FFFFFF.00"), Some(-1.0));
        assert_eq!(edge_number("#2024.80"), Some(8228.5));
        assert_eq!(edge_number("#3E8.123"), None, "three fraction digits");
        assert_eq!(edge_number("#XYZ"), None);
        assert_eq!(edge_number("#"), None);
        assert_eq!(edge_number("#.80"), Some(0.5), "an empty integer part is 0");
    }

    #[test]
    fn worked_example_from_the_research_notes() {
        let path = parse_edges("!0 0S1|2000 0|2000 #3E8.80[1000 #7D0.40 0 1000|0 0").unwrap();
        assert_eq!(
            path,
            vec![
                mv(0.0, 0.0),
                ln(2000.0, 0.0),
                ln(2000.0, 1000.5),
                qd(1000.0, 2000.25, 0.0, 1000.0),
                ln(0.0, 0.0),
            ]
        );
    }

    #[test]
    fn slash_is_a_line_or_a_close() {
        assert_eq!(
            parse_edges("!7995 6095/8205 7505").unwrap(),
            vec![mv(7995.0, 6095.0), ln(8205.0, 7505.0)]
        );
        assert_eq!(
            parse_edges("!0 0|10 0|10 10/").unwrap(),
            vec![mv(0.0, 0.0), ln(10.0, 0.0), ln(10.0, 10.0), ln(0.0, 0.0)]
        );
        // Closing an already closed subpath adds nothing.
        assert_eq!(
            parse_edges("!0 0|10 0|0 0/").unwrap(),
            vec![mv(0.0, 0.0), ln(10.0, 0.0), ln(0.0, 0.0)]
        );
    }

    #[test]
    fn both_bracket_kinds_are_quadratic_curves() {
        assert_eq!(
            parse_edges("!-226.5 229.5[-257.90625 #D0.3A 1 2]3 4 5 6").unwrap(),
            vec![
                mv(-226.5, 229.5),
                qd(-257.90625, 208.0 + 58.0 / 256.0, 1.0, 2.0),
                qd(3.0, 4.0, 5.0, 6.0),
            ]
        );
    }

    #[test]
    fn several_subpaths_and_selection_hints() {
        assert_eq!(
            parse_edges("!0 0S2|1 1!5 5S12|6 6").unwrap(),
            vec![mv(0.0, 0.0), ln(1.0, 1.0), mv(5.0, 5.0), ln(6.0, 6.0)]
        );
    }

    #[test]
    fn real_fixture_strings() {
        // From the bitmaps and as2 fixtures.
        assert_eq!(
            parse_edges("!#2024.80 6026[#1F07.6B #15EA.7C 8085 #1465.80").unwrap(),
            vec![
                mv(8228.5, 6026.0),
                qd(
                    7943.0 + 107.0 / 256.0,
                    5610.0 + 124.0 / 256.0,
                    8085.0,
                    5221.5
                ),
            ]
        );
        assert_eq!(
            parse_edges("!353 -354[500 -207 500 0").unwrap(),
            vec![mv(353.0, -354.0), qd(500.0, -207.0, 500.0, 0.0)]
        );
    }

    #[test]
    fn whitespace_and_empty_strings() {
        assert_eq!(parse_edges("").unwrap(), vec![]);
        assert_eq!(parse_edges("  ").unwrap(), vec![]);
        assert_eq!(
            parse_edges(" ! 1  2 \n| 3\t4 ").unwrap(),
            vec![mv(1.0, 2.0), ln(3.0, 4.0)]
        );
    }

    #[test]
    fn drawing_before_a_moveto_starts_at_the_origin() {
        assert_eq!(
            parse_edges("|10 0").unwrap(),
            vec![mv(0.0, 0.0), ln(10.0, 0.0)]
        );
    }

    #[test]
    fn errors_keep_the_commands_read_so_far() {
        let err = parse_edges("!0 0|10 0|20").unwrap_err();
        assert_eq!(err.partial, vec![mv(0.0, 0.0), ln(10.0, 0.0)]);
        assert_eq!(err.offset, 12);
        let err = parse_edges("!0 0 x").unwrap_err();
        assert_eq!(err.offset, 5);
        assert!(err.message.contains("'x'"));
        let err = parse_edges("!0 #12345.123").unwrap_err();
        assert!(err.message.contains("not a valid number"));
    }
}
