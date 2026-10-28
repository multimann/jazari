//! Unit tests of the text module: record building, glyph outlines, font
//! tags, and small documents published end to end with the embedded fonts
//! only (no system fonts, no Ruffle).

use jazari_xfl::Document;
use jazari_xfl::model::{
    Element, Frame, Layer, LineType, Matrix, PathCmd, Pt, Rgba, Symbol, SymbolInstance, SymbolKind,
    TextAttrs, TextElement, TextKind, TextRun, Timeline,
};
use swf::{Color, EditTextFlag, FontFlag, ShapeRecord, Tag, TextRecord, Twips};

use super::font::{self, FONT_BUDGET};
use super::records::{self, Placed, RecordError};
use crate::{BuildMode, PublishOptions, PublishOutput, Severity, publish};

// ---------------------------------------------------------------- records

fn glyph(index: u16, x: f64, y: f64, advance: f64, line: usize) -> Placed {
    Placed {
        font: 1,
        index,
        x,
        y,
        advance,
        height: 240.0,
        color: Color::from_rgb(0, 255),
        line,
    }
}

fn entries(r: &TextRecord) -> Vec<(u32, i32)> {
    r.glyphs.iter().map(|g| (g.index, g.advance)).collect()
}

#[test]
fn records_follow_flash_8() {
    // Two lines, as Flash writes the text fixture's left-aligned text: the
    // first record sets font, height, colour and y but not x (it is 0); the
    // second sets x and y only; the last glyph of a line keeps its advance.
    let g = [
        glyph(6, 0.0, 320.0, 200.0, 0),
        glyph(11, 200.0, 320.0, 200.0, 0),
        glyph(12, 400.0, 320.0, 100.0, 0),
        glyph(20, 0.0, 760.0, 180.0, 1),
        glyph(11, 180.0, 760.0, 200.0, 1),
    ];
    let recs = records::build(&g).expect("records");
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[0].font_id, Some(1));
    assert_eq!(recs[0].height, Some(Twips::new(240)));
    assert_eq!(recs[0].color, Some(Color::from_rgb(0, 255)));
    assert_eq!(recs[0].x_offset, None);
    assert_eq!(recs[0].y_offset, Some(Twips::new(320)));
    assert_eq!(entries(&recs[0]), [(6, 200), (11, 200), (12, 100)]);
    assert_eq!(recs[1].font_id, None);
    assert_eq!(recs[1].height, None);
    assert_eq!(recs[1].color, None);
    assert_eq!(recs[1].x_offset, Some(Twips::ZERO));
    assert_eq!(recs[1].y_offset, Some(Twips::new(760)));
    assert_eq!(entries(&recs[1]), [(20, 180), (11, 200)]);
}

#[test]
fn a_first_line_off_the_origin_sets_x() {
    let recs = records::build(&[glyph(1, 1100.0, 420.0, 133.0, 0)]).expect("records");
    assert_eq!(recs[0].x_offset, Some(Twips::new(1100)));
}

#[test]
fn style_changes_mid_line_continue_the_pen() {
    let mut red = glyph(3, 300.0, 320.0, 100.0, 0);
    red.color = Color::from_rgb(0xFF0000, 255);
    let mut big = glyph(4, 400.0, 320.0, 150.0, 0);
    big.color = red.color;
    big.height = 480.0;
    let g = [glyph(1, 0.0, 320.0, 300.0, 0), red, big];
    let recs = records::build(&g).expect("records");
    assert_eq!(recs.len(), 3);
    assert_eq!(recs[1].color, Some(Color::from_rgb(0xFF0000, 255)));
    assert_eq!((recs[1].x_offset, recs[1].y_offset), (None, None));
    assert_eq!(recs[1].font_id, None);
    assert_eq!(recs[2].font_id, Some(1), "font and height are set together");
    assert_eq!(recs[2].height, Some(Twips::new(480)));
    assert_eq!(recs[2].color, None);
    assert_eq!(recs[2].x_offset, None);
}

#[test]
fn marks_get_records_of_their_own_and_offsets_fold_into_advances() {
    // A base, a raised mark drawn 30 twips left of the pen with no
    // advance, then the next base: the mark needs its own baseline, and
    // the advances carry the x offsets so no x is needed.
    let g = [
        glyph(1, 0.0, 560.0, 300.0, 0),
        glyph(2, 270.4, 400.0, 0.0, 0),
        glyph(3, 300.0, 560.0, 200.0, 0),
    ];
    let recs = records::build(&g).expect("records");
    assert_eq!(recs.len(), 3);
    assert_eq!(entries(&recs[0]), [(1, 270)]);
    assert_eq!(recs[1].y_offset, Some(Twips::new(400)));
    assert_eq!(recs[1].x_offset, None);
    assert_eq!(entries(&recs[1]), [(2, 30)]);
    assert_eq!(recs[2].y_offset, Some(Twips::new(560)));
    assert_eq!(recs[2].x_offset, None);
    assert_eq!(entries(&recs[2]), [(3, 200)]);
}

#[test]
fn records_hold_at_most_255_glyphs() {
    let g: Vec<Placed> = (0..600)
        .map(|i| glyph(1, f64::from(i) * 50.0, 300.0, 50.0, 0))
        .collect();
    let recs = records::build(&g).expect("records");
    assert_eq!(
        recs.iter().map(|r| r.glyphs.len()).collect::<Vec<_>>(),
        [255, 255, 90]
    );
    assert!(recs[1].x_offset.is_none(), "the pen continues");
}

#[test]
fn out_of_range_values_are_errors() {
    let far = glyph(1, 40_000.0, 300.0, 50.0, 0);
    assert!(matches!(
        records::build(&[far]),
        Err(RecordError::Offset(_))
    ));
    let mut huge = glyph(1, 0.0, 300.0, 50.0, 0);
    huge.height = 70_000.0;
    assert!(matches!(
        records::build(&[huge]),
        Err(RecordError::Height(_))
    ));
    let mut nan = glyph(1, 0.0, 300.0, 50.0, 0);
    nan.x = f64::NAN;
    assert!(records::build(&[nan]).is_err());
}

// ---------------------------------------------------------------- outlines

fn pt(x: f64, y: f64) -> Pt {
    Pt::new(x, y)
}

#[test]
fn outlines_are_on_the_design_grid_and_closed_like_flash() {
    // A 2048-unit face: one unit is 10 DefineFont3 units. The implied
    // point at half a unit rounds up (y down), as Flash 8 writes it.
    let u = |v: f64| v / 2048.0;
    let cmds = [
        PathCmd::MoveTo(pt(u(0.0), u(0.0))),
        PathCmd::LineTo(pt(u(100.0), u(0.0))),
        PathCmd::QuadTo(pt(u(100.0), u(-69.5)), pt(u(0.0), u(0.0))),
        PathCmd::MoveTo(pt(u(10.0), u(-10.0))),
        PathCmd::LineTo(pt(u(20.0), u(-10.0))),
        PathCmd::LineTo(pt(u(10.0), u(-10.0))),
    ];
    let (recs, bounds) = font::outline_records(&cmds, 2048.0);
    let fills: Vec<Option<u32>> = recs
        .iter()
        .filter_map(|r| match r {
            ShapeRecord::StyleChange(s) => Some(s.fill_style_0),
            _ => None,
        })
        .collect();
    assert_eq!(
        fills,
        [Some(1), None],
        "only the first contour sets the fill"
    );
    match &recs[2] {
        ShapeRecord::CurvedEdge { control_delta, .. } => {
            assert_eq!(control_delta.dy, Twips::new(-690), "{recs:?}");
        }
        other => panic!("expected a curve, got {other:?}"),
    }
    // The curved contour is closed by a zero-length line; the straight one
    // is not.
    assert!(matches!(
        recs[3],
        ShapeRecord::StraightEdge { delta } if delta.dx == Twips::ZERO && delta.dy == Twips::ZERO
    ));
    assert!(matches!(recs[4], ShapeRecord::StyleChange(_)));
    assert_eq!(recs.len(), 7, "{recs:?}");
    // Exact bounds include the curve's extremum (x = 1000, y about -345),
    // not its control point (y = -690).
    assert_eq!(bounds.x_min, Twips::ZERO);
    assert_eq!(bounds.x_max, Twips::new(1000));
    assert_eq!(bounds.y_min, Twips::new(-345));
    assert_eq!(bounds.y_max, Twips::ZERO);
}

#[test]
fn degenerate_curves_become_lines_and_empty_edges_vanish() {
    let cmds = [
        PathCmd::MoveTo(pt(0.0, 0.0)),
        PathCmd::QuadTo(pt(0.0, 0.0), pt(0.5, 0.0)),
        PathCmd::LineTo(pt(0.5, 0.0)),
        PathCmd::LineTo(pt(0.0, 0.0)),
    ];
    let (recs, _) = font::outline_records(&cmds, 1000.0);
    assert_eq!(recs.len(), 3, "{recs:?}");
    assert!(matches!(recs[1], ShapeRecord::StraightEdge { .. }));
    // A path without a MoveTo still starts with one.
    let (recs, _) = font::outline_records(&[PathCmd::LineTo(pt(0.1, 0.0))], 1000.0);
    assert!(matches!(&recs[0], ShapeRecord::StyleChange(s) if s.fill_style_0 == Some(1)));
}

// ---------------------------------------------------------------- documents

fn attrs(face: &str, size: f64) -> TextAttrs {
    TextAttrs {
        face: face.to_owned(),
        size,
        ..TextAttrs::default()
    }
}

fn text(kind: TextKind, runs: Vec<TextRun>) -> TextElement {
    TextElement {
        kind,
        matrix: Matrix::translate(22.0, 22.0),
        width: 200.0,
        height: 30.0,
        runs,
        auto_expand: true,
        selectable: false,
        ..TextElement::default()
    }
}

fn run_of(s: &str, a: TextAttrs) -> TextRun {
    TextRun {
        text: s.to_owned(),
        attrs: a,
    }
}

fn frame(index: u32, elements: Vec<Element>) -> Frame {
    Frame {
        index,
        duration: 1,
        elements,
        ..Frame::default()
    }
}

fn doc_with(frames: Vec<Frame>) -> Document {
    Document {
        scenes: vec![Timeline {
            name: "Scene 1".into(),
            layers: vec![Layer {
                name: "text".into(),
                frames,
                ..Layer::default()
            }],
        }],
        ..Document::default()
    }
}

fn publish_doc(doc: &Document) -> PublishOutput {
    let fonts = jazari_text::FontLibrary::embedded_only();
    let as2 = jazari_as2::Compiler::new(Default::default());
    publish(
        doc,
        &PublishOptions {
            mode: BuildMode::Release,
            compress: false,
        },
        &fonts,
        &as2,
    )
}

fn with_tags<R>(out: &PublishOutput, f: impl FnOnce(&[Tag<'_>]) -> R) -> R {
    let bytes = out.swf.as_ref().expect("swf");
    let buf = swf::decompress_swf(&bytes[..]).expect("decompress");
    let parsed = swf::parse_swf(&buf).expect("parse");
    f(&parsed.tags)
}

fn messages(out: &PublishOutput) -> Vec<String> {
    out.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// Every DefineFont3 as (id, name, glyph count, flags, codes).
fn fonts_of(out: &PublishOutput) -> Vec<(u16, String, usize, FontFlag, Vec<u16>)> {
    with_tags(out, |tags| {
        tags.iter()
            .filter_map(|t| match t {
                Tag::DefineFont2(f) => Some((
                    f.id,
                    f.name.to_str_lossy(swf::UTF_8).into_owned(),
                    f.glyphs.len(),
                    f.flags,
                    f.glyphs.iter().map(|g| g.code).collect(),
                )),
                _ => None,
            })
            .collect()
    })
}

#[test]
fn static_text_publishes_font_text_and_csm_in_order() {
    let el = text(
        TextKind::Static,
        vec![run_of("Hello\rworld", attrs("Noto Sans", 12.0))],
    );
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    assert!(out.diagnostics.is_empty(), "{:?}", messages(&out));
    with_tags(&out, |tags| {
        let kinds: Vec<&str> = tags.iter().map(crate::tags::tag_name).collect();
        let at = kinds
            .iter()
            .position(|k| *k == "DefineFont2/3")
            .expect("font");
        assert_eq!(
            kinds[at..at + 4],
            [
                "DefineFont2/3",
                "DefineText",
                "CSMTextSettings",
                "PlaceObject"
            ]
        );
        let Tag::PlaceObject(p) = &tags[at + 3] else {
            panic!("expected a placement after the text: {:?}", tags[at + 3]);
        };
        assert_eq!(
            p.matrix.map(|m| (m.tx, m.ty)),
            Some((Twips::new(440), Twips::new(440)))
        );
        let Tag::DefineText(t) = &tags[at + 1] else {
            unreachable!()
        };
        assert_eq!(t.records.len(), 2);
        assert_eq!(t.matrix, swf::Matrix::IDENTITY);
        assert!(t.bounds.x_max > t.bounds.x_min && t.bounds.y_max > t.bounds.y_min);
    });
    let fonts = fonts_of(&out);
    assert_eq!(fonts.len(), 1);
    let (_, name, n, flags, codes) = &fonts[0];
    assert_eq!(name, "Noto Sans");
    assert_eq!(*n, "Helowrd".len());
    assert!(flags.contains(FontFlag::HAS_WIDE_CODES | FontFlag::HAS_LAYOUT));
    assert!(!flags.contains(FontFlag::IS_ANSI));
    assert!(
        codes.windows(2).all(|w| w[0] < w[1]),
        "codes ascend: {codes:?}"
    );
}

#[test]
fn font_names_end_with_nul_as_flash_writes_them() {
    let el = text(
        TextKind::Static,
        vec![run_of("a", attrs("Noto Sans", 12.0))],
    );
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    let bytes = out.swf.expect("swf");
    let name = b"\x0aNoto Sans\0";
    assert!(
        bytes.windows(name.len()).any(|w| w == name),
        "length byte 10 followed by the NUL-terminated name"
    );
}

#[test]
fn one_font_holds_the_glyphs_of_every_text_even_in_later_frames_and_symbols() {
    // Frame 1 uses "ab"; frame 2 uses "bc" and a clip whose text uses "d".
    // The font is written with frame 1, so the scan must have seen all four.
    let mut d = doc_with(vec![
        frame(
            0,
            vec![Element::Text(text(
                TextKind::Static,
                vec![run_of("ab", attrs("Noto Sans", 12.0))],
            ))],
        ),
        frame(
            1,
            vec![
                Element::Text(text(
                    TextKind::Static,
                    vec![run_of("bc", attrs("Noto Sans", 20.0))],
                )),
                Element::Instance(SymbolInstance {
                    library_item: "clip".into(),
                    kind: SymbolKind::MovieClip,
                    ..SymbolInstance::default()
                }),
            ],
        ),
    ]);
    d.library.symbols.insert(
        "clip".into(),
        Symbol {
            name: "clip".into(),
            kind: SymbolKind::MovieClip,
            timeline: Timeline {
                name: "clip".into(),
                layers: vec![Layer {
                    name: "t".into(),
                    frames: vec![frame(
                        0,
                        vec![Element::Text(text(
                            TextKind::Static,
                            vec![run_of("d", attrs("Noto Sans", 12.0))],
                        ))],
                    )],
                    ..Layer::default()
                }],
            },
            ..Symbol::default()
        },
    );
    let out = publish_doc(&d);
    assert!(out.diagnostics.is_empty(), "{:?}", messages(&out));
    let fonts = fonts_of(&out);
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    assert_eq!(fonts[0].4, [u16::from(b'a'), 98, 99, 100]);
}

#[test]
fn identical_texts_share_a_character() {
    let el = || {
        Element::Text(text(
            TextKind::Static,
            vec![run_of("same", attrs("Noto Sans", 12.0))],
        ))
    };
    let out = publish_doc(&doc_with(vec![frame(0, vec![el()]), frame(1, vec![el()])]));
    let texts = with_tags(&out, |tags| {
        tags.iter()
            .filter(|t| matches!(t, Tag::DefineText(_)))
            .count()
    });
    assert_eq!(texts, 1);
}

#[test]
fn transparent_colours_use_define_text_2() {
    let mut a = attrs("Noto Sans", 12.0);
    a.fill = Rgba::new(10, 20, 30, 128);
    let out = publish_doc(&doc_with(vec![frame(
        0,
        vec![Element::Text(text(TextKind::Static, vec![run_of("x", a)]))],
    )]));
    with_tags(&out, |tags| {
        let t = tags
            .iter()
            .find_map(|t| match t {
                Tag::DefineText2(t) => Some(t),
                _ => None,
            })
            .expect("DefineText2");
        assert_eq!(t.records[0].color, Some(Color::from_rgb(0x0A141E, 128)));
    });
}

#[test]
fn arabic_static_text_is_joined_and_right_to_left() {
    // "marhaban" in a face that is not installed: Noto Sans Arabic stands in
    // (with a warning) and the letters take their joining forms.
    let el = text(
        TextKind::Static,
        vec![run_of("مرحبا", attrs("Some Missing Arabic Face", 28.0))],
    );
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    let msgs = messages(&out);
    assert_eq!(msgs.len(), 1, "{msgs:?}");
    assert!(
        msgs[0].contains("is not installed") && msgs[0].contains("Noto"),
        "{msgs:?}"
    );
    let fonts = fonts_of(&out);
    let codes = &fonts[0].4;
    // Initial meem, final reh, initial hah and final alef are Presentation
    // Forms; Noto draws the medial beh as a dotless form plus its dot
    // (Private Use codes). No base letter is left unshaped.
    for c in [0xFEE3, 0xFEAE, 0xFEA3, 0xFE8E] {
        assert!(codes.contains(&c), "{c:04X} in {codes:04X?}");
    }
    assert!(
        !codes.iter().any(|c| (0x0621..=0x064A).contains(c)),
        "{codes:04X?}"
    );
    // Visual order, left to right: alef ... meem.
    with_tags(&out, |tags| {
        let t = tags
            .iter()
            .find_map(|t| match t {
                Tag::DefineText(t) => Some(t),
                _ => None,
            })
            .expect("text");
        let shown: Vec<u16> = t
            .records
            .iter()
            .flat_map(|r| &r.glyphs)
            .map(|g| codes[g.index as usize])
            .collect();
        assert_eq!(shown.first(), Some(&0xFE8E), "{shown:04X?}");
        assert_eq!(shown.last(), Some(&0xFEE3), "{shown:04X?}");
    });
}

#[test]
fn harakat_get_their_own_records() {
    // Beh with kasra under it, then meem with sukun over it.
    let el = text(
        TextKind::Static,
        vec![run_of("بِسْمِ", attrs("Noto Naskh Arabic", 40.0))],
    );
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    assert!(out.diagnostics.is_empty(), "{:?}", messages(&out));
    with_tags(&out, |tags| {
        let t = tags
            .iter()
            .find_map(|t| match t {
                Tag::DefineText(t) => Some(t),
                _ => None,
            })
            .expect("text");
        let ys: Vec<Option<i32>> = t
            .records
            .iter()
            .map(|r| r.y_offset.map(Twips::get))
            .collect();
        assert!(t.records.len() > 1, "{:?}", t.records);
        let baseline = ys[0].expect("first record sets y");
        assert!(
            ys.iter().flatten().any(|&y| y != baseline),
            "some mark is raised or lowered: {ys:?}"
        );
    });
}

#[test]
fn missing_glyphs_and_links_and_orientation_are_reported() {
    let mut a = attrs("Noto Sans", 12.0);
    a.url = Some("http://example.com".into());
    let mut el = text(TextKind::Static, vec![run_of("a中", a)]);
    el.orientation = "vertical left to right".into();
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    let msgs = messages(&out).join("\n");
    assert!(msgs.contains("published horizontally"), "{msgs}");
    assert!(msgs.contains("links on static text"), "{msgs}");
    assert!(
        msgs.contains("\"中\"") && msgs.contains(".notdef"),
        "{msgs}"
    );
    assert!(out.swf.is_some());
    for d in &out.diagnostics {
        assert_eq!(d.severity, Severity::Warning, "{d}");
        assert_eq!(d.origin.layer.as_deref(), Some("text"), "{d}");
        assert!(d.origin.instance.is_some(), "{d}");
    }
}

#[test]
fn characters_drawn_by_a_fallback_are_reported() {
    // Noto Sans has no Arabic: Noto Sans Arabic draws it.
    let el = text(
        TextKind::Static,
        vec![run_of("abc سلام", attrs("Noto Sans", 12.0))],
    );
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    let msgs = messages(&out);
    assert_eq!(msgs.len(), 1, "{msgs:?}");
    assert!(
        msgs[0].contains("font 'Noto Sans' cannot draw") && msgs[0].contains("'Noto Sans Arabic'"),
        "{msgs:?}"
    );
}

#[test]
fn empty_static_text_defines_nothing() {
    let el = text(TextKind::Static, vec![run_of("", attrs("Noto Sans", 12.0))]);
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    assert!(out.diagnostics.is_empty(), "{:?}", messages(&out));
    assert!(fonts_of(&out).is_empty());
}

#[test]
fn texts_beyond_the_record_limits_are_left_out_with_an_error() {
    let el = text(
        TextKind::Static,
        vec![run_of("big", attrs("Noto Sans", 5000.0))],
    );
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    let errs: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(errs.len(), 1, "{:?}", messages(&out));
    assert!(errs[0].starts_with("text left out"), "{errs:?}");
    assert!(out.swf.is_some());
}

#[test]
fn large_faces_are_split_into_fonts_under_the_budget() {
    // Over 500 different Latin, Greek and Cyrillic letters on one line: far
    // more than one 60 KB DefineFont3 holds, and a line far wider than the
    // 1638 px a record offset reaches (the pen gets there by advances).
    let s: String = ('\u{0041}'..='\u{024F}')
        .chain('\u{0391}'..='\u{03C9}')
        .chain('\u{0410}'..='\u{044F}')
        .filter(|c| c.is_alphabetic())
        .collect();
    let el = text(TextKind::Static, vec![run_of(&s, attrs("Noto Sans", 12.0))]);
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    assert!(out.diagnostics.is_empty(), "{:?}", messages(&out));
    let fonts = fonts_of(&out);
    assert!(fonts.len() >= 2, "{} fonts", fonts.len());
    let glyphs: usize = fonts.iter().map(|f| f.2).sum();
    assert!(glyphs >= 500, "{glyphs} glyphs");
    let bytes = out.swf.expect("swf");
    let buf = swf::decompress_swf(&bytes[..]).expect("decompress");
    let lens = tag_lengths(&buf.data, 75);
    assert_eq!(lens.len(), fonts.len());
    assert!(lens.iter().all(|&n| n <= FONT_BUDGET), "{lens:?}");
}

/// Body lengths of the root tags with a given code, from the raw stream.
fn tag_lengths(data: &[u8], code: u16) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 2 <= data.len() {
        let head = u16::from_le_bytes([data[at], data[at + 1]]);
        let (len, start) = if head & 0x3f == 0x3f {
            let l = u32::from_le_bytes([data[at + 2], data[at + 3], data[at + 4], data[at + 5]]);
            (l as usize, at + 6)
        } else {
            (usize::from(head & 0x3f), at + 2)
        };
        if head >> 6 == code {
            out.push(len);
        }
        if head >> 6 == 0 {
            break;
        }
        at = start + len;
    }
    out
}

// ---------------------------------------------------------------- edit text

/// What the tests check of a DefineEditText.
struct Edit {
    flags: EditTextFlag,
    bounds: (i32, i32, i32, i32),
    height: Option<Twips>,
    layout: Option<swf::TextLayout>,
    max_length: Option<u16>,
    variable: String,
    html: String,
}

fn edit_of(out: &PublishOutput) -> Edit {
    with_tags(out, |tags| {
        let e = tags
            .iter()
            .find_map(|t| match t {
                Tag::DefineEditText(e) => Some(e),
                _ => None,
            })
            .expect("edit text");
        let b = e.bounds();
        Edit {
            flags: e.flags(),
            bounds: (b.x_min.get(), b.y_min.get(), b.x_max.get(), b.y_max.get()),
            height: e.height(),
            layout: e.layout().cloned(),
            max_length: e.max_length(),
            variable: e.variable_name().to_str_lossy(swf::UTF_8).into_owned(),
            html: e
                .initial_text()
                .map(|t| t.to_str_lossy(swf::UTF_8).into_owned())
                .unwrap_or_default(),
        }
    })
}

#[test]
fn dynamic_text_matches_flash_8() {
    let mut a = attrs("ArialMT", 16.0);
    a.fill = Rgba::opaque(0x33, 0x33, 0x33);
    let mut el = text(TextKind::Dynamic, vec![run_of("Dynamic text field", a)]);
    el.width = 126.5;
    el.height = 28.7;
    el.selectable = true;
    el.name = Some("dyn_txt".into());
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    assert!(out.diagnostics.is_empty(), "{:?}", messages(&out));
    let e = edit_of(&out);
    assert_eq!(
        e.html,
        "<p align=\"left\"><font face=\"Arial\" size=\"16\" color=\"#333333\" letterSpacing=\"0.000000\" kerning=\"1\">Dynamic text field</font></p>"
    );
    assert_eq!(
        e.flags,
        EditTextFlag::HAS_TEXT
            | EditTextFlag::READ_ONLY
            | EditTextFlag::HAS_TEXT_COLOR
            | EditTextFlag::HAS_FONT
            | EditTextFlag::HAS_LAYOUT
            | EditTextFlag::HTML
    );
    assert_eq!(e.bounds, (-40, -40, 2570, 614));
    assert_eq!(e.height, Some(Twips::new(320)));
    assert_eq!(e.layout.map(|l| l.leading), Some(Twips::new(40)));
    let fonts = fonts_of(&out);
    assert_eq!(fonts.len(), 1);
    assert_eq!((fonts[0].1.as_str(), fonts[0].2), ("Arial", 0));
    // The placement carries the instance name.
    with_tags(&out, |tags| {
        assert!(tags.iter().any(|t| matches!(
            t,
            Tag::PlaceObject(p) if p.name.map(|n| n.to_str_lossy(swf::UTF_8) == "dyn_txt") == Some(true)
        )));
    });
}

#[test]
fn input_text_flags_follow_the_field() {
    let mut a = attrs("_sans", 12.0);
    a.align = jazari_xfl::model::Align::Center;
    a.left_margin = 3.0;
    a.indent = -2.0;
    let mut el = text(TextKind::Input, vec![run_of("x", a)]);
    el.line_type = LineType::Multiline;
    el.border = true;
    el.selectable = true;
    el.max_chars = Some(10);
    el.variable = Some("answer".into());
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    let e = edit_of(&out);
    assert!(
        e.html
            .starts_with("<p align=\"center\"><font face=\"_sans\""),
        "{}",
        e.html
    );
    let f = e.flags;
    assert!(!f.contains(EditTextFlag::READ_ONLY));
    assert!(f.contains(EditTextFlag::MULTILINE | EditTextFlag::WORD_WRAP | EditTextFlag::BORDER));
    assert!(!f.contains(EditTextFlag::NO_SELECT | EditTextFlag::PASSWORD));
    assert_eq!(e.max_length, Some(10));
    let l = e.layout.expect("layout");
    assert_eq!((l.left_margin.get(), l.indent.get()), (60, -40));
    assert_eq!(e.variable, "answer");

    let mut el = text(
        TextKind::Input,
        vec![run_of("secret", attrs("_sans", 12.0))],
    );
    el.line_type = LineType::Password;
    let out = publish_doc(&doc_with(vec![frame(0, vec![Element::Text(el)])]));
    let e = edit_of(&out);
    assert!(
        e.flags
            .contains(EditTextFlag::PASSWORD | EditTextFlag::NO_SELECT)
    );
    assert!(!e.flags.contains(EditTextFlag::MULTILINE));
}

#[test]
fn arabic_in_dynamic_text_is_reported_and_device_fonts_are_shared() {
    let el = |s: &str| {
        Element::Text(text(
            TextKind::Dynamic,
            vec![run_of(s, attrs("Tahoma", 20.0))],
        ))
    };
    let out = publish_doc(&doc_with(vec![frame(0, vec![el("مرحبا"), el("hello")])]));
    let msgs = messages(&out);
    assert_eq!(msgs.len(), 1, "{msgs:?}");
    assert!(
        msgs[0].contains("Arabic in dynamic text") && msgs[0].contains("مرحبا"),
        "{msgs:?}"
    );
    let fonts = fonts_of(&out);
    assert_eq!(fonts.len(), 1, "one device font for both fields: {fonts:?}");
}

#[test]
fn device_faces_drop_postscript_style_suffixes() {
    let lib = jazari_text::FontLibrary::embedded_only();
    let f = |s: &str| super::edit::device_face(&lib, s);
    assert_eq!(f(""), ("_sans".to_owned(), false, false));
    assert_eq!(f("_serif"), ("_serif".to_owned(), false, false));
    assert_eq!(f("ArialMT"), ("Arial".to_owned(), false, false));
    assert_eq!(
        f("TimesNewRomanPSMT"),
        ("TimesNewRoman".to_owned(), false, false)
    );
    assert_eq!(f("Arial-BoldItalicMT"), ("Arial".to_owned(), true, true));
    assert_eq!(f("Noto Sans"), ("Noto Sans".to_owned(), false, false));
}

#[test]
fn hostile_text_never_panics() {
    let long: String = "مرحبا world ١٢٣ ".repeat(300);
    let faces = [
        "",
        "\0",
        "_typewriter",
        &"x".repeat(300),
        "Noto Naskh Arabic",
    ];
    let strings = [
        "",
        "\0nul\0",
        "\r\r\r",
        "\u{2028}line\u{2029}para",
        "\u{202E}bidi \u{2067}isolate\u{2069}",
        "\u{0651}\u{0650}",
        "中文 😀 \u{FFFF}",
        long.as_str(),
    ];
    let sizes = [f64::NAN, -5.0, 0.0, 0.01, 1e9, f64::INFINITY, 12.0];
    let mut elements = Vec::new();
    for (i, s) in strings.iter().enumerate() {
        for kind in [TextKind::Static, TextKind::Dynamic, TextKind::Input] {
            let mut a = attrs(faces[i % faces.len()], sizes[i % sizes.len()]);
            a.letter_spacing = if i % 2 == 0 { f64::NAN } else { -1e6 };
            a.line_spacing = 1e9;
            a.indent = f64::NEG_INFINITY;
            a.left_margin = -3.0;
            a.url = Some("\"<&>\0".into());
            let mut el = text(kind, vec![run_of(s, a.clone()), run_of(s, a)]);
            el.left = f64::NAN;
            el.top = 1e12;
            el.width = -1.0;
            el.height = f64::INFINITY;
            el.max_chars = Some(u32::MAX);
            el.variable = Some("v\0ar".into());
            el.line_type = [
                LineType::SingleLine,
                LineType::Multiline,
                LineType::MultilineNoWrap,
                LineType::Password,
            ][i % 4];
            el.auto_expand = i % 2 == 0;
            elements.push(Element::Text(el));
        }
    }
    let out = publish_doc(&doc_with(vec![frame(0, elements)]));
    for m in messages(&out) {
        eprintln!("{m:?}");
    }
    assert!(out.swf.is_some(), "{:?}", messages(&out));
    assert!(
        !messages(&out).iter().any(|m| m.contains("internal")),
        "{:?}",
        messages(&out)
    );
}
