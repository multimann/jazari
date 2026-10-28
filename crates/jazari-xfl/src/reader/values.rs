//! Parsers for XFL attribute values: numbers, booleans and colours.
//!
//! They return `None` for values that cannot be read; the caller records a
//! warning and uses the default (see [`super::ctx::Ctx`]).

use crate::model::Rgba;

/// A finite decimal number (`"24"`, `"-0.5"`, `"45.00000125223908"`).
/// Surrounding whitespace is ignored; NaN and infinities are rejected.
pub(crate) fn parse_f64(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A non-negative whole number. `"12.0"` is accepted, `"12.5"` is not.
pub(crate) fn parse_u32(s: &str) -> Option<u32> {
    let v = parse_f64(s)?;
    if v >= 0.0 && v.fract() == 0.0 && v <= f64::from(u32::MAX) {
        // In range and integral, so the cast is exact.
        Some(v as u32)
    } else {
        None
    }
}

/// `true`/`false` (any case), also `1`/`0`.
pub(crate) fn parse_bool(s: &str) -> Option<bool> {
    let t = s.trim();
    if t.eq_ignore_ascii_case("true") || t == "1" {
        Some(true)
    } else if t.eq_ignore_ascii_case("false") || t == "0" {
        Some(false)
    } else {
        None
    }
}

/// An opaque colour written `#RRGGBB` (either case). `#RGB` and `0xRRGGBB`
/// are also accepted.
pub(crate) fn parse_color(s: &str) -> Option<Rgba> {
    let t = s.trim();
    let hex = t
        .strip_prefix('#')
        .or_else(|| t.strip_prefix("0x"))
        .or_else(|| t.strip_prefix("0X"))?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => {
            let v = u32::from_str_radix(hex, 16).ok()?;
            let [_, r, g, b] = v.to_be_bytes();
            Some(Rgba::opaque(r, g, b))
        }
        3 => {
            let v = u16::from_str_radix(hex, 16).ok()?;
            // Each nibble is repeated: #F80 = #FF8800. The masks keep every
            // value below 16, so the casts cannot truncate.
            let nib = |shift: u16| ((v >> shift) & 0xF) as u8 * 17;
            Some(Rgba::opaque(nib(8), nib(4), nib(0)))
        }
        _ => None,
    }
}

/// The leading integer of a string (`"8"`, `" 10 "`, `"1;2"` gives 1).
/// PublishSettings values are strings that may be empty or carry text.
pub(crate) fn parse_leading_int(s: &str) -> Option<i64> {
    let t = s.trim_start();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    let end = digits
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    digits[..end].parse::<i64>().ok().map(|v| sign * v)
}

/// The leading number of a version string, compared numerically: `"2.2"`,
/// `"24.0"`. Returns `None` when the string does not start with a number.
pub(crate) fn parse_version(s: &str) -> Option<f64> {
    let t = s.trim();
    let end = t
        .bytes()
        .position(|b| !(b.is_ascii_digit() || b == b'.'))
        .unwrap_or(t.len());
    let head = &t[..end];
    // "2.2.1" -> "2.2": keep at most one dot.
    let head = match head.match_indices('.').nth(1) {
        Some((second, _)) => &head[..second],
        None => head,
    };
    parse_f64(head)
}

/// Normalises line breaks in a script: CR LF and lone CR become LF.
pub(crate) fn normalize_script(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Normalises paragraph breaks in text runs: CR LF and lone LF become CR,
/// the paragraph separator of the model.
pub(crate) fn normalize_characters(s: &str) -> String {
    s.replace("\r\n", "\r").replace('\n', "\r")
}

/// Cuts a value for display in a warning.
pub(crate) fn excerpt(s: &str) -> String {
    const MAX: usize = 60;
    if s.chars().count() <= MAX {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(MAX).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_f64("24"), Some(24.0));
        assert_eq!(parse_f64(" -0.5 "), Some(-0.5));
        assert_eq!(parse_f64("45.00000125223908"), Some(45.000_001_252_239_08));
        assert_eq!(parse_f64("1e2"), Some(100.0));
        assert_eq!(parse_f64(".5"), Some(0.5));
        assert_eq!(parse_f64("NaN"), None);
        assert_eq!(parse_f64("inf"), None);
        assert_eq!(parse_f64("12px"), None);
        assert_eq!(parse_f64(""), None);
        // XFL decimals are always '.'-separated.
        assert_eq!(parse_f64("1,5"), None);
    }

    #[test]
    fn whole_numbers() {
        assert_eq!(parse_u32("1536"), Some(1536));
        assert_eq!(parse_u32("12.0"), Some(12));
        assert_eq!(parse_u32("12.5"), None);
        assert_eq!(parse_u32("-1"), None);
        assert_eq!(parse_u32("4294967296"), None);
    }

    #[test]
    fn booleans() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("FALSE"), Some(false));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("yes"), None);
    }

    #[test]
    fn colours() {
        assert_eq!(parse_color("#FF6C00"), Some(Rgba::opaque(0xFF, 0x6C, 0x00)));
        assert_eq!(parse_color("#cce0ff"), Some(Rgba::opaque(0xCC, 0xE0, 0xFF)));
        assert_eq!(parse_color("#F80"), Some(Rgba::opaque(0xFF, 0x88, 0x00)));
        assert_eq!(
            parse_color("0x336699"),
            Some(Rgba::opaque(0x33, 0x66, 0x99))
        );
        assert_eq!(parse_color("FF6C00"), None);
        assert_eq!(parse_color("#GG0000"), None);
        assert_eq!(parse_color("#FF6C0080"), None);
        assert_eq!(parse_color("#+12345"), None);
    }

    #[test]
    fn leading_integers_and_versions() {
        assert_eq!(parse_leading_int("8"), Some(8));
        assert_eq!(parse_leading_int(" 10 "), Some(10));
        assert_eq!(parse_leading_int("-3x"), Some(-3));
        assert_eq!(parse_leading_int("FlashLite40"), None);
        assert_eq!(parse_leading_int(""), None);
        assert_eq!(parse_version("2.0"), Some(2.0));
        assert_eq!(parse_version("24.0"), Some(24.0));
        assert_eq!(parse_version("2.2.1"), Some(2.2));
        assert_eq!(parse_version("CS6"), None);
    }

    #[test]
    fn line_breaks() {
        assert_eq!(normalize_script("a\r\nb\rc\nd"), "a\nb\nc\nd");
        assert_eq!(normalize_characters("a\r\nb\nc\rd"), "a\rb\rc\rd");
    }
}
