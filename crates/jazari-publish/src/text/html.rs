//! The initial text of a DefineEditText as Flash 8 HTML.
//!
//! Flash 8 writes every dynamic and input field's text as HTML, even when
//! "render as HTML" is off: one `<p>` per paragraph holding one `<font>`
//! with all the attributes of the paragraph's first run, in this order
//! (the `text` and `as2` fixtures and a probe of Flash 8):
//!
//! ```text
//! <p align="left"><font face="Arial" size="16" color="#333333" letterSpacing="0.000000" kerning="1">Dynamic text field</font></p>
//! ```
//!
//! A later run of the paragraph whose font attributes differ gets a nested
//! `<font>` with only the attributes that differ; bold and italic add `<b>`
//! and `<i>` inside it, and a link an `<a href>` (with `target` only when
//! one is set) around them:
//!
//! ```text
//! <p align="center"><font face="Arial" size="14" color="#ff0000" letterSpacing="0.000000" kerning="1"><b>Hello </b><font face="Times New Roman" size="18" color="#0000ff"><i>world</i></font></font></p>
//! ```
//!
//! Colours are lower-case hex, sizes whole points (Flash wrote 10.5 pt as
//! `10`), and `<`, `>`, `&` and `"` are escaped in text and attributes.
//! Flash 8 drops links from input text. A paragraph whose margins, indent
//! or leading differ from the field's own layout (taken from the first
//! paragraph) is wrapped in `<textformat>` (not seen from Flash 8, which
//! had no such probe).

use std::fmt::Write as _;

use jazari_xfl::model::{Align, TextAttrs, TextRun};

/// How a face appears in HTML and in the device font: family, bold,
/// italic.
pub type DeviceFace = (String, bool, bool);

/// One paragraph: its runs (text without the separator) and the attributes
/// that hold for the paragraph (those of its first run).
struct Paragraph<'a> {
    runs: Vec<(&'a str, &'a TextAttrs)>,
    attrs: &'a TextAttrs,
}

/// Splits runs into paragraphs at CR (and LF or CR LF, defensively). A
/// paragraph takes its attributes from its first run with text; an empty
/// paragraph (two separators in a row, a trailing separator, or no text at
/// all) from the run it is in.
fn paragraphs(runs: &[TextRun]) -> Vec<Paragraph<'_>> {
    let empty = |a| Paragraph {
        runs: vec![("", a)],
        attrs: a,
    };
    let mut out: Vec<Paragraph<'_>> = Vec::new();
    let mut cur: Option<Paragraph<'_>> = None;
    let mut after_separator = false;
    for run in runs {
        let mut rest = run.text.as_str();
        loop {
            let (piece, more) = match rest.find(['\r', '\n']) {
                Some(i) => {
                    let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
                    (&rest[..i], Some(&rest[i + skip..]))
                }
                None => (rest, None),
            };
            if !piece.is_empty() {
                cur.get_or_insert_with(|| Paragraph {
                    runs: Vec::new(),
                    attrs: &run.attrs,
                })
                .runs
                .push((piece, &run.attrs));
                after_separator = false;
            }
            match more {
                Some(m) => {
                    out.push(cur.take().unwrap_or_else(|| empty(&run.attrs)));
                    after_separator = true;
                    rest = m;
                }
                None => break,
            }
        }
    }
    match cur {
        Some(p) => out.push(p),
        None => {
            if let Some(last) = runs.last().filter(|_| after_separator || out.is_empty()) {
                out.push(empty(&last.attrs));
            }
        }
    }
    out
}

/// The font attributes of a run, in Flash's order: face, size, color,
/// letterSpacing, kerning.
fn font_attrs(a: &TextAttrs, family: &str) -> [(&'static str, String); 5] {
    [
        ("face", escape(family)),
        ("size", whole_points(a.size)),
        (
            "color",
            format!("#{:02x}{:02x}{:02x}", a.fill.r, a.fill.g, a.fill.b),
        ),
        ("letterSpacing", format!("{:.6}", finite(a.letter_spacing))),
        ("kerning", u8::from(a.auto_kern).to_string()),
    ]
}

/// The field's HTML. `face` maps an XFL face name to its device face;
/// `links` is false for input text, whose links Flash 8 drops.
pub fn field_html(runs: &[TextRun], face: &dyn Fn(&str) -> DeviceFace, links: bool) -> String {
    let paras = paragraphs(runs);
    let field = paras.first().map(|p| p.attrs);
    let mut out = String::new();
    for p in &paras {
        let format = field.is_some_and(|f| !same_block_format(f, p.attrs));
        if format {
            let a = p.attrs;
            let _ = write!(
                out,
                "<textformat leftmargin=\"{}\" rightmargin=\"{}\" indent=\"{}\" leading=\"{}\">",
                number(a.left_margin),
                number(a.right_margin),
                number(a.indent),
                number(a.line_spacing)
            );
        }
        let _ = write!(out, "<p align=\"{}\">", align(p.attrs.align));
        let outer = p.runs.first().map(|(_, a)| *a).unwrap_or(p.attrs);
        let outer_attrs = font_attrs(outer, &face(&outer.face).0);
        out.push_str("<font");
        for (name, value) in &outer_attrs {
            let _ = write!(out, " {name}=\"{value}\"");
        }
        out.push('>');
        for (text, a) in &p.runs {
            let (family, bold, italic) = face(&a.face);
            let differing: Vec<_> = font_attrs(a, &family)
                .into_iter()
                .zip(&outer_attrs)
                .filter(|(mine, theirs)| mine.1 != theirs.1)
                .map(|(mine, _)| mine)
                .collect();
            if !differing.is_empty() {
                out.push_str("<font");
                for (name, value) in &differing {
                    let _ = write!(out, " {name}=\"{value}\"");
                }
                out.push('>');
            }
            let link = a.url.as_deref().filter(|u| links && !u.is_empty());
            if let Some(url) = link {
                let _ = write!(out, "<a href=\"{}\"", escape(url));
                if let Some(target) = a.target.as_deref().filter(|t| !t.is_empty()) {
                    let _ = write!(out, " target=\"{}\"", escape(target));
                }
                out.push('>');
            }
            if bold {
                out.push_str("<b>");
            }
            if italic {
                out.push_str("<i>");
            }
            out.push_str(&escape(text));
            if italic {
                out.push_str("</i>");
            }
            if bold {
                out.push_str("</b>");
            }
            if link.is_some() {
                out.push_str("</a>");
            }
            if !differing.is_empty() {
                out.push_str("</font>");
            }
        }
        out.push_str("</font></p>");
        if format {
            out.push_str("</textformat>");
        }
    }
    out
}

/// Whether two paragraphs share margins, indent and leading.
fn same_block_format(a: &TextAttrs, b: &TextAttrs) -> bool {
    a.left_margin == b.left_margin
        && a.right_margin == b.right_margin
        && a.indent == b.indent
        && a.line_spacing == b.line_spacing
}

/// The HTML name of an alignment.
fn align(a: Align) -> &'static str {
    match a {
        Align::Left => "left",
        Align::Right => "right",
        Align::Center => "center",
        Align::Justify => "justify",
    }
}

/// A finite value, or 0.
fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// A font size in whole points, as Flash 8 writes it (10.5 becomes 10).
fn whole_points(v: f64) -> String {
    let v = finite(v).trunc().clamp(-1e9, 1e9);
    format!("{}", v as i64)
}

/// A number without a needless fraction (`16`, `12.5`).
fn number(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_owned();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

/// Escapes text and attribute values as Flash 8 does (`"` included). A
/// line break inside a paragraph (U+2028) becomes `<br>`; NUL, which would
/// end the string in the tag, is dropped.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\u{2028}' => out.push_str("<br>"),
            '\0' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use jazari_xfl::model::Rgba;

    fn run(text: &str, face: &str, size: f64, fill: Rgba) -> TextRun {
        TextRun {
            text: text.to_owned(),
            attrs: TextAttrs {
                face: face.to_owned(),
                size,
                fill,
                ..TextAttrs::default()
            },
        }
    }

    /// The family and style of the faces these tests use.
    fn face(f: &str) -> DeviceFace {
        match f {
            "ArialMT" => ("Arial".into(), false, false),
            "Arial-BoldMT" => ("Arial".into(), true, false),
            "TimesNewRomanPS-ItalicMT" => ("Times New Roman".into(), false, true),
            other => (other.into(), false, false),
        }
    }

    #[test]
    fn single_run_matches_flash_8() {
        let runs = [run(
            "Dynamic text field",
            "ArialMT",
            16.0,
            Rgba::opaque(0x33, 0x33, 0x33),
        )];
        assert_eq!(
            field_html(&runs, &face, true),
            "<p align=\"left\"><font face=\"Arial\" size=\"16\" color=\"#333333\" letterSpacing=\"0.000000\" kerning=\"1\">Dynamic text field</font></p>"
        );
    }

    #[test]
    fn styled_runs_and_paragraphs_match_flash_8() {
        // The probe's field A: a bold red run, an italic blue Times run,
        // then a second paragraph, centred.
        let mut runs = vec![
            run("Hello ", "Arial-BoldMT", 14.0, Rgba::opaque(255, 0, 0)),
            run(
                "world",
                "TimesNewRomanPS-ItalicMT",
                18.0,
                Rgba::opaque(0, 0, 255),
            ),
            run("\rSecond para", "ArialMT", 14.0, Rgba::BLACK),
        ];
        for r in &mut runs {
            r.attrs.align = Align::Center;
        }
        assert_eq!(
            field_html(&runs, &face, true),
            "<p align=\"center\"><font face=\"Arial\" size=\"14\" color=\"#ff0000\" letterSpacing=\"0.000000\" kerning=\"1\"><b>Hello </b><font face=\"Times New Roman\" size=\"18\" color=\"#0000ff\"><i>world</i></font></font></p>\
             <p align=\"center\"><font face=\"Arial\" size=\"14\" color=\"#000000\" letterSpacing=\"0.000000\" kerning=\"1\">Second para</font></p>"
        );
    }

    #[test]
    fn escapes_sizes_spacing_and_input_links_match_flash_8() {
        // The probe's field C: input text, 10.5 pt, letter spacing 1.5, no
        // kerning, right-aligned, with a link that Flash 8 dropped.
        let mut r = run("a<b>&\"c\" link\rline2", "ArialMT", 10.5, Rgba::BLACK);
        r.attrs.letter_spacing = 1.5;
        r.attrs.auto_kern = false;
        r.attrs.align = Align::Right;
        r.attrs.url = Some("http://example.com/".into());
        assert_eq!(
            field_html(&[r], &face, false),
            "<p align=\"right\"><font face=\"Arial\" size=\"10\" color=\"#000000\" letterSpacing=\"1.500000\" kerning=\"0\">a&lt;b&gt;&amp;&quot;c&quot; link</font></p>\
             <p align=\"right\"><font face=\"Arial\" size=\"10\" color=\"#000000\" letterSpacing=\"1.500000\" kerning=\"0\">line2</font></p>"
        );
    }

    #[test]
    fn links_match_flash_8() {
        // The probe's static text E, as Flash 8 wrote it into a text field.
        let mut link = run("link", "ArialMT", 12.0, Rgba::BLACK);
        link.attrs.url = Some("http://example.com/".into());
        let runs = [
            run("Static ", "ArialMT", 12.0, Rgba::BLACK),
            link,
            run(" here", "ArialMT", 12.0, Rgba::BLACK),
        ];
        assert_eq!(
            field_html(&runs, &face, true),
            "<p align=\"left\"><font face=\"Arial\" size=\"12\" color=\"#000000\" letterSpacing=\"0.000000\" kerning=\"1\">Static <a href=\"http://example.com/\">link</a> here</font></p>"
        );
        let mut targeted = run("x", "ArialMT", 12.0, Rgba::BLACK);
        targeted.attrs.url = Some("u?a=1&b".into());
        targeted.attrs.target = Some("_blank".into());
        let html = field_html(&[targeted], &face, true);
        assert!(
            html.contains("<a href=\"u?a=1&amp;b\" target=\"_blank\">x</a>"),
            "{html}"
        );
    }

    #[test]
    fn empty_paragraphs_keep_their_format() {
        let runs = [run("a\r\rb\r", "X", 10.0, Rgba::BLACK)];
        let html = field_html(&runs, &face, true);
        assert_eq!(html.matches("<p ").count(), 4, "{html}");
        assert!(html.contains("kerning=\"1\"></font></p>"), "{html}");
    }

    #[test]
    fn differing_block_formats_get_textformat() {
        let mut b = run("b", "X", 10.0, Rgba::BLACK);
        b.attrs.indent = 5.0;
        let runs = [run("a\r", "X", 10.0, Rgba::BLACK), b];
        let html = field_html(&runs, &face, true);
        assert!(
            html.contains(
                "<textformat leftmargin=\"0\" rightmargin=\"0\" indent=\"5\" leading=\"2\"><p"
            ),
            "{html}"
        );
        assert!(html.ends_with("</p></textformat>"), "{html}");
    }
}
