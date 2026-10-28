//! Timeline behaviour on small synthetic documents, published end to end
//! and read back.

use crate::{BuildMode, PublishOptions, PublishOutput, Severity, publish};
use jazari_xfl::Document;
use jazari_xfl::model::{
    Edge, Element, FillStyle, Frame, FrameLabel, GraphicPlayback, LabelKind, Layer, LayerKind,
    Linkage, LoopMode, Matrix, MotionTween, PathCmd, Pt, Rgba, Shape, Symbol, SymbolInstance,
    SymbolKind, Timeline, Tween,
};
use swf::{PlaceObjectAction, Tag};

fn square(size_twips: f64, r: u8) -> Element {
    let s = size_twips;
    Element::Shape(Shape {
        fills: vec![FillStyle::Solid(Rgba::opaque(r, 0, 0))],
        edges: vec![Edge {
            fill0: 0,
            fill1: 1,
            stroke: 0,
            path: vec![
                PathCmd::MoveTo(Pt::new(0.0, 0.0)),
                PathCmd::LineTo(Pt::new(s, 0.0)),
                PathCmd::LineTo(Pt::new(s, s)),
                PathCmd::LineTo(Pt::new(0.0, s)),
                PathCmd::LineTo(Pt::new(0.0, 0.0)),
            ],
        }],
        ..Shape::default()
    })
}

fn moved(el: Element, tx: f64) -> Element {
    match el {
        Element::Shape(mut s) => {
            s.matrix = Matrix::translate(tx, 0.0);
            Element::Shape(s)
        }
        other => other,
    }
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

/// Layers are given top first, as in XFL.
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

fn run(doc: &Document, mode: BuildMode) -> PublishOutput {
    let fonts = jazari_text::FontLibrary::embedded_only();
    let as2 = jazari_as2::Compiler::new(Default::default());
    publish(
        doc,
        &PublishOptions {
            mode,
            compress: false,
        },
        &fonts,
        &as2,
    )
}

fn errors(out: &PublishOutput) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(ToString::to_string)
        .collect()
}

/// Root tags per frame, as short strings.
fn frames(out: &PublishOutput) -> Vec<Vec<String>> {
    let bytes = out.swf.as_ref().expect("swf");
    let buf = swf::decompress_swf(&bytes[..]).expect("decompress");
    let parsed = swf::parse_swf(&buf).expect("parse");
    let mut all = vec![Vec::new()];
    for t in &parsed.tags {
        let s = match t {
            Tag::ShowFrame => {
                all.push(Vec::new());
                continue;
            }
            Tag::PlaceObject(p) => match p.action {
                PlaceObjectAction::Place(id) => format!("place {} #{id}", p.depth),
                PlaceObjectAction::Replace(id) => format!("replace {} #{id}", p.depth),
                PlaceObjectAction::Modify => format!("modify {}", p.depth),
            },
            Tag::RemoveObject(r) => format!("remove {}", r.depth),
            Tag::DefineShape(s) => format!("shape #{}", s.id),
            Tag::DefineSprite(s) => format!("sprite #{}", s.id),
            Tag::DefineButton2(b) => format!("button #{} {}", b.id, b.actions.len()),
            Tag::DoAction(_) => "action".into(),
            Tag::FrameLabel(l) => format!("label {}", l.label.to_str_lossy(swf::UTF_8)),
            Tag::ExportAssets(e) => format!("export {}", e.len()),
            _ => continue,
        };
        if let Some(last) = all.last_mut() {
            last.push(s);
        }
    }
    all.pop();
    all
}

fn places(frame: &[String]) -> Vec<&str> {
    frame
        .iter()
        .filter(|s| {
            s.starts_with("place")
                || s.starts_with("modify")
                || s.starts_with("replace")
                || s.starts_with("remove")
        })
        .map(String::as_str)
        .collect()
}

#[test]
fn depth_bands_put_the_bottom_layer_lowest() {
    let mut d = doc(vec![
        layer(
            "top",
            vec![kf(
                0,
                1,
                vec![Element::Instance(inst("mc", SymbolKind::MovieClip))],
            )],
        ),
        layer(
            "mid",
            vec![kf(
                0,
                1,
                vec![square(100.0, 1), moved(square(100.0, 2), 50.0)],
            )],
        ),
        layer("bottom", vec![kf(0, 1, vec![square(200.0, 3)])]),
    ]);
    add_symbol(
        &mut d,
        "mc",
        SymbolKind::MovieClip,
        vec![layer("a", vec![kf(0, 1, vec![square(10.0, 4)])])],
    );
    let out = run(&d, BuildMode::Release);
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    let f = frames(&out);
    let p = places(&f[0]);
    // bottom = 1, mid = two runs (different matrices) at 2 and 3, top = 4.
    assert_eq!(p.len(), 4, "{p:?}");
    assert!(p[0].starts_with("place 1 "));
    assert!(p[1].starts_with("place 2 ") && p[2].starts_with("place 3 "));
    assert!(p[3].starts_with("place 4 "));
}

#[test]
fn a_band_is_as_wide_as_the_widest_keyframe() {
    let d = doc(vec![
        layer("top", vec![kf(0, 2, vec![square(10.0, 9)])]),
        layer(
            "bottom",
            vec![
                kf(0, 1, vec![square(10.0, 1)]),
                kf(
                    1,
                    1,
                    vec![
                        square(10.0, 1),
                        moved(square(10.0, 2), 5.0),
                        moved(square(10.0, 3), 9.0),
                    ],
                ),
            ],
        ),
    ]);
    let out = run(&d, BuildMode::Release);
    let f = frames(&out);
    // The top layer sits above the three depths of the bottom band.
    assert!(
        places(&f[0]).contains(&"place 4 #2")
            || places(&f[0]).iter().any(|p| p.starts_with("place 4 "))
    );
    assert!(
        !places(&f[1]).iter().any(|p| p.contains(" 4")),
        "top unchanged: {:?}",
        f[1]
    );
}

#[test]
fn a_flattened_graphic_takes_a_sub_band() {
    let mut d = doc(vec![
        layer("top", vec![kf(0, 1, vec![square(10.0, 9)])]),
        layer(
            "g",
            vec![kf(
                0,
                1,
                vec![Element::Instance(inst("G", SymbolKind::Graphic))],
            )],
        ),
    ]);
    add_symbol(
        &mut d,
        "G",
        SymbolKind::Graphic,
        vec![
            layer("g-top", vec![kf(0, 1, vec![square(30.0, 2)])]),
            layer("g-bottom", vec![kf(0, 1, vec![square(40.0, 3)])]),
        ],
    );
    let out = run(&d, BuildMode::Release);
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    let f = frames(&out);
    let p = places(&f[0]);
    assert_eq!(p.len(), 3, "{p:?}");
    assert!(
        p[0].starts_with("place 1 ")
            && p[1].starts_with("place 2 ")
            && p[2].starts_with("place 3 ")
    );
    assert!(
        !f[0].iter().any(|s| s.starts_with("sprite")),
        "graphics are not sprites"
    );
}

#[test]
fn graphic_frames_follow_loop_mode() {
    // A 3-frame graphic: frame n shows a square of size 10(n+1).
    let mut d = doc(Vec::new());
    add_symbol(
        &mut d,
        "G",
        SymbolKind::Graphic,
        vec![layer(
            "a",
            vec![
                kf(0, 1, vec![square(10.0, 1)]),
                kf(1, 1, vec![square(20.0, 1)]),
                kf(2, 1, vec![square(30.0, 1)]),
            ],
        )],
    );
    let mut i = inst("G", SymbolKind::Graphic);
    i.graphic = Some(GraphicPlayback {
        mode: LoopMode::Loop,
        first_frame: 1,
    });
    d.scenes[0]
        .layers
        .push(layer("g", vec![kf(0, 5, vec![Element::Instance(i)])]));
    let out = run(&d, BuildMode::Release);
    let bytes = out.swf.expect("swf");
    let buf = swf::decompress_swf(&bytes[..]).expect("decompress");
    let parsed = swf::parse_swf(&buf).expect("parse");
    let mut size_of = std::collections::HashMap::new();
    let mut shown = Vec::new();
    let mut cur = 0;
    for t in &parsed.tags {
        match t {
            Tag::DefineShape(s) => {
                size_of.insert(s.id, s.shape_bounds.x_max.get());
            }
            Tag::PlaceObject(p) => {
                if let PlaceObjectAction::Place(id) | PlaceObjectAction::Replace(id) = p.action {
                    cur = id;
                }
            }
            Tag::ShowFrame => shown.push(size_of[&cur]),
            _ => {}
        }
    }
    assert_eq!(shown, vec![20, 30, 10, 20, 30]);
}

#[test]
fn the_same_clip_persists_across_keyframes_and_another_replaces_it() {
    let mut d = doc(vec![layer(
        "a",
        vec![
            kf(
                0,
                1,
                vec![Element::Instance(inst("A", SymbolKind::MovieClip))],
            ),
            kf(1, 1, {
                let mut i = inst("A", SymbolKind::MovieClip);
                i.matrix = Matrix::translate(10.0, 0.0);
                vec![Element::Instance(i)]
            }),
            kf(
                2,
                1,
                vec![Element::Instance(inst("B", SymbolKind::MovieClip))],
            ),
        ],
    )]);
    add_symbol(
        &mut d,
        "A",
        SymbolKind::MovieClip,
        vec![layer("x", vec![kf(0, 1, vec![square(10.0, 1)])])],
    );
    add_symbol(
        &mut d,
        "B",
        SymbolKind::MovieClip,
        vec![layer("x", vec![kf(0, 1, vec![square(20.0, 1)])])],
    );
    let out = run(&d, BuildMode::Release);
    let f = frames(&out);
    assert_eq!(places(&f[1]), vec!["modify 1"]);
    let p = places(&f[2]);
    assert_eq!(p[0], "remove 1");
    assert!(p[1].starts_with("place 1 "));
}

#[test]
fn motion_tweens_modify_every_in_between_frame() {
    let mut d = doc(vec![layer(
        "a",
        vec![
            Frame {
                tween: Tween::Motion(MotionTween::default()),
                ..kf(
                    0,
                    4,
                    vec![Element::Instance(inst("A", SymbolKind::MovieClip))],
                )
            },
            kf(4, 1, {
                let mut i = inst("A", SymbolKind::MovieClip);
                i.matrix = Matrix::translate(40.0, 0.0);
                vec![Element::Instance(i)]
            }),
        ],
    )]);
    add_symbol(
        &mut d,
        "A",
        SymbolKind::MovieClip,
        vec![layer("x", vec![kf(0, 1, vec![square(10.0, 1)])])],
    );
    let out = run(&d, BuildMode::Release);
    let f = frames(&out);
    for frame in &f[1..] {
        assert_eq!(places(frame), vec!["modify 1"], "{frame:?}");
    }
}

#[test]
fn recursive_symbols_are_reported_not_followed() {
    let mut d = doc(vec![layer(
        "a",
        vec![kf(
            0,
            1,
            vec![
                Element::Instance(inst("A", SymbolKind::MovieClip)),
                Element::Instance(inst("G", SymbolKind::Graphic)),
            ],
        )],
    )]);
    add_symbol(
        &mut d,
        "A",
        SymbolKind::MovieClip,
        vec![layer(
            "x",
            vec![kf(
                0,
                1,
                vec![Element::Instance(inst("A", SymbolKind::MovieClip))],
            )],
        )],
    );
    add_symbol(
        &mut d,
        "G",
        SymbolKind::Graphic,
        vec![layer(
            "x",
            vec![kf(
                0,
                1,
                vec![
                    square(5.0, 1),
                    Element::Instance(inst("G", SymbolKind::Graphic)),
                ],
            )],
        )],
    );
    let out = run(&d, BuildMode::Release);
    assert!(out.swf.is_some());
    let e = errors(&out);
    assert!(e.iter().any(|m| m.contains("'A' contains itself")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("'G' contains itself")), "{e:?}");
}

#[test]
fn depth_overflow_is_reported_and_bounded() {
    let mut d = doc(vec![layer(
        "a",
        vec![kf(
            0,
            1,
            (0..16_500)
                .map(|_| Element::Instance(inst("A", SymbolKind::MovieClip)))
                .collect(),
        )],
    )]);
    add_symbol(
        &mut d,
        "A",
        SymbolKind::MovieClip,
        vec![layer("x", vec![kf(0, 1, vec![square(10.0, 1)])])],
    );
    let out = run(&d, BuildMode::Release);
    let e = errors(&out);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("16383"));
    let f = frames(&out);
    assert_eq!(places(&f[0]).len(), 16_383);
}

#[test]
fn guide_and_hidden_layers() {
    let mut d = doc(vec![
        Layer {
            kind: LayerKind::Guide,
            ..layer("guide", vec![kf(0, 1, vec![square(10.0, 1)])])
        },
        Layer {
            visible: false,
            ..layer("hidden", vec![kf(0, 1, vec![square(20.0, 1)])])
        },
    ]);
    let out = run(&d, BuildMode::Release);
    assert_eq!(
        places(&frames(&out)[0]).len(),
        1,
        "hidden layers publish by default"
    );
    d.publish.publish_hidden_layers = false;
    let out = run(&d, BuildMode::Release);
    assert!(places(&frames(&out)[0]).is_empty());
}

#[test]
fn scenes_are_concatenated_and_mapped() {
    let mut d = doc(vec![layer("a", vec![kf(0, 2, vec![square(10.0, 1)])])]);
    d.scenes.push(Timeline {
        name: "Two".into(),
        layers: vec![layer(
            "b",
            vec![Frame {
                label: Some(FrameLabel {
                    name: "start".into(),
                    kind: LabelKind::Name,
                }),
                ..kf(0, 3, vec![square(10.0, 1)])
            }],
        )],
    });
    let out = run(&d, BuildMode::Release);
    assert_eq!(out.frame_map, vec![(0, 0), (0, 1), (1, 0), (1, 1), (1, 2)]);
    let f = frames(&out);
    assert_eq!(f.len(), 5);
    // Nothing survives a scene change, even an identical shape.
    let p = places(&f[2]);
    assert_eq!(p[0], "remove 1");
    assert!(f[2].contains(&"label start".to_owned()));
}

#[test]
fn absurd_durations_stop_at_the_frame_limit() {
    let d = doc(vec![layer(
        "a",
        vec![kf(0, u32::MAX, vec![square(10.0, 1)])],
    )]);
    let out = run(&d, BuildMode::Release);
    assert_eq!(out.frame_map.len(), 65535);
    assert_eq!(errors(&out).len(), 1, "{:?}", errors(&out));
}

#[test]
fn missing_symbols_and_empty_documents() {
    let d = doc(vec![layer(
        "a",
        vec![kf(
            0,
            1,
            vec![Element::Instance(inst("nope", SymbolKind::MovieClip))],
        )],
    )]);
    let out = run(&d, BuildMode::Release);
    assert!(out.swf.is_some());
    assert_eq!(errors(&out).len(), 1);
    let out = run(&Document::default(), BuildMode::Release);
    assert!(out.swf.is_some());
    assert!(out.frame_map.is_empty());
    assert_eq!(frames(&out).len(), 1);
}

#[test]
fn buttons_share_definitions_unless_handlers_differ() {
    let mut d = doc(Vec::new());
    // Only an up frame: as in Flash 8, it serves every state.
    add_symbol(
        &mut d,
        "Btn",
        SymbolKind::Button,
        vec![layer("x", vec![kf(0, 1, vec![square(10.0, 1)])])],
    );
    let plain = inst("Btn", SymbolKind::Button);
    let mut scripted = inst("Btn", SymbolKind::Button);
    scripted.script = Some("on (release) { trace(1); }".into());
    let scripted2 = scripted.clone();
    d.scenes[0].layers.push(layer(
        "b",
        vec![kf(
            0,
            1,
            vec![
                Element::Instance(plain.clone()),
                Element::Instance(plain),
                Element::Instance(scripted),
                Element::Instance(scripted2),
            ],
        )],
    ));
    let out = run(&d, BuildMode::Release);
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    let f = frames(&out);
    let buttons: Vec<_> = f[0].iter().filter(|s| s.starts_with("button")).collect();
    assert_eq!(buttons.len(), 2, "{buttons:?}");
    assert!(buttons[0].ends_with(" 0") && buttons[1].ends_with(" 1"));
    let bytes = out.swf.expect("swf");
    let buf = swf::decompress_swf(&bytes[..]).expect("decompress");
    let parsed = swf::parse_swf(&buf).expect("parse");
    for t in &parsed.tags {
        if let Tag::DefineButton2(b) = t {
            assert_eq!(b.records.len(), 1);
            assert_eq!(
                b.records[0].states.bits(),
                0b1111,
                "the only frame serves every state (Flash 8)"
            );
        }
    }
}

#[test]
fn linked_symbols_are_exported_without_instances() {
    let mut d = doc(vec![layer("a", vec![kf(0, 1, vec![square(10.0, 1)])])]);
    add_symbol(
        &mut d,
        "Lib",
        SymbolKind::MovieClip,
        vec![layer("x", vec![kf(0, 1, vec![square(10.0, 1)])])],
    );
    if let Some(s) = d.library.symbols.get_mut("Lib") {
        s.linkage = Some(Linkage {
            identifier: "lib_id".into(),
            export_for_as: true,
            ..Linkage::default()
        });
    }
    let out = run(&d, BuildMode::Release);
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    let f = frames(&out);
    assert!(f[0].contains(&"export 1".to_owned()), "{:?}", f[0]);
}

#[test]
fn the_stage_build_strips_scripts() {
    let mut d = doc(vec![layer(
        "a",
        vec![Frame {
            script: Some("trace('x');".into()),
            ..kf(0, 1, {
                let mut i = inst("A", SymbolKind::MovieClip);
                i.script = Some("onClipEvent (load) { trace(1); }".into());
                vec![Element::Instance(i)]
            })
        }],
    )]);
    add_symbol(
        &mut d,
        "A",
        SymbolKind::MovieClip,
        vec![layer(
            "x",
            vec![Frame {
                script: Some("trace('inner');".into()),
                ..kf(0, 2, vec![square(10.0, 1)])
            }],
        )],
    );
    let release = run(&d, BuildMode::Release);
    let stage = run(&d, BuildMode::Stage);
    assert!(errors(&release).is_empty() && errors(&stage).is_empty());
    let hook = jazari_as2::stage_goto_hook().expect("hook");
    let bytes = stage.swf.expect("swf");
    let buf = swf::decompress_swf(&bytes[..]).expect("decompress");
    let parsed = swf::parse_swf(&buf).expect("parse");
    let mut root_actions = Vec::new();
    for t in &parsed.tags {
        match t {
            Tag::DoAction(a) => root_actions.push(a.to_vec()),
            Tag::PlaceObject(p) => assert!(p.clip_actions.is_none()),
            Tag::DefineSprite(s) => {
                let acts: Vec<_> = s
                    .tags
                    .iter()
                    .filter(|t| matches!(t, Tag::DoAction(_)))
                    .collect();
                assert_eq!(acts, vec![&Tag::DoAction(&[0x07, 0x00])]);
            }
            _ => {}
        }
    }
    assert_eq!(root_actions, vec![hook]);
}
