//! A backwards goto in Ruffle must treat a movie clip placed again after a
//! blank keyframe as a different instance, as Flash Player does. The
//! publisher marks such placements with `ratio` = frame (as Flash 8 does);
//! without it the later instance survives the rewind with its variables.

use jazari_publish::{BuildMode, PublishOptions, publish};
use jazari_xfl::Document;
use jazari_xfl::model::{
    Edge, Element, FillStyle, Frame, Layer, PathCmd, Pt, Rgba, Shape, Symbol, SymbolInstance,
    SymbolKind, Timeline,
};

fn square() -> Element {
    Element::Shape(Shape {
        fills: vec![FillStyle::Solid(Rgba::opaque(0, 0, 200))],
        edges: vec![Edge {
            fill0: 0,
            fill1: 1,
            stroke: 0,
            path: vec![
                PathCmd::MoveTo(Pt::new(0.0, 0.0)),
                PathCmd::LineTo(Pt::new(400.0, 0.0)),
                PathCmd::LineTo(Pt::new(400.0, 400.0)),
                PathCmd::LineTo(Pt::new(0.0, 0.0)),
            ],
        }],
        ..Shape::default()
    })
}

fn kf(index: u32, duration: u32, elements: Vec<Element>, script: Option<&str>) -> Frame {
    Frame {
        index,
        duration,
        elements,
        script: script.map(str::to_owned),
        ..Frame::default()
    }
}

fn clip() -> Element {
    Element::Instance(SymbolInstance {
        library_item: "mc".into(),
        kind: SymbolKind::MovieClip,
        name: Some("a_mc".into()),
        ..SymbolInstance::default()
    })
}

#[test]
fn a_clip_placed_again_does_not_survive_a_rewind() {
    let mut doc = Document {
        scenes: vec![Timeline {
            name: "Scene 1".into(),
            layers: vec![
                Layer {
                    name: "as".into(),
                    frames: vec![
                        kf(0, 3, Vec::new(), None),
                        kf(
                            3,
                            1,
                            Vec::new(),
                            Some("a_mc.mark = \"second\";\ntrace(\"set \" + a_mc.mark);"),
                        ),
                        kf(
                            4,
                            2,
                            Vec::new(),
                            Some("gotoAndStop(2);\ntrace(\"after \" + a_mc.mark);"),
                        ),
                    ],
                    ..Layer::default()
                },
                Layer {
                    name: "clips".into(),
                    frames: vec![
                        kf(0, 2, vec![clip()], None),
                        kf(2, 1, Vec::new(), None),
                        kf(3, 3, vec![clip()], None),
                    ],
                    ..Layer::default()
                },
            ],
        }],
        ..Document::default()
    };
    doc.library.symbols.insert(
        "mc".into(),
        Symbol {
            name: "mc".into(),
            kind: SymbolKind::MovieClip,
            timeline: Timeline {
                name: "mc".into(),
                layers: vec![Layer {
                    name: "l".into(),
                    frames: vec![kf(0, 1, vec![square()], None)],
                    ..Layer::default()
                }],
            },
            ..Symbol::default()
        },
    );
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
    let swf = out.swf.expect("published");
    let log = jazari_preview::run_headless(&swf, 8).expect("runs");
    let traces: Vec<String> = log
        .iter()
        .filter(|l| l.kind == jazari_preview::LogKind::Trace)
        .map(|l| l.text.clone())
        .collect();
    assert_eq!(traces, ["set second", "after undefined"]);
}
