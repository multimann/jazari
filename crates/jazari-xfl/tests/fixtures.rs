//! Reads the Flash 8 oracle fixtures (in `tests/fixtures/` at the repository
//! root) and checks facts taken from their XML by hand.
//!
//! The fixtures were made by the Flash 8 IDE and converted to XFL by JPEXS;
//! see `tests/fixtures/MANIFEST.md` for how each was built.

use std::path::{Path, PathBuf};

use jazari_xfl::model::{
    Align, BitmapData, BlendMode, Caps, ContainerKind, Document, Element, FillStyle, FilterKind,
    GraphicPlayback, Interp, Joints, LabelKind, Layer, LayerKind, LoopMode, Matrix, PathCmd, Rgba,
    RotateMode, Shape, Spread, SymbolInstance, SymbolKind, TextElement, TextKind, Timeline, Tween,
};

/// A path under the shared fixtures in `tests/fixtures/` at the repository
/// root: `name` alone is that fixture's XFL folder, `name/rest` a file inside
/// it, and `_src/file` one of the generated source images.
fn data(path: &str) -> PathBuf {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    match path.split_once('/') {
        Some(("_src", file)) => fixtures.join("_src").join(file),
        Some((name, rest)) => fixtures.join(name).join("xfl").join(rest),
        None => fixtures.join(path).join("xfl"),
    }
}

/// Opens a fixture and requires an empty import report.
fn open_clean(name: &str) -> Document {
    let (doc, report) =
        jazari_xfl::open(&data(name)).unwrap_or_else(|e| panic!("{name}: cannot open: {e}"));
    let warnings: Vec<String> = report.iter().map(ToString::to_string).collect();
    assert!(
        warnings.is_empty(),
        "{name}: unexpected warnings:\n{}",
        warnings.join("\n")
    );
    doc
}

fn layer<'a>(t: &'a Timeline, name: &str) -> &'a Layer {
    t.layers
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no layer '{name}' in '{}'", t.name))
}

fn layer_names(t: &Timeline) -> Vec<&str> {
    t.layers.iter().map(|l| l.name.as_str()).collect()
}

/// The elements of the keyframe span containing `frame`.
fn elements_at(l: &Layer, frame: u32) -> &[Element] {
    &l.frame_at(frame)
        .unwrap_or_else(|| panic!("layer '{}' has nothing at frame {frame}", l.name))
        .elements
}

fn shapes(elements: &[Element]) -> Vec<&Shape> {
    elements
        .iter()
        .filter_map(|e| match e {
            Element::Shape(s) => Some(s),
            _ => None,
        })
        .collect()
}

fn instances(elements: &[Element]) -> Vec<&SymbolInstance> {
    elements
        .iter()
        .filter_map(|e| match e {
            Element::Instance(i) => Some(i),
            _ => None,
        })
        .collect()
}

fn texts(elements: &[Element]) -> Vec<&TextElement> {
    elements
        .iter()
        .filter_map(|e| match e {
            Element::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn instance<'a>(elements: &'a [Element], name: &str) -> &'a SymbolInstance {
    instances(elements)
        .into_iter()
        .find(|i| i.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no instance named '{name}'"))
}

/// Bounds of every point of every edge (twips).
fn edge_bounds(shape: &Shape) -> (f64, f64, f64, f64) {
    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut add = |x: f64, y: f64| b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    for e in &shape.edges {
        for cmd in &e.path {
            match *cmd {
                PathCmd::MoveTo(p) | PathCmd::LineTo(p) => add(p.x, p.y),
                PathCmd::QuadTo(c, p) => {
                    add(c.x, c.y);
                    add(p.x, p.y);
                }
            }
        }
    }
    b
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn shapes_basic() {
    let doc = open_clean("shapes-basic");
    assert_eq!(
        (doc.width, doc.height, doc.frame_rate),
        (550.0, 400.0, 12.0)
    );
    assert_eq!(doc.background, Rgba::WHITE);
    assert_eq!(doc.xfl_version, "2.0");
    assert_eq!(doc.source.container, ContainerKind::Folder);
    assert_eq!(doc.scenes.len(), 1);
    let scene = &doc.scenes[0];
    assert_eq!(scene.name, "Scene 1");
    assert_eq!(scene.frame_count(), 1);
    assert_eq!(
        layer_names(scene),
        [
            "actions", "lines", "joins", "solid", "holes", "front", "mid", "back"
        ]
    );
    assert!(scene.layers.iter().all(|l| l.kind == LayerKind::Normal));
    assert_eq!(
        layer(scene, "actions").frames[0].script.as_deref(),
        Some("trace(\"shapes-basic: frame 1\");")
    );
    assert!(doc.library.symbols.is_empty());

    // 12 lines: caps none/round/square by row, widths 1/4/10/20 by column,
    // drawn between (20,30) and (275,110) px.
    let lines = shapes(elements_at(layer(scene, "lines"), 0));
    assert_eq!(lines.len(), 1);
    let s = lines[0];
    assert_eq!(s.strokes.len(), 12);
    assert!(s.fills.is_empty());
    let weights: Vec<f64> = s.strokes.iter().map(|st| st.weight).collect();
    assert_eq!(&weights[..4], &[1.0, 4.0, 10.0, 20.0]);
    assert_eq!(s.strokes[0].caps, Caps::None);
    assert_eq!(s.strokes[4].caps, Caps::Round);
    assert_eq!(s.strokes[8].caps, Caps::Square);
    assert_eq!(edge_bounds(s), (400.0, 600.0, 5500.0, 2200.0));
    assert!(
        s.edges
            .iter()
            .all(|e| e.fill0 == 0 && e.fill1 == 0 && e.stroke >= 1)
    );

    // Joins: miter (limit 3 and 1.5), round and bevel.
    let joins = shapes(elements_at(layer(scene, "joins"), 0));
    let strokes: Vec<_> = joins.iter().flat_map(|s| s.strokes.iter()).collect();
    for j in [Joints::Miter, Joints::Round, Joints::Bevel] {
        assert!(strokes.iter().any(|st| st.joints == j), "{j:?}");
    }
    assert!(
        strokes
            .iter()
            .any(|st| st.joints == Joints::Miter && st.miter_limit == 1.5)
    );
    assert!(
        strokes
            .iter()
            .any(|st| st.joints == Joints::Miter && st.miter_limit == 3.0)
    );
    assert!(strokes.iter().all(|st| st.kind == "SolidStroke"));
    let b = joins
        .iter()
        .map(|s| edge_bounds(s))
        .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |a, b| {
            (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
        });
    assert_eq!(
        b,
        (400.0, 3000.0, 5300.0, 6000.0),
        "joins span (20,150)-(265,300) px"
    );

    // Holes: the inner contours carry the fill on the left (fillStyle0).
    let holes = shapes(elements_at(layer(scene, "holes"), 0));
    assert!(
        holes
            .iter()
            .any(|s| s.edges.iter().any(|e| e.fill0 != 0 && e.fill1 == 0))
    );
    assert!(holes.iter().any(|s| s.edges.iter().any(|e| e.fill1 != 0)));

    // Alpha fills: #33AA33 at 50 % (0.50196 → 128) and #00CC00 at 60 % (153).
    let all_fills: Vec<&FillStyle> = scene
        .layers
        .iter()
        .flat_map(|l| l.frames.iter())
        .flat_map(|f| shapes(&f.elements))
        .flat_map(|s| s.fills.iter())
        .collect();
    assert!(all_fills.contains(&&FillStyle::Solid(Rgba::new(0x33, 0xAA, 0x33, 128))));
    assert!(all_fills.contains(&&FillStyle::Solid(Rgba::new(0x00, 0xCC, 0x00, 153))));
    assert!(all_fills.contains(&&FillStyle::Solid(Rgba::opaque(0x00, 0x80, 0x80))));
}

#[test]
fn gradients() {
    let doc = open_clean("gradients");
    assert_eq!(doc.background, Rgba::opaque(0xE0, 0xE0, 0xE0));
    let scene = &doc.scenes[0];
    assert_eq!(
        layer_names(scene),
        ["actions", "stroke-radial", "strokes", "fills", "backdrop"]
    );
    let fills: Vec<&FillStyle> = shapes(elements_at(layer(scene, "fills"), 0))
        .into_iter()
        .flat_map(|s| s.fills.iter())
        .collect();
    let gradients: Vec<(&FillStyle, &jazari_xfl::model::Gradient)> = fills
        .iter()
        .filter_map(|f| match f {
            FillStyle::Linear(g) | FillStyle::Radial(g) => Some((*f, g)),
            _ => None,
        })
        .collect();
    // Linear 3-stop with the box of R1 (centre (95,130)).
    assert!(
        gradients
            .iter()
            .any(|(f, g)| matches!(f, FillStyle::Linear(_))
                && g.stops.len() == 3
                && g.matrix == Matrix::new(0.091552734375, 0.0, 0.0, 0.048828125, 95.0, 130.0))
    );
    // Alpha stops rotated 90°: b and c carry the box.
    assert!(gradients.iter().any(|(_, g)| g.matrix.a == 0.0
        && g.matrix.b == 0.048828125
        && g.stops[0].color == Rgba::new(0, 0, 255, 0)
        && g.stops[2].color == Rgba::new(0, 255, 0, 128)));
    // linearRGB with reflect.
    assert!(
        gradients
            .iter()
            .any(|(_, g)| g.interpolation == Interp::LinearRgb && g.spread == Spread::Reflect)
    );
    // Focal gradients: +180/255 and -120/255 (repeat).
    assert!(
        gradients
            .iter()
            .any(|(_, g)| approx(g.focal_point, 180.0 / 255.0) && g.spread == Spread::Pad)
    );
    assert!(
        gradients
            .iter()
            .any(|(_, g)| approx(g.focal_point, -120.0 / 255.0) && g.spread == Spread::Repeat)
    );
    // The Flash 8 maximum of 15 stops.
    assert!(gradients.iter().any(|(_, g)| g.stops.len() == 15));

    // Gradient strokes: linear R/G/B (14 px) and radial yellow-magenta (10 px).
    let linear_stroke = &shapes(elements_at(layer(scene, "strokes"), 0))[0].strokes[0];
    assert_eq!(linear_stroke.weight, 14.0);
    let FillStyle::Linear(g) = &linear_stroke.fill else {
        panic!("linear stroke fill expected")
    };
    let colours: Vec<Rgba> = g.stops.iter().map(|s| s.color).collect();
    assert_eq!(
        colours,
        [
            Rgba::opaque(255, 0, 0),
            Rgba::opaque(0, 255, 0),
            Rgba::opaque(0, 0, 255)
        ]
    );
    let radial_stroke = &shapes(elements_at(layer(scene, "stroke-radial"), 0))[0].strokes[0];
    assert_eq!(radial_stroke.weight, 10.0);
    let FillStyle::Radial(g) = &radial_stroke.fill else {
        panic!("radial stroke fill expected")
    };
    assert_eq!(g.stops[0].color, Rgba::opaque(255, 255, 0));
    assert_eq!(g.stops[1].color, Rgba::opaque(255, 0, 255));
    assert_eq!(g.matrix.tx, 330.0);
}

#[test]
fn symbols() {
    let doc = open_clean("symbols");
    assert_eq!((doc.width, doc.height), (640.0, 480.0));
    let names: Vec<&str> = doc.library.symbols.keys().map(String::as_str).collect();
    assert_eq!(names, ["Box", "Outer", "Inner"]);
    assert!(
        doc.library
            .symbols
            .values()
            .all(|s| s.kind == SymbolKind::MovieClip)
    );
    let scene = &doc.scenes[0];
    assert_eq!(
        layer_names(scene),
        ["actions", "blend", "filters", "color", "nested", "backdrop"]
    );
    assert!(
        layer(scene, "actions").frames[0]
            .script
            .as_deref()
            .is_some_and(|s| s.starts_with("trace(\"symbols: frame 1\");\ntrace(\"nested: \""))
    );

    // Nesting: Outer holds innerA_mc and innerB_mc (scaled 0.5).
    let outer = &doc.library.symbols["Outer"];
    let inner_layer = layer(&outer.timeline, "inner");
    let a = instance(elements_at(inner_layer, 0), "innerA_mc");
    let b = instance(elements_at(inner_layer, 0), "innerB_mc");
    assert_eq!(
        (a.library_item.as_str(), a.matrix),
        ("Inner", Matrix::translate(-25.0, 0.0))
    );
    assert_eq!(b.matrix, Matrix::new(0.5, 0.0, 0.0, 0.5, 30.0, 0.0));
    let outer_mc = instance(elements_at(layer(scene, "nested"), 0), "outer_mc");
    assert_eq!(outer_mc.matrix, Matrix::translate(90.0, 70.0));
    assert_eq!(outer_mc.kind, SymbolKind::MovieClip);

    // Colour effects, normalised.
    let color = elements_at(layer(scene, "color"), 0);
    assert!(instance(color, "plain_mc").color.is_identity());
    assert_eq!(
        instance(color, "alpha_mc").color.mult,
        [1.0, 1.0, 1.0, 0.3984375]
    );
    let tint = instance(color, "tint_mc").color;
    assert!(tint.mult[..3].iter().all(|m| approx(*m, 0.4)));
    assert!(approx(tint.add[2], 153.0) && tint.add[0] == 0.0 && tint.add[1] == 0.0);
    let bright = instance(color, "bright_mc").color;
    assert_eq!((bright.mult, bright.add), ([0.5, 0.5, 0.5, 1.0], [0.0; 4]));
    let adv = instance(color, "advanced_mc").color;
    assert_eq!(adv.mult, [0.5, 1.0, 0.19921875, 0.80078125]);
    assert_eq!(adv.add, [100.0, -60.0, 0.0, 0.0]);

    // Filters.
    let filters = elements_at(layer(scene, "filters"), 0);
    let drop = &instance(filters, "drop_mc").filters;
    assert_eq!(drop.len(), 1);
    let FilterKind::DropShadow(d) = &drop[0].kind else {
        panic!("drop shadow expected")
    };
    assert_eq!(
        (d.distance, d.blur_x, d.strength, d.quality),
        (8.0, 6.0, 0.8, 3)
    );
    assert_eq!(d.color, Rgba::opaque(0x33, 0x33, 0x33));
    assert!((d.angle - 45.0).abs() < 1e-5);
    let FilterKind::Blur(blur) = &instance(filters, "blur_mc").filters[0].kind else {
        panic!("blur expected")
    };
    assert_eq!((blur.blur_x, blur.blur_y, blur.quality), (10.0, 4.0, 2));
    let FilterKind::Glow(glow) = &instance(filters, "glow_mc").filters[0].kind else {
        panic!("glow expected")
    };
    assert_eq!(
        (glow.color, glow.blur_x, glow.strength, glow.quality),
        (Rgba::opaque(255, 0, 0), 12.0, 2.0, 1)
    );
    let FilterKind::Bevel(bevel) = &instance(filters, "bevel_mc").filters[0].kind else {
        panic!("bevel expected")
    };
    assert_eq!(
        (bevel.highlight_color, bevel.shadow_color),
        (Rgba::WHITE, Rgba::BLACK)
    );
    let multi = &instance(filters, "multi_mc").filters;
    assert!(
        matches!(&multi[0].kind, FilterKind::Glow(g) if g.inner && g.color == Rgba::opaque(255, 255, 0))
    );
    assert!(matches!(&multi[1].kind, FilterKind::DropShadow(d) if d.distance == 6.0));
    assert!(multi.iter().all(|f| f.enabled));

    // Blend modes and cacheAsBitmap.
    let blend = elements_at(layer(scene, "blend"), 0);
    assert_eq!(instance(blend, "normal_mc").blend_mode, BlendMode::Normal);
    assert_eq!(
        instance(blend, "multiply_mc").blend_mode,
        BlendMode::Multiply
    );
    assert_eq!(instance(blend, "screen_mc").blend_mode, BlendMode::Screen);
    assert_eq!(instance(blend, "add_mc").blend_mode, BlendMode::Add);
    let cache = instance(blend, "cache_mc");
    assert!(cache.cache_as_bitmap);
    assert_eq!(cache.matrix, Matrix::translate(560.0, 339.95));
}

#[test]
fn tweens() {
    let doc = open_clean("tweens");
    assert_eq!(doc.frame_rate, 24.0);
    let scene = &doc.scenes[0];
    assert_eq!(scene.frame_count(), 60);
    assert_eq!(
        layer_names(scene),
        [
            "actions",
            "rotate_cw",
            "rotate_auto",
            "scale_ease",
            "alpha",
            "consecutive",
            "graphics"
        ]
    );
    let counter = &doc.library.symbols["Counter"];
    assert_eq!(counter.kind, SymbolKind::Graphic);
    assert_eq!(counter.timeline.frame_count(), 10);
    assert_eq!(counter.timeline.layers[0].frames.len(), 10);
    assert_eq!(doc.library.symbols["Arrow"].kind, SymbolKind::MovieClip);

    let actions = layer(scene, "actions");
    assert_eq!(actions.frames.len(), 2);
    assert!(
        actions.frames[1]
            .script
            .as_deref()
            .is_some_and(|s| s.contains("stop();"))
    );
    assert_eq!(actions.frames[1].index, 59);

    let motion = |l: &Layer, i: usize| match &l.frames[i].tween {
        Tween::Motion(m) => m.clone(),
        other => panic!(
            "layer '{}' keyframe {i}: motion tween expected, got {other:?}",
            l.name
        ),
    };
    let first_matrix = |l: &Layer, frame: u32| instances(elements_at(l, frame))[0].matrix;

    // Clockwise once, then counter-clockwise twice, then hold.
    let cw = layer(scene, "rotate_cw");
    let spans: Vec<(u32, u32)> = cw.frames.iter().map(|f| (f.index, f.duration)).collect();
    assert_eq!(spans, [(0, 23), (23, 24), (47, 13)]);
    let m0 = motion(cw, 0);
    assert_eq!(
        (m0.rotate, m0.rotate_times, m0.ease),
        (RotateMode::Clockwise, 1, 0.0)
    );
    assert!(m0.scale && m0.snap && m0.sync && !m0.orient_to_path);
    assert_eq!(m0.custom_ease, None);
    let m1 = motion(cw, 1);
    assert_eq!(
        (m1.rotate, m1.rotate_times),
        (RotateMode::CounterClockwise, 2)
    );
    assert_eq!(cw.frames[2].tween, Tween::None);
    assert_eq!(first_matrix(cw, 0), Matrix::translate(60.0, 50.0));
    assert_eq!(first_matrix(cw, 23), Matrix::translate(260.0, 50.0));
    assert_eq!(first_matrix(cw, 59), Matrix::translate(60.0, 50.0));

    // Auto rotation to a quarter turn.
    let auto = layer(scene, "rotate_auto");
    assert_eq!(motion(auto, 0).rotate, RotateMode::Auto);
    assert_eq!(
        first_matrix(auto, 23),
        Matrix::new(0.0, 1.0, -1.0, 0.0, 500.0, 50.0)
    );

    // Scale with ease: the XFL `acceleration` is kept as written (JPEXS wrote
    // +100 for the IDE's ease-in -100; see MANIFEST.md).
    let scale = layer(scene, "scale_ease");
    assert_eq!(motion(scale, 0).ease, 100.0);
    assert_eq!(motion(scale, 1).ease, -100.0);
    assert_eq!(first_matrix(scale, 0), Matrix::translate(100.0, 150.0));
    assert_eq!(
        first_matrix(scale, 23),
        Matrix::new(2.0, 0.0, 0.0, 2.0, 100.0, 150.0)
    );
    assert_eq!(
        first_matrix(scale, 47),
        Matrix::new(0.5, 0.0, 0.0, 0.5, 100.0, 150.0)
    );

    // Alpha 100 → 0 → 100.
    let alpha = layer(scene, "alpha");
    let a = |frame| instances(elements_at(alpha, frame))[0].color.mult[3];
    assert_eq!((a(0), a(29), a(59)), (1.0, 0.0, 1.0));

    // Consecutive tweens with eases 0, -50, +50 as written.
    let cons = layer(scene, "consecutive");
    let eases: Vec<f64> = (0..3).map(|i| motion(cons, i).ease).collect();
    assert_eq!(eases, [0.0, -50.0, 50.0]);
    assert_eq!(
        first_matrix(cons, 30),
        Matrix::new(0.0, 1.0, -1.0, 0.0, 200.0, 320.0)
    );
    assert_eq!(
        first_matrix(cons, 45),
        Matrix::new(-1.0, 0.0, 0.0, -1.0, 60.0, 320.0)
    );

    // Graphic playback modes.
    let graphics = instances(elements_at(layer(scene, "graphics"), 0));
    let playback: Vec<Option<GraphicPlayback>> = graphics.iter().map(|g| g.graphic).collect();
    let gp = |mode, first_frame| Some(GraphicPlayback { mode, first_frame });
    assert_eq!(
        playback,
        [
            gp(LoopMode::Loop, 0),
            gp(LoopMode::Loop, 7),
            gp(LoopMode::PlayOnce, 3),
            gp(LoopMode::SingleFrame, 6)
        ]
    );
    assert!(
        graphics
            .iter()
            .all(|g| g.kind == SymbolKind::Graphic && g.name.is_none())
    );
    let tx: Vec<f64> = graphics.iter().map(|g| g.matrix.tx).collect();
    assert_eq!(tx, [106.0, 236.0, 366.0, 496.0]);
    assert!(graphics.iter().all(|g| g.matrix.ty == 374.95));
}

#[test]
fn as2() {
    let doc = open_clean("as2");
    assert_eq!(
        (doc.width, doc.height, doc.frame_rate),
        (400.0, 300.0, 12.0)
    );
    let scene = &doc.scenes[0];
    assert_eq!(scene.frame_count(), 12);
    assert_eq!(
        layer_names(scene),
        ["actions", "labels", "text", "clip", "button"]
    );

    let labels: Vec<(u32, &str, LabelKind)> = layer(scene, "labels")
        .frames
        .iter()
        .filter_map(|f| f.label.as_ref().map(|l| (f.index, l.name.as_str(), l.kind)))
        .collect();
    assert_eq!(
        labels,
        [(4, "loop", LabelKind::Name), (11, "end", LabelKind::Name)]
    );

    let actions = layer(scene, "actions");
    let script_frames: Vec<u32> = actions
        .frames
        .iter()
        .filter(|f| f.script.is_some())
        .map(|f| f.index)
        .collect();
    assert_eq!(script_frames, [0, 4, 7, 11]);
    let frame1 = actions.frames[0].script.as_deref().unwrap_or_default();
    assert!(frame1.starts_with("var passes = 0;\nfunction greet(who) {\n\treturn \"Hello, \""));
    assert!(frame1.ends_with("scoreText = \"score: 0\";\n"));
    let frame8 = actions.frames[2].script.as_deref().unwrap_or_default();
    assert!(
        frame8.starts_with("if (passes < 2) {"),
        "entities are unescaped"
    );
    assert!(frame8.contains("gotoAndPlay(\\\"loop\\\")"));

    let text = texts(elements_at(layer(scene, "text"), 0));
    assert_eq!(text.len(), 1);
    let t = text[0];
    assert_eq!(t.kind, TextKind::Dynamic);
    assert_eq!(t.variable.as_deref(), Some("scoreText"));
    assert_eq!(t.name, None);
    assert!(t.auto_expand && t.selectable);
    assert_eq!(t.runs.len(), 1);
    assert_eq!(t.runs[0].text, "score: ?");
    assert_eq!(
        (t.runs[0].attrs.face.as_str(), t.runs[0].attrs.size),
        ("_sans", 20.0)
    );
    assert_eq!(t.matrix, Matrix::translate(22.0, 22.0));

    let clip = instance(elements_at(layer(scene, "clip"), 0), "ticker_mc");
    assert_eq!(clip.kind, SymbolKind::MovieClip);
    let clip_script = clip.script.as_deref().unwrap_or_default();
    assert!(clip_script.starts_with("onClipEvent (load) {\n\tthis.n = 0;"));
    assert!(clip_script.contains("onClipEvent (enterFrame) {"));

    let button = instance(elements_at(layer(scene, "button"), 0), "my_btn");
    assert_eq!(button.kind, SymbolKind::Button);
    assert_eq!(
        button.script.as_deref(),
        Some("on (release) {\n\ttrace(\"my_btn: release\");\n}\n")
    );

    let btn = &doc.library.symbols["Btn"];
    assert_eq!(btn.kind, SymbolKind::Button);
    assert_eq!(btn.timeline.frame_count(), 4, "up, over, down, hit");
    assert_eq!(doc.library.symbols["Ticker"].kind, SymbolKind::MovieClip);
}

#[test]
fn text_with_arabic() {
    let doc = open_clean("text");
    let scene = &doc.scenes[0];
    assert_eq!(
        layer_names(scene),
        ["actions", "dynamic", "arabic", "static"]
    );

    let arabic = texts(elements_at(layer(scene, "arabic"), 0));
    assert_eq!(arabic.len(), 1);
    let t = arabic[0];
    assert_eq!(t.kind, TextKind::Static);
    assert_eq!(t.runs.len(), 1);
    let chars: Vec<u32> = t.runs[0].text.chars().map(u32::from).collect();
    assert_eq!(
        chars,
        [
            0x645, 0x631, 0x62D, 0x628, 0x627, 0x20, 0x628, 0x627, 0x644, 0x639, 0x627, 0x644,
            0x645
        ]
    );
    assert_eq!(t.runs[0].text, "مرحبا بالعالم");
    assert_eq!(
        (t.runs[0].attrs.face.as_str(), t.runs[0].attrs.size),
        ("Tahoma", 28.0)
    );
    assert_eq!(t.matrix, Matrix::translate(22.0, 202.0));
    assert_eq!((t.width, t.height), (187.85, 33.8));
    assert!(t.auto_expand && !t.selectable);

    let statics = texts(elements_at(layer(scene, "static"), 0));
    assert_eq!(statics.len(), 4);
    let find = |prefix: &str| {
        statics
            .iter()
            .find(|t| t.runs[0].text.starts_with(prefix))
            .unwrap_or_else(|| panic!("no text starting with {prefix:?}"))
    };
    let left = find("Left aligned");
    assert_eq!(
        left.runs[0].text,
        "Left aligned, Arial 18\rsecond line of static text"
    );
    assert_eq!(left.runs[0].attrs.face, "ArialMT");
    assert_eq!(left.runs[0].attrs.align, Align::Left);
    let center = find("Centered");
    assert_eq!(center.runs[0].attrs.align, Align::Center);
    assert_eq!(center.runs[0].attrs.fill, Rgba::opaque(0xCC, 0, 0));
    assert_eq!(center.runs[0].attrs.face, "TimesNewRomanPSMT");
    let right = find("Right aligned");
    assert_eq!(right.runs[0].text.matches('\r').count(), 2);
    assert_eq!(right.runs[0].attrs.align, Align::Right);
    assert_eq!(right.runs[0].attrs.fill, Rgba::opaque(0, 0x33, 0xCC));
    let bold = find("Bold Italic");
    assert_eq!(bold.runs[0].attrs.face, "TimesNewRomanPS-BoldItalicMT");
    assert_eq!(bold.runs[0].attrs.size, 30.0);

    let dynamic = texts(elements_at(layer(scene, "dynamic"), 0));
    let d = dynamic[0];
    assert_eq!(d.kind, TextKind::Dynamic);
    assert_eq!(d.name.as_deref(), Some("dyn_txt"));
    assert_eq!(d.runs[0].text, "Dynamic text field");
    assert_eq!(d.runs[0].attrs.fill, Rgba::opaque(0x33, 0x33, 0x33));
    assert_eq!(d.runs[0].attrs.size, 16.0);
}

#[test]
fn bitmaps() {
    let doc = open_clean("bitmaps");
    assert_eq!(doc.background, Rgba::opaque(0xCC, 0xE0, 0xFF));
    let names: Vec<&str> = doc.library.bitmaps.keys().map(String::as_str).collect();
    assert_eq!(names, ["alpha-disc.png", "bars.jpg"]);

    // The lossless bitmap: premultiplied as stored. LIBRARY/bitmap-1.png is
    // JPEXS's export of the same data (it already carries Flash's alpha + 1),
    // so it matches after premultiplying with round(c × a / 255); the rule
    // against the original import is checked further down.
    let disc = &doc.library.bitmaps["alpha-disc.png"];
    assert_eq!((disc.width, disc.height, disc.smoothing), (128, 128, false));
    let BitmapData::Rgba {
        premultiplied,
        pixels,
    } = &disc.data
    else {
        panic!("pixels expected")
    };
    assert!(*premultiplied);
    let png = image::open(data("bitmaps/LIBRARY/bitmap-1.png"))
        .expect("the fixture PNG decodes")
        .to_rgba8();
    assert_eq!(png.dimensions(), (128, 128));
    let expected: Vec<u8> = png
        .pixels()
        .flat_map(|p| {
            let [r, g, b, a] = p.0;
            let pre = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
            [pre(r), pre(g), pre(b), a]
        })
        .collect();
    assert_eq!(pixels.len(), expected.len());
    let differing = pixels.iter().zip(&expected).filter(|(a, b)| a != b).count();
    assert_eq!(differing, 0, "the .dat plane matches the premultiplied PNG");
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3])
    );
    // Against the image Flash 8 imported (tests/fixtures/_src/alpha-disc.png):
    // semi-transparent pixels are stored with alpha a + 1 and colours
    // floor(c × a / 256); opaque colours are within 1 of the source (Flash's
    // import is not bit-exact, see MANIFEST.md). This is the plane Flash publishes, so the reader
    // must not "correct" it.
    let source = image::open(data("_src/alpha-disc.png"))
        .expect("the source PNG decodes")
        .to_rgba8();
    assert_eq!(source.dimensions(), (128, 128));
    let mut semi = 0;
    for (src, stored) in source.pixels().zip(pixels.as_chunks::<4>().0) {
        let [r, g, b, a] = src.0;
        match a {
            0 => assert_eq!(stored, &[0, 0, 0, 0]),
            255 => {
                assert_eq!(stored[3], 255);
                for (s, c) in stored[..3].iter().zip([r, g, b]) {
                    assert!(
                        s.abs_diff(c) <= 1,
                        "opaque {:?} stored as {stored:?}",
                        src.0
                    );
                }
            }
            _ => {
                semi += 1;
                let pre = |c: u8| ((u32::from(c) * u32::from(a)) / 256) as u8;
                assert_eq!(
                    stored,
                    &[pre(r), pre(g), pre(b), a + 1],
                    "source {:?}",
                    src.0
                );
            }
        }
    }
    assert!(semi > 1000, "the disc has a soft edge ({semi} pixels)");

    // The JPEG: passed through, identical to the imported file.
    let bars = &doc.library.bitmaps["bars.jpg"];
    assert_eq!((bars.width, bars.height), (160, 120));
    let BitmapData::Jpeg { bytes, alpha } = &bars.data else {
        panic!("JPEG expected")
    };
    assert_eq!(alpha, &None);
    let original = std::fs::read(data("bitmaps/LIBRARY/bitmap-2.jpg")).expect("fixture JPEG");
    assert_eq!(bytes, &original);

    let scene = &doc.scenes[0];
    let placed: Vec<(&str, Matrix)> = elements_at(layer(scene, "bitmaps"), 0)
        .iter()
        .filter_map(|e| match e {
            Element::Bitmap(b) => Some((b.library_item.as_str(), b.matrix)),
            _ => None,
        })
        .collect();
    assert_eq!(
        placed,
        [
            ("alpha-disc.png", Matrix::translate(26.0, 26.0)),
            (
                "alpha-disc.png",
                Matrix::new(
                    1.0825347900390625,
                    0.625,
                    -0.625,
                    1.0825347900390625,
                    100.7,
                    180.7
                )
            ),
            ("bars.jpg", Matrix::translate(220.0, 20.0)),
            (
                "bars.jpg",
                Matrix::new(
                    0.7244415283203125,
                    -0.1941070556640625,
                    0.194122314453125,
                    0.7244415283203125,
                    230.35,
                    222.05
                )
            ),
        ]
    );

    let fill_of = |name: &str| {
        let s = shapes(elements_at(layer(scene, name), 0))[0];
        match &s.fills[0] {
            FillStyle::Bitmap(b) => (b.clone(), s.clone()),
            other => panic!("bitmap fill expected, got {other:?}"),
        }
    };
    let (plain, plain_shape) = fill_of("bitmapfill");
    assert_eq!(plain.library_item, "bars.jpg");
    assert_eq!(plain.matrix, Matrix::new(20.0, 0.0, 0.0, 20.0, 385.0, 20.0));
    assert!(!plain.clipped);
    assert_eq!(edge_bounds(&plain_shape), (7700.0, 400.0, 10900.0, 2800.0));
    let (oval, oval_shape) = fill_of("bitmapfill-oval");
    assert_eq!(
        oval.matrix,
        Matrix::new(
            18.7939453125,
            6.84051513671875,
            -6.84051513671875,
            18.7939453125,
            415.4,
            201.3
        )
    );
    assert_eq!(oval_shape.strokes[0].weight, 2.0);
    // Fractional hex coordinates: the first edge starts at #2024.80 = 8228.5 tw.
    assert!(oval_shape.edges.iter().any(|e| matches!(
        e.path.first(),
        Some(PathCmd::MoveTo(p)) if p.x == 8228.5 && p.y == 6026.0
    )));
}

#[test]
fn smoke() {
    let doc = open_clean("smoke");
    assert_eq!(
        (doc.width, doc.height, doc.frame_rate),
        (550.0, 400.0, 24.0)
    );
    let scene = &doc.scenes[0];
    assert_eq!(scene.frame_count(), 20);
    assert_eq!(layer_names(scene), ["actions", "Layer 1"]);
    let names: Vec<&str> = doc.library.symbols.keys().map(String::as_str).collect();
    assert_eq!(names, ["Box"]);
    let l = layer(scene, "Layer 1");
    assert!(matches!(&l.frames[0].tween, Tween::Motion(m) if m.rotate == RotateMode::Auto));
    assert_eq!((l.frames[0].index, l.frames[0].duration), (0, 19));
    let start = instances(elements_at(l, 0))[0];
    let end = instances(elements_at(l, 19))[0];
    assert_eq!(start.library_item, "Box");
    assert_eq!(start.matrix, Matrix::translate(100.0, 100.0));
    assert_eq!(end.matrix, Matrix::translate(400.0, 250.0));
    assert_eq!(
        layer(scene, "actions").frames[0].script.as_deref(),
        Some("trace(\"hello from Flash 8\");")
    );
}

#[test]
fn publish_settings_of_the_fixtures() {
    for name in ["shapes-basic", "as2", "bitmaps", "smoke"] {
        let doc = open_clean(name);
        let p = &doc.publish;
        assert_eq!((p.flash_version, p.actionscript_version), (8, 2), "{name}");
        assert!(p.compress, "{name}");
        assert_eq!(p.jpeg_quality, 80, "{name}");
        assert!(!p.protect && !p.omit_trace && !p.use_network, "{name}");
        assert!(p.package_paths.is_empty(), "{name}");
    }
}
