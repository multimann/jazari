//! Text publishing compared with Flash 8's `text` and `as2` fixtures.
//!
//! The `text` comparisons need the fonts Flash 8 used (Arial, Times New
//! Roman, Times New Roman Bold Italic and Tahoma, as installed on Windows);
//! where they are missing they say so on stderr and check nothing. The
//! dynamic fields use device fonts and need no installed font.

mod common;

use std::collections::BTreeMap;

use common::{fixtures_dir, flash8, open, with_parsed};
use jazari_publish::{BuildMode, PublishOptions, PublishOutput, publish};
use jazari_text::{FaceMatch, FontLibrary, FontOrigin};
use swf::{CharacterId, ShapeRecord, Tag};

/// A font as the comparisons see it.
#[derive(Debug, Clone, PartialEq)]
struct FontInfo {
    name: String,
    bold_italic: u8,
    codes: Vec<u16>,
    outlines: Vec<Vec<(char, i32, i32)>>,
}

/// One DefineText record with the font named instead of numbered.
#[derive(Debug, Clone, PartialEq)]
struct Record {
    font: Option<(String, u8)>,
    color: Option<swf::Color>,
    x: Option<i32>,
    y: Option<i32>,
    height: Option<i32>,
    glyphs: Vec<(u32, i32)>,
}

/// A placement: depth, matrix and instance name.
type Place = (u16, Option<swf::Matrix>, Option<String>);

/// What the comparisons read from a movie.
#[derive(Debug, Default)]
struct Movie {
    fonts: BTreeMap<CharacterId, FontInfo>,
    /// DefineText records, in order of definition.
    texts: Vec<Vec<Record>>,
    /// Placements, in order.
    places: Vec<Place>,
    /// Each DefineEditText as debug text with its ids blanked, and its
    /// font's id.
    edits: Vec<(String, CharacterId)>,
    /// Number of CSMTextSettings tags.
    csm: usize,
}

fn read_movie(bytes: &[u8]) -> Movie {
    with_parsed(bytes, |s| {
        let mut m = Movie::default();
        for t in &s.tags {
            match t {
                Tag::DefineFont2(f) => {
                    m.fonts.insert(
                        f.id,
                        FontInfo {
                            name: f.name.to_str_lossy(swf::UTF_8).into_owned(),
                            bold_italic: f.flags.bits() & 3,
                            codes: f.glyphs.iter().map(|g| g.code).collect(),
                            outlines: f.glyphs.iter().map(|g| points(&g.shape_records)).collect(),
                        },
                    );
                }
                Tag::DefineText(t) | Tag::DefineText2(t) => {
                    let recs = t
                        .records
                        .iter()
                        .map(|r| Record {
                            font: r
                                .font_id
                                .and_then(|id| m.fonts.get(&id))
                                .map(|f| (f.name.clone(), f.bold_italic)),
                            color: r.color,
                            x: r.x_offset.map(|v| v.get()),
                            y: r.y_offset.map(|v| v.get()),
                            height: r.height.map(|v| v.get()),
                            glyphs: r.glyphs.iter().map(|g| (g.index, g.advance)).collect(),
                        })
                        .collect();
                    m.texts.push(recs);
                }
                Tag::DefineEditText(e) => {
                    let font = e.font_id().unwrap_or(0);
                    let blank = (**e)
                        .clone()
                        .with_id(0)
                        .with_font_id(0, e.height().unwrap_or_default());
                    m.edits.push((format!("{blank:?}"), font));
                }
                Tag::CsmTextSettings(_) => m.csm += 1,
                Tag::PlaceObject(p) => m.places.push((
                    p.depth,
                    p.matrix,
                    p.name.map(|n| n.to_str_lossy(swf::UTF_8).into_owned()),
                )),
                _ => {}
            }
        }
        m
    })
}

/// A glyph outline as absolute points (kind, x, y).
fn points(recs: &[ShapeRecord]) -> Vec<(char, i32, i32)> {
    let (mut x, mut y) = (0, 0);
    let mut out = Vec::new();
    for r in recs {
        match r {
            ShapeRecord::StyleChange(s) => {
                if let Some(m) = &s.move_to {
                    x = m.x.get();
                    y = m.y.get();
                    out.push(('M', x, y));
                }
            }
            ShapeRecord::StraightEdge { delta } => {
                x += delta.dx.get();
                y += delta.dy.get();
                out.push(('L', x, y));
            }
            ShapeRecord::CurvedEdge {
                control_delta,
                anchor_delta,
            } => {
                x += control_delta.dx.get();
                y += control_delta.dy.get();
                out.push(('C', x, y));
                x += anchor_delta.dx.get();
                y += anchor_delta.dy.get();
                out.push(('Q', x, y));
            }
        }
    }
    out
}

fn publish_with(name: &str, fonts: &FontLibrary) -> PublishOutput {
    let doc = open(name);
    let as2 = jazari_as2::Compiler::new(Default::default());
    publish(
        &doc,
        &PublishOptions {
            mode: BuildMode::Release,
            compress: true,
        },
        fonts,
        &as2,
    )
}

/// The installed fonts, or `None` (after a note) when a face of the `text`
/// fixture is missing.
fn system_fonts() -> Option<FontLibrary> {
    let lib = FontLibrary::new();
    for face in [
        "ArialMT",
        "TimesNewRomanPSMT",
        "TimesNewRomanPS-BoldItalicMT",
        "Tahoma",
    ] {
        let r = lib.resolve_detailed(face);
        let installed = r.matched != FaceMatch::Fallback
            && lib
                .face_info(r.font)
                .is_some_and(|i| i.origin == FontOrigin::System);
        if !installed {
            eprintln!("skipped: {face} is not installed");
            return None;
        }
    }
    Some(lib)
}

#[test]
fn latin_text_matches_flash_8() {
    let Some(lib) = system_fonts() else {
        return;
    };
    let out = publish_with("text", &lib);
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
    let ours = read_movie(out.swf.as_deref().expect("swf"));
    let flash = read_movie(&flash8("text"));

    // Fonts: the same faces, codes and outlines (Flash 8 keeps TrueType
    // outlines on the design grid and closes curved contours with a
    // zero-length line; so do we).
    let find = |m: &Movie, name: &str, style: u8| -> FontInfo {
        m.fonts
            .values()
            .find(|f| f.name == name && f.bold_italic == style)
            .unwrap_or_else(|| panic!("no font {name} {style}"))
            .clone()
    };
    for (name, style) in [("Arial", 0), ("Times New Roman", 0), ("Times New Roman", 3)] {
        assert_eq!(
            find(&ours, name, style),
            find(&flash, name, style),
            "{name} {style}"
        );
    }

    // Records of the four Latin texts: fonts, colours, positions, heights,
    // glyphs and advances equal (jazari-text rounds the nominal advance and
    // the kerning separately, as Flash does: ", A" in 12 pt Arial is 54).
    assert_eq!(ours.texts.len(), flash.texts.len());
    for (i, (a, b)) in ours.texts.iter().zip(&flash.texts).take(4).enumerate() {
        assert_eq!(a.len(), b.len(), "text {i}");
        for (ra, rb) in a.iter().zip(b) {
            assert_eq!(
                (&ra.font, ra.color, ra.x, ra.y, ra.height),
                (&rb.font, rb.color, rb.x, rb.y, rb.height),
                "text {i}"
            );
            assert_eq!(ra.glyphs, rb.glyphs, "text {i}");
        }
    }

    // Placements, and one CSMTextSettings per text and field.
    assert_eq!(ours.places, flash.places);
    assert_eq!((ours.csm, flash.csm), (6, 6));
}

#[test]
fn arabic_text_is_joined_and_right_to_left() {
    let Some(lib) = system_fonts() else {
        return;
    };
    let out = publish_with("text", &lib);
    let ours = read_movie(out.swf.as_deref().expect("swf"));
    let tahoma = ours
        .fonts
        .values()
        .find(|f| f.name == "Tahoma")
        .expect("Tahoma");
    let arabic = &ours.texts[4];
    let shown: Vec<u16> = arabic
        .iter()
        .flat_map(|r| &r.glyphs)
        .map(|g| tahoma.codes[g.0 as usize])
        .collect();
    // مرحبا بالعالم joined, as shown from left to right: final meem,
    // initial lam, final alef, medial ain, initial lam, final alef, initial
    // beh, space, final alef, medial beh, initial hah, final reh, initial
    // meem. Flash 8 wrote the 13 base letters unjoined, left to right.
    assert_eq!(
        shown,
        [
            0xFEE2, 0xFEDF, 0xFE8E, 0xFECC, 0xFEDF, 0xFE8E, 0xFE91, 0x0020, 0xFE8E, 0xFE92, 0xFEA3,
            0xFEAE, 0xFEE3
        ]
    );
    assert_eq!(arabic[0].y, Some(560), "the baseline Flash 8 used");
}

#[test]
fn dynamic_fields_match_flash_8() {
    for name in ["text", "as2"] {
        let out = publish_with(name, &FontLibrary::embedded_only());
        let ours = read_movie(out.swf.as_deref().expect("swf"));
        let flash = read_movie(&flash8(name));
        assert_eq!(ours.edits.len(), 1, "{name}");
        // Everything but the ids: bounds, flags, height, colour, layout,
        // variable and the HTML text.
        assert_eq!(ours.edits[0].0, flash.edits[0].0, "{name}");
        // Ours always names a glyph-less device font (Flash 8 points the
        // text fixture's field at the static text's embedded Arial).
        let font = &ours.fonts[&ours.edits[0].1];
        assert!(font.codes.is_empty(), "{name}");
        let expected = if name == "as2" { "_sans" } else { "Arial" };
        assert_eq!(font.name, expected, "{name}");
        // The field is placed where Flash places it, with its name (depths
        // are the timeline's business).
        let field = |m: &Movie| m.places.last().map(|p| (p.1, p.2.clone()));
        assert_eq!(field(&ours), field(&flash), "{name}");
    }
}

/// Writes `target/text-check/arabic-showcase.swf` with the installed fonts:
/// vowelled Arabic in an installed Naskh face and in the embedded fallback,
/// lam-alef, Arabic and Latin digits, a colour change mid-line, and Arabic
/// punctuation in a Latin face. It is for looking at, closely:
///
/// ```text
/// cargo test -p jazari-publish --test text -- --ignored
/// ```
#[test]
#[ignore]
fn write_arabic_showcase() {
    use jazari_xfl::model::{
        Element, Frame, Layer, Matrix, Rgba, TextAttrs, TextElement, TextKind, TextRun, Timeline,
    };
    let attrs = |face: &str, size: f64, fill: Rgba| TextAttrs {
        face: face.into(),
        size,
        fill,
        ..TextAttrs::default()
    };
    let text = |y: f64, runs: Vec<TextRun>| {
        Element::Text(TextElement {
            kind: TextKind::Static,
            matrix: Matrix::translate(20.0, y),
            width: 520.0,
            auto_expand: true,
            runs,
            selectable: false,
            ..TextElement::default()
        })
    };
    let run = |s: &str, a: TextAttrs| TextRun {
        text: s.into(),
        attrs: a,
    };
    let black = Rgba::BLACK;
    let red = Rgba::opaque(0xCC, 0, 0);
    let basmala = "بِسْمِ اللَّهِ الرَّحْمَٰنِ الرَّحِيمِ";
    let elements = vec![
        text(
            10.0,
            vec![run(basmala, attrs("Traditional Arabic", 40.0, black))],
        ),
        text(
            80.0,
            vec![run(basmala, attrs("Some Naskh Face", 36.0, black))],
        ),
        text(
            150.0,
            vec![
                run("لا إله إلا ", attrs("Tahoma", 32.0, black)),
                run("الله", attrs("Tahoma", 32.0, red)),
                run(" عام ٢٠٢٦ (2026)", attrs("Tahoma", 32.0, black)),
            ],
        ),
        text(
            210.0,
            vec![run(
                "سلام عليكم: ٣ كتب، 12 قلمًا؟",
                attrs("ArialMT", 32.0, black),
            )],
        ),
    ];
    let doc = jazari_xfl::Document {
        width: 560.0,
        height: 280.0,
        scenes: vec![Timeline {
            name: "Scene 1".into(),
            layers: vec![Layer {
                name: "text".into(),
                frames: vec![Frame {
                    index: 0,
                    duration: 1,
                    elements,
                    ..Frame::default()
                }],
                ..Layer::default()
            }],
        }],
        ..jazari_xfl::Document::default()
    };
    let lib = FontLibrary::new();
    let as2 = jazari_as2::Compiler::new(Default::default());
    let out = publish(
        &doc,
        &PublishOptions {
            mode: BuildMode::Release,
            compress: true,
        },
        &lib,
        &as2,
    );
    for d in &out.diagnostics {
        println!("{d}");
    }
    let dir = fixtures_dir().join("../../target/text-check");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("arabic-showcase.swf");
    std::fs::write(&path, out.swf.expect("swf")).expect("write");
    println!("wrote {}", path.display());
}
