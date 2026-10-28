//! Shaping and layout golden tests with the embedded Noto fonts (CPU only).
//!
//! The library is built with `FontLibrary::embedded_only()`, so these tests
//! give the same result on every machine. They also guard the harfrust pin:
//! a dependency bump that changes Arabic shaping fails here.

use std::sync::OnceLock;

use jazari_text::{
    FontId, FontLibrary, PositionedGlyph, TWIPS_PER_PX, TextBlock, TextLayout, layout,
};
use jazari_xfl::model::{Align, LineType, TextAttrs, TextElement, TextKind, TextRun};
use skrifa::MetadataProvider;

fn lib() -> &'static FontLibrary {
    static LIB: OnceLock<FontLibrary> = OnceLock::new();
    LIB.get_or_init(FontLibrary::embedded_only)
}

fn attrs(face: &str, size: f64, align: Align) -> TextAttrs {
    TextAttrs {
        face: face.to_owned(),
        size,
        align,
        ..TextAttrs::default()
    }
}

/// A one-run label (no wrapping).
fn label(face: &str, size: f64, text: &str) -> TextBlock {
    TextBlock {
        runs: vec![TextRun {
            text: text.to_owned(),
            attrs: attrs(face, size, Align::Left),
        }],
        auto_expand: true,
        ..TextBlock::default()
    }
}

/// A one-run block that wraps at `width` px.
fn wrapping(face: &str, size: f64, align: Align, width: f64, text: &str) -> TextBlock {
    TextBlock {
        runs: vec![TextRun {
            text: text.to_owned(),
            attrs: attrs(face, size, align),
        }],
        width,
        auto_expand: false,
        line_type: LineType::Multiline,
    }
}

/// The nominal glyph of a character in an embedded face.
fn cmap(font: FontId, c: char) -> u16 {
    let data = match font {
        f if f == lib().embedded().sans_arabic => jazari_text::embedded::NOTO_SANS_ARABIC,
        f if f == lib().embedded().naskh_arabic => jazari_text::embedded::NOTO_NASKH_ARABIC,
        _ => jazari_text::embedded::NOTO_SANS,
    };
    let font = skrifa::FontRef::new(data).expect("embedded font parses");
    let gid = font.charmap().map(c).expect("character is mapped").to_u32();
    u16::try_from(gid).expect("glyph id fits u16")
}

/// Pen position of each glyph of a line without trailing spaces.
fn pens(line: &jazari_text::LayoutLine) -> Vec<f64> {
    let mut pen = line.x;
    line.glyphs
        .iter()
        .map(|g| {
            let p = pen;
            pen += g.advance;
            p
        })
        .collect()
}

fn single_line(l: &TextLayout) -> &jazari_text::LayoutLine {
    assert_eq!(l.lines.len(), 1, "expected one line: {l:#?}");
    &l.lines[0]
}

#[test]
fn lam_alef_takes_its_ligature_forms() {
    // The embedded Noto fonts (2.x) do not have a single lam-alef glyph in
    // their shaping: their `rlig` lookups turn lam + alef into two special
    // contextual glyphs that draw the ligature together. (Fonts with a real
    // ligature glyph, such as Arial or Traditional Arabic, give one glyph
    // with code U+FEFB: see `system_fonts.rs`.) The golden result is
    // therefore two glyphs, neither of them an ordinary joining form.
    let e = lib().embedded();
    for (face, font) in [
        ("Traditional Arabic", e.naskh_arabic),
        ("Arabic Transparent", e.sans_arabic),
    ] {
        let l = layout(lib(), &label(face, 24.0, "لا"));
        let line = single_line(&l);
        assert!(line.rtl);
        let [alef, lam] = line.glyphs.as_slice() else {
            panic!("{face}: expected the two lam-alef glyphs, got {line:#?}");
        };
        assert_eq!(
            (alef.cluster, lam.cluster),
            (2, 0),
            "{face}: alef is drawn left of lam"
        );
        assert!(alef.font == font && lam.font == font, "{face}");
        assert_ne!(
            lam.glyph,
            cmap(font, '\u{FEDF}'),
            "{face}: not the plain initial lam"
        );
        assert_ne!(lam.glyph, cmap(font, 'ل'), "{face}: not the isolated lam");
        assert_ne!(
            alef.glyph,
            cmap(font, '\u{FE8E}'),
            "{face}: not the plain final alef"
        );
        // The special forms have no code point: they get Private Use codes.
        assert!((0xE000..=0xF8FF).contains(&alef.code) && (0xE000..=0xF8FF).contains(&lam.code));
        assert!(alef.x < lam.x && alef.advance > 0.0 && lam.advance > 0.0);
    }
}

#[test]
fn arabic_is_laid_out_right_to_left_and_joined() {
    let text = "مرحبا بالعالم";
    let l = layout(lib(), &label("Traditional Arabic", 24.0, text));
    let line = single_line(&l);
    assert!(line.rtl);
    // Visual order runs against logical order: clusters never increase
    // from left to right, and the first logical letter is rightmost.
    assert!(
        line.glyphs.windows(2).all(|w| w[0].cluster >= w[1].cluster),
        "{line:#?}"
    );
    let last = line.glyphs.last().expect("glyphs");
    assert_eq!(last.cluster, 0);
    let rightmost = line.glyphs.iter().map(|g| g.x).fold(f64::MIN, f64::max);
    assert_eq!(last.x, rightmost);
    // The line ends at its content's right edge.
    let right_edge = last.x + last.advance;
    assert!((right_edge - (line.x + line.width)).abs() < 1e-9);
    // Initial meem is joined: not the isolated form.
    let naskh = lib().embedded().naskh_arabic;
    assert!(line.glyphs.iter().all(|g| g.font == naskh));
    assert_ne!(last.glyph, cmap(naskh, 'م'));
    // One spacing glyph per character ("ال" is alef + lam, not lam-alef).
    // Noto Naskh Arabic draws the dots of beh as separate zero-advance mark
    // glyphs, so there are more glyphs than characters.
    assert_eq!(
        line.glyphs.iter().filter(|g| g.advance > 0.0).count(),
        text.chars().count()
    );
    assert_eq!(
        line.glyphs.len(),
        text.chars().count() + 2,
        "two dots below the two behs"
    );
    // Contextual forms, named by the Presentation Forms their codes carry:
    // initial meem, final reh, initial hah, ..., final meem.
    let code_at = |cluster: usize| {
        line.glyphs
            .iter()
            .find(|g| g.cluster == cluster && g.advance > 0.0)
            .map(|g| g.code)
    };
    assert_eq!(code_at(0), Some(0xFEE3));
    assert_eq!(code_at(2), Some(0xFEAE));
    assert_eq!(code_at(4), Some(0xFEA3));
    assert_eq!(code_at(text.len() - 2), Some(0xFEE2));
    // Noto's dotless beh has no code point of its own: a Private Use code.
    assert!(code_at(6).is_some_and(|c| (0xE000..=0xF8FF).contains(&c)));
}

#[test]
fn harakat_are_marks_with_offsets() {
    // Beh kasra, seen sukun, meem kasra.
    let text = "بِسْمِ";
    let l = layout(lib(), &label("Traditional Arabic", 36.0, text));
    let line = single_line(&l);
    let pens = pens(line);
    // Harakat keep their own code points as codes. (Noto Naskh Arabic also
    // draws beh's dot as a zero-advance mark, with a Private Use code.)
    let marks: Vec<(usize, &PositionedGlyph)> = line
        .glyphs
        .iter()
        .enumerate()
        .filter(|(_, g)| (0x064B..=0x0652).contains(&g.code))
        .collect();
    let mut codes: Vec<u16> = marks.iter().map(|(_, g)| g.code).collect();
    codes.sort_unstable();
    assert_eq!(
        codes,
        [0x0650, 0x0650, 0x0652],
        "kasra, kasra, sukun: {line:#?}"
    );
    assert!(
        marks.iter().all(|(_, g)| g.advance == 0.0),
        "marks do not advance the pen"
    );
    let mut any_dy = false;
    for (i, g) in &marks {
        let dx = g.x - pens[*i];
        let dy = g.y - line.baseline;
        assert!(
            dx.abs() > 0.5 || dy.abs() > 0.5,
            "mark {i} sits at the pen: {g:?}"
        );
        any_dy |= dy.abs() > 0.5;
        // A mark belongs to its base's cluster.
        assert!(
            line.glyphs
                .iter()
                .any(|b| b.cluster == g.cluster && b.advance > 0.0)
        );
    }
    assert!(any_dy, "some haraka must be raised or lowered");
    // Bases sit on the baseline.
    assert!(
        line.glyphs
            .iter()
            .filter(|g| g.advance > 0.0)
            .all(|g| g.y == line.baseline)
    );
}

#[test]
fn digits_stay_left_to_right_inside_arabic() {
    let text = "عام 2026";
    let l = layout(lib(), &label("Traditional Arabic", 24.0, text));
    let line = single_line(&l);
    assert!(line.rtl);
    let digit_start = text.find('2').expect("digits");
    let (digits, letters): (Vec<&PositionedGlyph>, Vec<&PositionedGlyph>) =
        line.glyphs.iter().partition(|g| g.cluster >= digit_start);
    assert_eq!(digits.len(), 4);
    let codes: Vec<u16> = digits.iter().map(|g| g.code).collect();
    assert_eq!(
        codes,
        [0x32, 0x30, 0x32, 0x36],
        "digits read 2026 from left to right"
    );
    assert!(digits.windows(2).all(|w| w[0].x < w[1].x));
    // The number comes after "عام" logically, so it is to its left.
    let digits_right = digits
        .iter()
        .map(|g| g.x + g.advance)
        .fold(f64::MIN, f64::max);
    let letters_left = letters
        .iter()
        .filter(|g| g.code != 0x20)
        .map(|g| g.x)
        .fold(f64::MAX, f64::min);
    assert!(digits_right <= letters_left + 1e-9, "{line:#?}");
}

#[test]
fn rtl_paragraph_wraps_with_right_alignment() {
    let text = "مرحبا بالعالم هذا نص عربي طويل بعض الشيء";
    let width_px = 120.0;
    let l = layout(
        lib(),
        &wrapping("Traditional Arabic", 20.0, Align::Right, width_px, text),
    );
    let box_width = width_px * TWIPS_PER_PX;
    assert_eq!(l.width, box_width);
    assert!(l.lines.len() >= 3, "{} lines", l.lines.len());
    let mut expected_start = 0;
    for (k, line) in l.lines.iter().enumerate() {
        assert!(line.rtl);
        assert_eq!(
            line.text.start, expected_start,
            "lines cover the text in order"
        );
        expected_start = line.text.end;
        assert!(line.width <= box_width, "line {k} overflows");
        // Flush right: origins are whole pixels, so within half a pixel.
        let right = line.x + line.width;
        assert!(
            (right - box_width).abs() <= 10.0,
            "line {k} ends at {right}, box {box_width}"
        );
        // Trailing spaces of an RTL line sit at its visual left.
        let trailing = text[line.text.clone()].ends_with(' ');
        if trailing {
            let first = line.glyphs.first().expect("glyphs");
            assert_eq!(first.code, 0x20, "line {k}: trailing space first visually");
            assert!(first.x < line.x);
        }
        // Lines stack downwards.
        if k > 0 {
            assert!(line.baseline > l.lines[k - 1].baseline);
        }
    }
    assert_eq!(expected_start, text.len());
    // The first word is at the top right.
    let top = &l.lines[0];
    let first_letter = top
        .glyphs
        .iter()
        .find(|g| g.cluster == 0)
        .expect("first letter on line 1");
    assert_eq!(
        first_letter.x,
        top.glyphs.iter().map(|g| g.x).fold(f64::MIN, f64::max)
    );
}

#[test]
fn latin_text_centres_in_the_box() {
    let text = "Hello, world\rsecond line";
    let width_px = 300.0;
    let block = TextBlock {
        runs: vec![TextRun {
            text: text.to_owned(),
            attrs: attrs("Arial", 18.0, Align::Center),
        }],
        width: width_px,
        auto_expand: true,
        line_type: LineType::SingleLine,
    };
    let l = layout(lib(), &block);
    assert_eq!(l.lines.len(), 2);
    let sans = lib().embedded().sans;
    for line in &l.lines {
        assert!(!line.rtl);
        assert!(
            line.glyphs.iter().all(|g| g.font == sans),
            "Arial falls back to Noto Sans"
        );
        let left = line.x;
        let right = width_px * TWIPS_PER_PX - (line.x + line.width);
        assert!(
            (left - right).abs() <= TWIPS_PER_PX,
            "left {left} right {right}"
        );
        assert_eq!(line.x % TWIPS_PER_PX, 0.0, "origin on a whole pixel");
        assert!(line.glyphs.windows(2).all(|w| w[0].x < w[1].x));
        assert!(line.glyphs.iter().all(|g| g.y == line.baseline));
    }
    let codes: String = l.lines[0]
        .glyphs
        .iter()
        .filter_map(|g| char::from_u32(g.code.into()))
        .collect();
    assert_eq!(codes, "Hello, world");
    assert_eq!(l.lines[0].glyphs[0].glyph, cmap(sans, 'H'));
}

#[test]
fn outlines_are_closed_quadratic_contours_in_ems() {
    use jazari_xfl::model::PathCmd;
    let e = lib().embedded();
    // The special lam form of Noto Naskh Arabic's lam-alef.
    let ligature = layout(lib(), &label("Traditional Arabic", 12.0, "لا")).lines[0].glyphs[1].glyph;
    // (font, glyph, minimum contours, has curves)
    let cases = [
        (e.sans, cmap(e.sans, 'o'), 2, true),
        (e.sans, cmap(e.sans, 'l'), 1, false),
        (e.sans, cmap(e.sans, 'a'), 2, true),
        (e.naskh_arabic, cmap(e.naskh_arabic, 'ب'), 2, true),
        (e.naskh_arabic, ligature, 1, true),
        (e.sans_arabic, cmap(e.sans_arabic, 'ه'), 2, true),
    ];
    for (font, glyph, min_contours, curved) in cases {
        let path = lib().glyph_outline(font, glyph);
        assert!(
            matches!(path.first(), Some(PathCmd::MoveTo(_))),
            "glyph {glyph}: starts with MoveTo"
        );
        let mut contours = 0;
        let mut start = None;
        let mut last = None;
        let mut quads = 0;
        let (mut min_y, mut max_y) = (f64::MAX, f64::MIN);
        for cmd in &path {
            let p = match *cmd {
                PathCmd::MoveTo(p) => {
                    assert_eq!(
                        start, last,
                        "glyph {glyph}: contour {contours} is not closed"
                    );
                    contours += 1;
                    start = Some(p);
                    p
                }
                PathCmd::LineTo(p) => p,
                PathCmd::QuadTo(c, p) => {
                    quads += 1;
                    assert!(c.x.abs() < 3.0 && c.y.abs() < 3.0);
                    p
                }
            };
            assert!(
                p.x.abs() < 3.0 && p.y.abs() < 3.0,
                "glyph {glyph}: point {p:?} is not in ems"
            );
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
            last = Some(p);
        }
        assert_eq!(start, last, "glyph {glyph}: last contour is not closed");
        assert!(
            contours >= min_contours,
            "glyph {glyph}: {contours} contours"
        );
        assert_eq!(
            quads > 0,
            curved,
            "glyph {glyph}: {quads} quadratic segments"
        );
        assert!(
            min_y < -0.3,
            "glyph {glyph}: y grows downwards, so ink is above zero ({min_y})"
        );
        let _ = max_y;
    }
    // 'l' sits on the baseline and rises to the ascender.
    let l = lib().glyph_outline(e.sans, cmap(e.sans, 'l'));
    let ys = l.iter().map(|c| match *c {
        PathCmd::MoveTo(p) | PathCmd::LineTo(p) | PathCmd::QuadTo(_, p) => p.y,
    });
    let max_y = ys.clone().fold(f64::MIN, f64::max);
    let min_y = ys.fold(f64::MAX, f64::min);
    assert!(
        max_y.abs() < 0.01 && min_y < -0.7,
        "l spans {min_y}..{max_y}"
    );
    assert!(
        lib().glyph_outline(e.sans, cmap(e.sans, ' ')).is_empty(),
        "a space has no outline"
    );
}

#[test]
fn punctuation_missing_from_arabic_fonts_falls_back_and_mirrors() {
    let text = "مرحبا (عالم)؟ \"نعم\"; a/b";
    let l = layout(lib(), &label("Traditional Arabic", 24.0, text));
    let line = single_line(&l);
    let e = lib().embedded();
    assert!(
        line.glyphs.iter().all(|g| g.glyph != 0),
        "no .notdef: {line:#?}"
    );
    // Noto Naskh Arabic has no ASCII brackets: Noto Sans supplies them.
    let close = text.find(')').expect("close paren");
    let open = text.find('(').expect("open paren");
    let g_close = line
        .glyphs
        .iter()
        .find(|g| g.cluster == close)
        .expect("close glyph");
    let g_open = line
        .glyphs
        .iter()
        .find(|g| g.cluster == open)
        .expect("open glyph");
    assert_eq!(g_close.font, e.sans);
    // In right-to-left text the logical ')' is drawn as '(' on the left.
    assert_eq!(g_close.glyph, cmap(e.sans, '('));
    assert_eq!(g_open.glyph, cmap(e.sans, ')'));
    assert!(g_close.x < g_open.x);
    // The Arabic question mark stays in the Arabic font.
    let q = text.find('؟').expect("question mark");
    assert_eq!(
        line.glyphs
            .iter()
            .find(|g| g.cluster == q)
            .expect("؟ glyph")
            .font,
        e.naskh_arabic
    );
}

#[test]
fn latin_inside_arabic_and_arabic_inside_latin_get_glyphs() {
    let e = lib().embedded();
    for (face, text) in [
        ("Traditional Arabic", "كلمة Word كلمة"),
        ("Arial", "Word كلمة word"),
    ] {
        let l = layout(lib(), &label(face, 20.0, text));
        let line = single_line(&l);
        assert!(
            line.glyphs.iter().all(|g| g.glyph != 0),
            "{face}: no .notdef"
        );
        for g in &line.glyphs {
            let c = text[g.cluster..].chars().next().expect("cluster char");
            if c.is_ascii_alphabetic() {
                assert_eq!(g.font, e.sans, "{face}: Latin from Noto Sans");
            } else if !c.is_whitespace() {
                assert_ne!(g.font, e.sans, "{face}: Arabic from an Arabic font");
            }
        }
    }
    // A Naskh-like face prefers Noto Naskh Arabic, others Noto Sans Arabic.
    let naskh = layout(lib(), &label("Simplified Arabic", 20.0, "كلمة"));
    assert!(naskh.glyphs().all(|g| g.font == e.naskh_arabic));
    let sans = layout(lib(), &label("Tahoma", 20.0, "كلمة"));
    assert!(sans.glyphs().all(|g| g.font == e.sans_arabic));
}

#[test]
fn paragraphs_stack_by_metrics_and_line_spacing() {
    let mut a = attrs("Arial", 20.0, Align::Left);
    a.line_spacing = 5.0;
    let block = TextBlock {
        runs: vec![TextRun {
            text: "one\rtwo\r\rfour\r".to_owned(),
            attrs: a,
        }],
        auto_expand: true,
        ..TextBlock::default()
    };
    let l = layout(lib(), &block);
    assert_eq!(
        l.lines.len(),
        5,
        "an empty paragraph and a trailing CR each make a line"
    );
    let m = lib().metrics(lib().embedded().sans);
    let size = 20.0 * TWIPS_PER_PX;
    let (asc, desc) = (m.ascent * size, m.descent * size);
    let snap = |t: f64| (t / TWIPS_PER_PX).round() * TWIPS_PER_PX;
    assert_eq!(l.lines[0].baseline, snap(asc));
    for k in 1..5 {
        let expected = snap(l.lines[k - 1].baseline + desc + 5.0 * TWIPS_PER_PX + asc);
        assert_eq!(l.lines[k].baseline, expected, "line {k}");
    }
    assert!(l.lines[2].glyphs.is_empty() && l.lines[4].glyphs.is_empty());
    assert_eq!(l.height, l.lines[4].baseline + desc);
    assert!(layout(lib(), &TextBlock::default()).lines.is_empty());
}

#[test]
fn letter_spacing_widens_latin_but_not_arabic() {
    let measure = |face: &str, text: &str, spacing: f64| {
        let mut a = attrs(face, 20.0, Align::Left);
        a.letter_spacing = spacing;
        let block = TextBlock {
            runs: vec![TextRun {
                text: text.to_owned(),
                attrs: a,
            }],
            auto_expand: true,
            ..TextBlock::default()
        };
        layout(lib(), &block).lines[0].width
    };
    let plain = measure("Arial", "abcd", 0.0);
    let spaced = measure("Arial", "abcd", 2.0);
    // After each of the three inner clusters and the last one.
    assert_eq!(spaced - plain, 4.0 * 2.0 * TWIPS_PER_PX);
    assert_eq!(
        measure("Traditional Arabic", "كلمة", 2.0),
        measure("Traditional Arabic", "كلمة", 0.0)
    );
}

#[test]
fn margins_and_indent_offset_lines() {
    let mut a = attrs("Arial", 12.0, Align::Left);
    a.left_margin = 10.0;
    a.right_margin = 20.0;
    a.indent = 15.0;
    let block = TextBlock {
        runs: vec![TextRun {
            text: "word word word word word word word word".to_owned(),
            attrs: a.clone(),
        }],
        width: 150.0,
        auto_expand: false,
        line_type: LineType::Multiline,
    };
    let l = layout(lib(), &block);
    assert!(l.lines.len() >= 2);
    assert_eq!(l.lines[0].x, 25.0 * TWIPS_PER_PX, "left margin + indent");
    assert_eq!(l.lines[1].x, 10.0 * TWIPS_PER_PX, "left margin only");
    let area = (150.0 - 10.0 - 20.0) * TWIPS_PER_PX;
    assert!(l.lines[0].width <= area - 15.0 * TWIPS_PER_PX);
    assert!(l.lines.iter().skip(1).all(|line| line.width <= area));

    // Right-aligned Arabic: the indent is at the right (start) edge.
    a.face = "Traditional Arabic".to_owned();
    a.align = Align::Right;
    let block = TextBlock {
        runs: vec![TextRun {
            text: "كلمة كلمة كلمة كلمة كلمة كلمة كلمة".to_owned(),
            attrs: a,
        }],
        width: 150.0,
        auto_expand: false,
        line_type: LineType::Multiline,
    };
    let l = layout(lib(), &block);
    assert!(l.lines.len() >= 2);
    let right = |k: usize| l.lines[k].x + l.lines[k].width;
    assert!((right(0) - (150.0 - 20.0 - 15.0) * TWIPS_PER_PX).abs() <= 10.0);
    assert!((right(1) - (150.0 - 20.0) * TWIPS_PER_PX).abs() <= 10.0);
}

#[test]
fn justify_stretches_all_but_the_last_line() {
    for (face, text) in [
        (
            "Arial",
            "the quick brown fox jumps over the lazy dog again and again",
        ),
        (
            "Traditional Arabic",
            "نص عربي طويل يملأ عدة أسطر في هذا الصندوق الضيق جدا",
        ),
    ] {
        let l = layout(lib(), &wrapping(face, 16.0, Align::Justify, 160.0, text));
        let n = l.lines.len();
        assert!(n >= 3, "{face}: {n} lines");
        let area = 160.0 * TWIPS_PER_PX;
        for line in &l.lines[..n - 1] {
            assert!(
                (line.width - area).abs() < 1e-6,
                "{face}: stretched to the box"
            );
            assert!(line.x.abs() < 1e-9);
        }
        let last = &l.lines[n - 1];
        assert!(last.width < area);
        if last.rtl {
            assert!(
                (last.x + last.width - area).abs() <= 10.0,
                "{face}: last RTL line at the right"
            );
        } else {
            assert_eq!(last.x, 0.0, "{face}: last LTR line at the left");
        }
    }
}

#[test]
fn auto_expanding_labels_grow_to_fit() {
    let mut block = label("Arial", 20.0, "a label much wider than its box");
    block.width = 10.0;
    let l = layout(lib(), &block);
    assert_eq!(l.lines.len(), 1);
    assert_eq!(l.width, l.lines[0].width);
    assert!(l.width > 10.0 * TWIPS_PER_PX);
}

#[test]
fn blocks_from_static_text_elements() {
    let el = TextElement {
        kind: TextKind::Static,
        width: 80.0,
        auto_expand: false,
        runs: vec![TextRun {
            text: "x".to_owned(),
            attrs: TextAttrs::default(),
        }],
        ..TextElement::default()
    };
    let b = TextBlock::from_element(&el);
    assert!(b.wraps());
    assert_eq!(b.line_type, LineType::Multiline);
    let label = TextBlock::from_element(&TextElement {
        auto_expand: true,
        ..el.clone()
    });
    assert!(!label.wraps());
    let dynamic = TextElement {
        kind: TextKind::Dynamic,
        line_type: LineType::SingleLine,
        ..el
    };
    assert!(!TextBlock::from_element(&dynamic).wraps());
}

#[test]
fn runs_keep_their_sizes_and_join_across_colour_changes() {
    let mut red = attrs("Traditional Arabic", 24.0, Align::Left);
    red.fill = jazari_xfl::model::Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let black = attrs("Traditional Arabic", 24.0, Align::Left);
    // "سلام" split inside the word: the letters must still join.
    let block = TextBlock {
        runs: vec![
            TextRun {
                text: "سل".to_owned(),
                attrs: black.clone(),
            },
            TextRun {
                text: "ام".to_owned(),
                attrs: red,
            },
        ],
        auto_expand: true,
        ..TextBlock::default()
    };
    let split = layout(lib(), &block);
    let whole = layout(lib(), &label("Traditional Arabic", 24.0, "سلام"));
    let ids = |l: &TextLayout| l.glyphs().map(|g| g.glyph).collect::<Vec<_>>();
    assert_eq!(
        ids(&split),
        ids(&whole),
        "same contextual forms across runs"
    );
    let runs: Vec<usize> = split.glyphs().map(|g| g.run).collect();
    assert!(runs.contains(&0) && runs.contains(&1));

    // Sizes come from each glyph's run.
    let mut big = black.clone();
    big.size = 48.0;
    let block = TextBlock {
        runs: vec![
            TextRun {
                text: "ab".to_owned(),
                attrs: big,
            },
            TextRun {
                text: "cd".to_owned(),
                attrs: black,
            },
        ],
        auto_expand: true,
        ..TextBlock::default()
    };
    let l = layout(lib(), &block);
    let sizes: Vec<f64> = l.glyphs().map(|g| g.size).collect();
    assert_eq!(sizes, [960.0, 960.0, 480.0, 480.0]);
}

/// Exact glyphs, positions and advances for one vowelled word, at 50 px so
/// that twips equal Noto's font units. A change here means harfrust, the
/// embedded font or the layout rules changed: check the rendering, then
/// update the numbers.
#[test]
fn golden_vowelled_word_in_noto_naskh() {
    let l = layout(lib(), &label("Traditional Arabic", 50.0, "بِسْمِ"));
    let line = single_line(&l);
    assert_eq!(line.x, 0.0);
    assert_eq!(line.baseline, 1060.0, "round(1069 / 20) px");
    let got: Vec<(u16, f64, f64, f64)> = line
        .glyphs
        .iter()
        .map(|g| (g.glyph, g.x, g.y - line.baseline, g.advance))
        .collect();
    let expected = [
        (436, 238.0, 62.0, 0.0),   // kasra under meem
        (75, 0.0, 0.0, 528.0),     // final meem
        (388, 822.0, 222.0, 0.0),  // sukun over seen
        (34, 528.0, 0.0, 663.0),   // medial seen
        (436, 1230.0, 201.0, 0.0), // kasra under beh
        (323, 1261.0, 31.0, 0.0),  // beh's dot
        (19, 1191.0, 0.0, 275.0),  // initial dotless beh
    ];
    assert_eq!(got, expected);
    let ids: Vec<u16> = layout(lib(), &label("Traditional Arabic", 50.0, "مرحبا بالعالم"))
        .glyphs()
        .map(|g| g.glyph)
        .collect();
    assert_eq!(
        ids,
        [75, 70, 9, 46, 70, 9, 323, 19, 3, 9, 323, 16, 25, 29, 77]
    );
}

#[test]
fn document_fonts_win_over_embedded_ones() {
    let mut lib = FontLibrary::embedded_only();
    let embedded = lib.resolve("NotoSans-Regular");
    assert_eq!(
        lib.add_fonts_dir(
            std::path::Path::new("no/such/dir"),
            jazari_text::FontOrigin::Document
        ),
        0
    );
    let added = lib
        .add_font_data(
            jazari_text::embedded::NOTO_SANS.to_vec(),
            jazari_text::FontOrigin::Document,
        )
        .expect("Noto Sans loads");
    assert_eq!(added.len(), 1);
    assert_ne!(added[0], embedded);
    assert_eq!(
        lib.resolve("NotoSans-Regular"),
        added[0],
        "the document copy wins"
    );
    assert!(
        lib.add_font_data(b"not a font".to_vec(), jazari_text::FontOrigin::Document)
            .is_err()
    );
    let a = cmap(embedded, 'a');
    let advance = lib.glyph_advance(added[0], a);
    assert!(
        (0.4..0.7).contains(&advance),
        "advance of 'a' in ems: {advance}"
    );
}

#[test]
fn font_library_is_shareable_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<FontLibrary>();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                layout(lib(), &label("Traditional Arabic", 20.0, "مرحبا"))
                    .lines
                    .len()
            })
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().expect("layout thread"), 1);
    }
}

/// A colour change inside a run of one face must not change the glyphs:
/// digits and punctuation after the cut stay in the face they would have
/// used without it (they used to jump to Noto Sans).
#[test]
fn colour_changes_do_not_change_fonts() {
    for face in ["Tahoma", "Traditional Arabic"] {
        let whole = layout(lib(), &label(face, 20.0, "عام 2026، نعم"));
        let mut red = attrs(face, 20.0, Align::Left);
        red.fill = jazari_xfl::model::Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        let split = TextBlock {
            runs: vec![
                TextRun {
                    text: "عام ".to_owned(),
                    attrs: attrs(face, 20.0, Align::Left),
                },
                TextRun {
                    text: "2026، نعم".to_owned(),
                    attrs: red,
                },
            ],
            auto_expand: true,
            ..TextBlock::default()
        };
        let split = layout(lib(), &split);
        let glyphs = |l: &TextLayout| {
            l.glyphs()
                .map(|g| (g.font, g.glyph, g.x, g.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(glyphs(&split), glyphs(&whole), "{face}");
    }
}

/// A haraka in a run with other shaping options (here autoKern off) is
/// still shaped with its base, so it keeps its mark-to-base attachment
/// instead of sitting on the baseline between letters.
#[test]
fn marks_in_their_own_run_stay_attached() {
    let face = "Traditional Arabic";
    let whole = layout(lib(), &label(face, 20.0, "بِت"));
    let mut unkerned = attrs(face, 20.0, Align::Left);
    unkerned.auto_kern = false;
    let split = TextBlock {
        runs: vec![
            TextRun {
                text: "ب".to_owned(),
                attrs: attrs(face, 20.0, Align::Left),
            },
            TextRun {
                text: "\u{0650}".to_owned(),
                attrs: unkerned,
            },
            TextRun {
                text: "ت".to_owned(),
                attrs: attrs(face, 20.0, Align::Left),
            },
        ],
        auto_expand: true,
        ..TextBlock::default()
    };
    let split = layout(lib(), &split);
    let placed = |l: &TextLayout| l.glyphs().map(|g| (g.glyph, g.x, g.y)).collect::<Vec<_>>();
    assert_eq!(placed(&split), placed(&whole));
    // The kasra joins its base's cluster; it is the zero-advance glyph
    // drawn below the baseline.
    let kasra = split
        .glyphs()
        .find(|g| g.advance == 0.0 && g.y != split.lines[0].baseline)
        .expect("the kasra is drawn with an offset");
    assert!(
        kasra.y > split.lines[0].baseline,
        "kasra sits below the baseline"
    );
}
