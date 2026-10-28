//! Hostile and extreme documents, built in code: the publisher must finish
//! quickly, never panic, never overflow the stack, and keep what it can.
//! Each test documents the defect it guards against.

use std::sync::mpsc;
use std::time::Duration;

use jazari_publish::{BuildMode, PublishOptions, PublishOutput, Severity, publish};
use jazari_xfl::Document;
use jazari_xfl::model::{
    ColorEffect, Edge, Element, FillStyle, Frame, GraphicPlayback, Layer, LayerKind, Matrix,
    PathCmd, Pt, Rgba, Shape, Symbol, SymbolInstance, SymbolKind, Timeline,
};
use swf::{PlaceObjectAction, Tag};

fn square(size: f64) -> Element {
    Element::Shape(Shape {
        fills: vec![FillStyle::Solid(Rgba::opaque(200, 0, 0))],
        edges: vec![Edge {
            fill0: 0,
            fill1: 1,
            stroke: 0,
            path: vec![
                PathCmd::MoveTo(Pt::new(0.0, 0.0)),
                PathCmd::LineTo(Pt::new(size, 0.0)),
                PathCmd::LineTo(Pt::new(size, size)),
                PathCmd::LineTo(Pt::new(0.0, size)),
                PathCmd::LineTo(Pt::new(0.0, 0.0)),
            ],
        }],
        ..Shape::default()
    })
}

fn inst(item: &str, kind: SymbolKind) -> SymbolInstance {
    SymbolInstance {
        library_item: item.to_owned(),
        kind,
        graphic: (kind == SymbolKind::Graphic).then(GraphicPlayback::default),
        ..SymbolInstance::default()
    }
}

fn kf(index: u32, duration: u32, elements: Vec<Element>) -> Frame {
    Frame {
        index,
        duration,
        elements,
        ..Frame::default()
    }
}

fn layer(name: &str, frames: Vec<Frame>) -> Layer {
    Layer {
        name: name.to_owned(),
        frames,
        ..Layer::default()
    }
}

fn doc(layers: Vec<Layer>) -> Document {
    Document {
        scenes: vec![Timeline {
            name: "Scene 1".into(),
            layers,
        }],
        ..Document::default()
    }
}

fn add_symbol(doc: &mut Document, name: &str, kind: SymbolKind, layers: Vec<Layer>) {
    doc.library.symbols.insert(
        name.to_owned(),
        Symbol {
            name: name.to_owned(),
            kind,
            timeline: Timeline {
                name: name.to_owned(),
                layers,
            },
            ..Symbol::default()
        },
    );
}

/// Publishes on a thread with `stack` bytes of stack (1 MiB is the
/// Windows main-thread size the CLI runs with) and fails the test when it
/// takes longer than `limit`.
fn publish_bounded(doc: Document, stack: usize, limit: Duration) -> PublishOutput {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let fonts = jazari_text::FontLibrary::embedded_only();
            let as2 = jazari_as2::Compiler::new(Default::default());
            let out = publish(
                &doc,
                &PublishOptions {
                    mode: BuildMode::Release,
                    compress: false,
                },
                &fonts,
                &as2,
            );
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(limit)
        .unwrap_or_else(|_| panic!("publish did not finish within {limit:?} (or crashed)"))
}

fn errors(out: &PublishOutput) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(ToString::to_string)
        .collect()
}

fn parsed_tags(out: &PublishOutput, f: impl FnOnce(&[Tag<'_>])) {
    let bytes = out.swf.as_ref().expect("swf");
    let buf = swf::decompress_swf(&bytes[..]).expect("decompress");
    let parsed = swf::parse_swf(&buf).expect("parse");
    f(&parsed.tags);
}

/// `levels` graphic symbols, each holding `fan` instances of the next; the
/// last holds `leaf`. Flattening visits fan^levels instances per frame
/// unless the planner prunes what cannot be placed.
fn fan_out(fan: usize, levels: usize, leaf: Vec<Element>) -> Document {
    let mut d = doc(vec![layer(
        "l",
        vec![kf(
            0,
            2,
            vec![Element::Instance(inst("G0", SymbolKind::Graphic))],
        )],
    )]);
    for lv in 0..levels {
        let elements = if lv + 1 == levels {
            leaf.clone()
        } else {
            (0..fan)
                .map(|_| Element::Instance(inst(&format!("G{}", lv + 1), SymbolKind::Graphic)))
                .collect()
        };
        add_symbol(
            &mut d,
            &format!("G{lv}"),
            SymbolKind::Graphic,
            vec![layer("l", vec![kf(0, 1, elements)])],
        );
    }
    d
}

/// Exponential flattening with content: 12^7 shapes would need far more
/// than 16383 depths. Everything past the last depth is left out, so the
/// planner must stop there instead of visiting every instance.
#[test]
fn exponential_graphic_nesting_with_content_finishes() {
    let d = fan_out(12, 8, vec![square(10.0)]);
    let out = publish_bounded(d, 8 << 20, Duration::from_secs(60));
    assert!(out.swf.is_some());
    assert!(
        errors(&out).iter().any(|e| e.contains("depths")),
        "{:?}",
        errors(&out)
    );
}

/// Exponential flattening of empty graphics: nothing takes a depth, so a
/// depth limit alone does not stop the walk.
#[test]
fn exponential_graphic_nesting_without_content_finishes() {
    let d = fan_out(12, 9, Vec::new());
    let out = publish_bounded(d, 8 << 20, Duration::from_secs(60));
    assert!(out.swf.is_some());
}

/// A chain of movie clips as deep as the nesting limit allows, published
/// on a 1 MiB stack (the Windows main thread, where the CLI publishes).
#[test]
fn deep_movie_clip_nesting_fits_a_small_stack() {
    let levels = 80;
    let mut d = doc(vec![layer(
        "l",
        vec![kf(
            0,
            1,
            vec![Element::Instance(inst("M0", SymbolKind::MovieClip))],
        )],
    )]);
    for lv in 0..levels {
        let elements = if lv + 1 == levels {
            vec![square(10.0)]
        } else {
            vec![
                square(5.0),
                Element::Instance(inst(&format!("M{}", lv + 1), SymbolKind::MovieClip)),
            ]
        };
        add_symbol(
            &mut d,
            &format!("M{lv}"),
            SymbolKind::MovieClip,
            vec![layer("l", vec![kf(0, 1, elements)])],
        );
    }
    let out = publish_bounded(d, 1 << 20, Duration::from_secs(60));
    assert!(out.swf.is_some());
}

/// Movie clips and graphics alternating: each kind has its own nesting
/// guard, so together they nest twice as deep.
#[test]
fn deep_mixed_nesting_fits_a_small_stack() {
    let levels = 140;
    let mut d = doc(vec![layer(
        "l",
        vec![kf(
            0,
            1,
            vec![Element::Instance(inst("S0", SymbolKind::MovieClip))],
        )],
    )]);
    for lv in 0..levels {
        let kind = if lv % 2 == 0 {
            SymbolKind::MovieClip
        } else {
            SymbolKind::Graphic
        };
        let next_kind = if lv % 2 == 0 {
            SymbolKind::Graphic
        } else {
            SymbolKind::MovieClip
        };
        let elements = if lv + 1 == levels {
            vec![square(10.0)]
        } else {
            vec![Element::Instance(inst(&format!("S{}", lv + 1), next_kind))]
        };
        add_symbol(
            &mut d,
            &format!("S{lv}"),
            kind,
            vec![layer("l", vec![kf(0, 1, elements)])],
        );
    }
    let out = publish_bounded(d, 1 << 20, Duration::from_secs(60));
    assert!(out.swf.is_some());
}

/// Extreme but finite instance properties must be clamped to what the SWF
/// fields hold, not make the whole placement fail its round trip: matrix
/// scale fields have at most 31 bits, colour transform fields 15.
#[test]
fn extreme_instance_values_are_clamped_not_dropped() {
    let mut big = inst("mc", SymbolKind::MovieClip);
    big.name = Some("big_mc".into());
    big.matrix = Matrix {
        a: 1e7,
        b: -1e7,
        c: f64::INFINITY,
        d: f64::NAN,
        tx: 1e12,
        ty: -1e12,
    };
    let mut bright = inst("mc", SymbolKind::MovieClip);
    bright.name = Some("bright_mc".into());
    bright.color = ColorEffect {
        mult: [100.0, -100.0, 1.0, 1e9],
        add: [1e6, -1e6, 0.0, 0.0],
    };
    let mut d = doc(vec![
        layer("a", vec![kf(0, 1, vec![Element::Instance(big)])]),
        layer("b", vec![kf(0, 1, vec![Element::Instance(bright)])]),
    ]);
    add_symbol(
        &mut d,
        "mc",
        SymbolKind::MovieClip,
        vec![layer("l", vec![kf(0, 1, vec![square(10.0)])])],
    );
    let out = publish_bounded(d, 8 << 20, Duration::from_secs(60));
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    parsed_tags(&out, |tags| {
        let names: Vec<String> = tags
            .iter()
            .filter_map(|t| match t {
                Tag::PlaceObject(p) => p.name.map(|n| n.to_str_lossy(swf::UTF_8).into_owned()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["bright_mc", "big_mc"]);
    });
}

/// A movie clip placed again after a blank keyframe is a new instance.
/// Flash 8 marks such placements with `ratio` = the frame number (the
/// `scenes` fixture: the dot clip re-placed at root frame 4 has ratio 4),
/// and players compare ratios to decide whether an instance survives a
/// backwards goto. Without it a rewinding goto keeps the later instance.
#[test]
fn movie_clips_placed_after_frame_one_carry_their_frame_as_ratio() {
    let mut d = doc(vec![layer(
        "l",
        vec![
            kf(
                0,
                2,
                vec![Element::Instance(inst("mc", SymbolKind::MovieClip))],
            ),
            kf(2, 1, Vec::new()),
            kf(
                3,
                2,
                vec![
                    Element::Instance(inst("mc", SymbolKind::MovieClip)),
                    square(4.0),
                ],
            ),
        ],
    )]);
    add_symbol(
        &mut d,
        "mc",
        SymbolKind::MovieClip,
        vec![layer("l", vec![kf(0, 1, vec![square(10.0)])])],
    );
    let out = publish_bounded(d, 8 << 20, Duration::from_secs(60));
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    parsed_tags(&out, |tags| {
        let mut frame = 0u32;
        let mut seen = Vec::new();
        for t in tags {
            match t {
                Tag::ShowFrame => frame += 1,
                Tag::PlaceObject(p) => {
                    if let PlaceObjectAction::Place(_) = p.action {
                        seen.push((frame, p.depth, p.ratio));
                    }
                }
                _ => {}
            }
        }
        // The clip at frame 0 (no ratio) and frame 3 (ratio 3, above the
        // raw square, which Flash draws underneath); shapes never carry one.
        assert_eq!(seen, [(0, 1, None), (3, 1, None), (3, 2, Some(3))]);
    });
}

/// A mask layer inside a button: DefineButton2 records cannot clip, so the
/// mask must not be drawn as ordinary content over the button.
#[test]
fn masks_inside_buttons_are_not_drawn_as_content() {
    let mut d = doc(vec![layer(
        "l",
        vec![kf(
            0,
            1,
            vec![Element::Instance(inst("btn", SymbolKind::Button))],
        )],
    )]);
    let mask = Layer {
        name: "mask".into(),
        kind: LayerKind::Mask,
        frames: vec![kf(0, 1, vec![square(100.0)])],
        ..Layer::default()
    };
    let masked = Layer {
        name: "masked".into(),
        parent: Some(0),
        frames: vec![kf(0, 1, vec![square(30.0)])],
        ..Layer::default()
    };
    add_symbol(&mut d, "btn", SymbolKind::Button, vec![mask, masked]);
    let out = publish_bounded(d, 8 << 20, Duration::from_secs(60));
    parsed_tags(&out, |tags| {
        let records = tags
            .iter()
            .find_map(|t| match t {
                Tag::DefineButton2(b) => Some(b.records.len()),
                _ => None,
            })
            .expect("button");
        assert_eq!(records, 1, "only the masked square is drawn");
    });
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("mask") && d.message.contains("button")),
        "{:?}",
        out.diagnostics
    );
}

/// More filters than the u8 count of PlaceObject3 holds: the placement is
/// kept with the first 255 instead of failing its round trip.
#[test]
fn too_many_filters_are_capped() {
    use jazari_xfl::model::{BlurFilter, Filter, FilterKind};
    let mut many = inst("mc", SymbolKind::MovieClip);
    many.name = Some("many_mc".into());
    many.filters = (0..300)
        .map(|_| Filter {
            enabled: true,
            kind: FilterKind::Blur(BlurFilter {
                blur_x: 4.0,
                blur_y: 4.0,
                quality: 1,
            }),
        })
        .collect();
    let mut d = doc(vec![layer(
        "a",
        vec![kf(0, 1, vec![Element::Instance(many)])],
    )]);
    add_symbol(
        &mut d,
        "mc",
        SymbolKind::MovieClip,
        vec![layer("l", vec![kf(0, 1, vec![square(10.0)])])],
    );
    let out = publish_bounded(d, 8 << 20, Duration::from_secs(60));
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    parsed_tags(&out, |tags| {
        let n = tags.iter().find_map(|t| match t {
            Tag::PlaceObject(p) if p.name.is_some() => p.filters.as_ref().map(Vec::len),
            _ => None,
        });
        assert_eq!(n, Some(255));
    });
}

/// Text with absurd attributes and a very long line: every value is
/// clamped or reported, nothing panics, and the movie still reads back.
#[test]
fn absurd_text_is_reported_not_fatal() {
    use jazari_xfl::model::{TextAttrs, TextElement, TextKind, TextRun};
    let long: String = "abc ".repeat(20_000);
    let attrs = |size: f64, spacing: f64| TextAttrs {
        face: "Arial".into(),
        size,
        letter_spacing: spacing,
        line_spacing: f64::NAN,
        indent: f64::INFINITY,
        left_margin: -1e12,
        right_margin: f64::NAN,
        ..TextAttrs::default()
    };
    let text = |kind: TextKind, size: f64, spacing: f64, s: &str| {
        Element::Text(TextElement {
            kind,
            matrix: Matrix {
                a: f64::NAN,
                tx: 1e15,
                ..Matrix::IDENTITY
            },
            width: f64::INFINITY,
            height: -5.0,
            runs: vec![TextRun {
                text: s.to_owned(),
                attrs: attrs(size, spacing),
            }],
            name: Some("t".into()),
            max_chars: Some(u32::MAX),
            ..TextElement::default()
        })
    };
    let d = doc(vec![layer(
        "l",
        vec![kf(
            0,
            1,
            vec![
                text(TextKind::Static, 12.0, 0.0, &long),
                text(TextKind::Static, 1e9, 1e9, "big"),
                text(TextKind::Static, f64::NAN, f64::NAN, "nan\r\r\r"),
                text(TextKind::Dynamic, 1e9, -1e9, &long),
                text(TextKind::Input, f64::NAN, 0.0, "x"),
            ],
        )],
    )]);
    let out = publish_bounded(d, 8 << 20, Duration::from_secs(120));
    for d in &out.diagnostics {
        println!("{d}");
    }
    assert!(out.swf.is_some(), "{:?}", out.diagnostics);
}

/// 9-slice scaling is not published; that must be said, not silent.
#[test]
fn nine_slice_scaling_is_reported() {
    let mut d = doc(vec![layer(
        "l",
        vec![kf(
            0,
            1,
            vec![Element::Instance(inst("mc", SymbolKind::MovieClip))],
        )],
    )]);
    add_symbol(
        &mut d,
        "mc",
        SymbolKind::MovieClip,
        vec![layer("l", vec![kf(0, 1, vec![square(10.0)])])],
    );
    if let Some(sym) = d.library.symbols.get_mut("mc") {
        sym.scale9 = Some(jazari_xfl::model::Rect::default());
    }
    let out = publish_bounded(d, 8 << 20, Duration::from_secs(60));
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("9-slice")),
        "{:?}",
        out.diagnostics
    );
}
