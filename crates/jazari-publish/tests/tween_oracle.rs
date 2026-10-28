//! Motion tweens against Flash 8, frame by frame.
//!
//! Each test publishes a fixture (the `tweens` and `smoke` fixtures and the
//! probes in `tests/fixtures/tween-probes`), simulates the root display
//! list of our SWF and of Flash 8's `flash8.swf`, pairs the movie clips of
//! both by depth order (one per tweened layer) and compares, per frame,
//! the matrix (Fixed16 units of a..d, twips of tx/ty), the colour transform
//! and the filters. The worst error per layer is printed (run with
//! `--nocapture`) and checked against the bound that layer is known to
//! meet; the bounds and the reasons for the non-zero ones are documented in
//! `tests/fixtures/MANIFEST.md` (section "tween-probes") and in
//! `src/tween/matrix.rs`.
//!
//! No Ruffle involved.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use jazari_publish::BuildMode;
use jazari_xfl::model::{Element, SymbolKind};
use swf::{CharacterId, ColorTransform, Filter, Matrix, PlaceObjectAction, Tag};

/// What one depth holds after a frame.
#[derive(Debug, Clone, PartialEq)]
struct State {
    char: CharacterId,
    matrix: Matrix,
    cxform: ColorTransform,
    filters: Vec<Filter>,
}

/// The root display list after every frame, the sprites, and the width (in
/// twips) of every shape.
struct Movie {
    frames: Vec<BTreeMap<u16, State>>,
    sprites: Vec<CharacterId>,
    shape_width: BTreeMap<CharacterId, i32>,
}

fn simulate(bytes: &[u8]) -> Movie {
    common::with_parsed(bytes, |swf| {
        let mut movie = Movie {
            frames: Vec::new(),
            sprites: Vec::new(),
            shape_width: BTreeMap::new(),
        };
        let mut dl: BTreeMap<u16, State> = BTreeMap::new();
        for t in &swf.tags {
            match t {
                Tag::DefineShape(s) => {
                    movie.shape_width.insert(s.id, s.shape_bounds.width().get());
                }
                Tag::DefineSprite(s) => movie.sprites.push(s.id),
                Tag::RemoveObject(r) => {
                    dl.remove(&r.depth);
                }
                Tag::PlaceObject(p) => {
                    if let PlaceObjectAction::Place(id) = p.action {
                        dl.insert(
                            p.depth,
                            State {
                                char: id,
                                matrix: Matrix::IDENTITY,
                                cxform: ColorTransform::IDENTITY,
                                filters: Vec::new(),
                            },
                        );
                    }
                    if let Some(e) = dl.get_mut(&p.depth) {
                        if let PlaceObjectAction::Replace(id) = p.action {
                            e.char = id;
                        }
                        if let Some(m) = p.matrix {
                            e.matrix = m;
                        }
                        if let Some(c) = p.color_transform {
                            e.cxform = c;
                        }
                        if let Some(f) = &p.filters {
                            e.filters = f.clone();
                        }
                    }
                }
                Tag::ShowFrame => movie.frames.push(dl.clone()),
                _ => {}
            }
        }
        movie
    })
}

/// `tests/fixtures/tween-probes`.
fn probes_dir() -> PathBuf {
    common::fixtures_dir().join("tween-probes")
}

/// Opens, publishes and reads a fixture (`dir` holds `xfl/` and
/// `flash8.swf`); returns the document, ours and Flash's.
fn load(dir: &std::path::Path) -> (jazari_xfl::Document, Movie, Movie) {
    let (doc, _) = jazari_xfl::open(&dir.join("xfl"))
        .unwrap_or_else(|e| panic!("open {}: {e}", dir.display()));
    let out = common::publish_doc(&doc, BuildMode::Release);
    let errors = common::errors(&out);
    assert!(errors.is_empty(), "{}: {errors:?}", dir.display());
    let ours = simulate(&out.swf.expect("swf"));
    let flash = std::fs::read(dir.join("flash8.swf")).expect("flash8.swf");
    (doc, ours, simulate(&flash))
}

/// The root layers that hold a movie clip, bottom first: the order of the
/// sprites in the display list.
fn clip_layers(doc: &jazari_xfl::Document) -> Vec<String> {
    let Some(tl) = doc.scenes.first() else {
        return Vec::new();
    };
    tl.layers
        .iter()
        .rev()
        .filter(|l| {
            l.frames.iter().any(|f| {
                f.elements
                    .iter()
                    .any(|e| matches!(e, Element::Instance(i) if i.kind == SymbolKind::MovieClip))
            })
        })
        .map(|l| l.name.clone())
        .collect()
}

/// The worst differences seen on one layer.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Worst {
    /// a..d, in 1/65536.
    linear: i64,
    /// tx/ty, in twips.
    translate: i64,
    /// Colour transform numbers (multipliers in 1/256, offsets).
    color: i64,
    /// Frames whose filter lists differ (colour matrices within 1e-3).
    filter_frames: usize,
    /// Frame (1-based) of the worst linear difference.
    at: usize,
}

fn cx(c: &ColorTransform) -> [i64; 8] {
    [
        c.r_multiply.get().into(),
        c.g_multiply.get().into(),
        c.b_multiply.get().into(),
        c.a_multiply.get().into(),
        c.r_add.into(),
        c.g_add.into(),
        c.b_add.into(),
        c.a_add.into(),
    ]
}

/// Filter lists equal; colour matrices (Adjust Color) within 1e-3 of each
/// entry, relative for the offsets: Flash computes them in its own float
/// order and stores f32.
fn filters_match(a: &[Filter], b: &[Filter]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Filter::ColorMatrixFilter(p), Filter::ColorMatrixFilter(q)) => p
                .matrix
                .iter()
                .zip(&q.matrix)
                .all(|(u, v)| (u - v).abs() <= 1e-3 * v.abs().max(1.0)),
            _ => x == y,
        })
}

/// Compares the sprites of both movies frame by frame, per layer.
fn compare(
    name: &str,
    doc: &jazari_xfl::Document,
    ours: &Movie,
    flash: &Movie,
) -> Vec<(String, Worst)> {
    assert_eq!(ours.frames.len(), flash.frames.len(), "{name}: frame count");
    let layers = clip_layers(doc);
    let mut worst = vec![Worst::default(); layers.len()];
    for (f, (o, e)) in ours.frames.iter().zip(&flash.frames).enumerate() {
        let pick = |m: &Movie, dl: &BTreeMap<u16, State>| -> Vec<State> {
            dl.values()
                .filter(|s| m.sprites.contains(&s.char))
                .cloned()
                .collect()
        };
        let (o, e) = (pick(ours, o), pick(flash, e));
        assert_eq!(o.len(), e.len(), "{name}: sprites at frame {}", f + 1);
        assert!(
            o.len() <= layers.len(),
            "{name}: more sprites than clip layers"
        );
        for (i, (a, b)) in o.iter().zip(&e).enumerate() {
            let w = &mut worst[i];
            let lin = [
                a.matrix.a.get() - b.matrix.a.get(),
                a.matrix.b.get() - b.matrix.b.get(),
                a.matrix.c.get() - b.matrix.c.get(),
                a.matrix.d.get() - b.matrix.d.get(),
            ]
            .iter()
            .map(|d| i64::from(*d).abs())
            .max()
            .unwrap_or(0);
            if lin > w.linear {
                w.linear = lin;
                w.at = f + 1;
            }
            let tr = i64::from((a.matrix.tx.get() - b.matrix.tx.get()).abs())
                .max(i64::from((a.matrix.ty.get() - b.matrix.ty.get()).abs()));
            w.translate = w.translate.max(tr);
            let col = cx(&a.cxform)
                .iter()
                .zip(cx(&b.cxform))
                .map(|(x, y)| (x - y).abs())
                .max()
                .unwrap_or(0);
            w.color = w.color.max(col);
            if !filters_match(&a.filters, &b.filters) {
                if w.filter_frames == 0 {
                    eprintln!(
                        "{name}/{}: first filter difference at frame {}:\n  ours  {:?}\n  flash {:?}",
                        layers[i],
                        f + 1,
                        a.filters,
                        b.filters
                    );
                }
                w.filter_frames += 1;
            }
        }
    }
    let out: Vec<(String, Worst)> = layers.into_iter().zip(worst).collect();
    for (layer, w) in &out {
        eprintln!(
            "{name}/{layer}: worst a..d {} (frame {}), tx/ty {} twips, colour {}, filter frames {}",
            w.linear, w.at, w.translate, w.color, w.filter_frames
        );
    }
    out
}

/// Asserts per-layer bounds: `(layer, linear, translate)`; every layer not
/// listed must be exact. Colours and filters must always be exact.
fn check(name: &str, results: &[(String, Worst)], bounds: &[(&str, i64, i64)]) {
    for (layer, w) in results {
        let (lin, tr) = bounds
            .iter()
            .find(|(l, _, _)| l == layer)
            .map(|(_, a, b)| (*a, *b))
            .unwrap_or((0, 0));
        assert!(
            w.linear <= lin && w.translate <= tr,
            "{name}/{layer}: a..d off by {} (bound {lin}), tx/ty by {} twips (bound {tr})",
            w.linear,
            w.translate
        );
        assert_eq!(w.color, 0, "{name}/{layer}: colour transform differs");
        assert_eq!(w.filter_frames, 0, "{name}/{layer}: filters differ");
    }
}

/// The `tweens` fixture: CW/CCW/auto rotation, eased scale, alpha and
/// consecutive eased tweens. Rotations, the alpha tween and every colour
/// are exact. Residuals: one eased move of `consecutive` is 1 twip off, the
/// 2 → 0.5 ease-out scale is 1/65536 off on 8 frames, and Flash 8 moved the
/// still `scale_ease` clip by 1 twip on one frame.
#[test]
fn tweens_fixture_matches_flash_8() {
    let dir = common::fixtures_dir().join("tweens");
    let (doc, ours, flash) = load(&dir);
    let r = compare("tweens", &doc, &ours, &flash);
    check("tweens", &r, &[("consecutive", 0, 1), ("scale_ease", 1, 1)]);
}

/// The `smoke` fixture's linear tween is exact.
#[test]
fn smoke_fixture_matches_flash_8() {
    let dir = common::fixtures_dir().join("smoke");
    let (doc, ours, flash) = load(&dir);
    let r = compare("smoke", &doc, &ours, &flash);
    check("smoke", &r, &[]);
}

/// Rotation, scale, skew, mirroring, rotate modes and "scale" off. Layers
/// whose keyframe angles are axis-aligned (`r0_90`, `r0_m90`, `r_cw360`,
/// `scale_rot`, `mirror`, `cw_times2`, `none_rot`, `eased_rot`) are exact
/// in a..d. The
/// others keep the residual of Flash 8's own decomposition of truncated
/// keyframe matrices: a frame can land on the other side of a sine table
/// entry (up to ~370/65536) and scales can differ by a few units. The
/// probe set its matrices through JSFL, which left the transformation
/// point where it was on the stage (so it is off the registration point at
/// the end keyframes): translations are within 3 twips.
#[test]
fn matrices_probe_matches_flash_8() {
    let (doc, ours, flash) = load(&probes_dir().join("matrices"));
    let r = compare("matrices", &doc, &ours, &flash);
    let b = 400;
    check(
        "matrices",
        &r,
        &[
            ("r0_6", b, 0),
            ("r42_48", b, 1),
            ("r84_90", b, 2),
            ("r10_100", b, 1),
            ("scale_rot", 0, 1),
            ("skew_x", b, 1),
            ("skew_y", b, 1),
            ("skew_xy", b, 2),
            ("no_scale", b, 3),
            ("aniso", b, 2),
            ("ccw_long", b, 2),
            ("cw_times2", 0, 1),
            ("r170_m170", b, 2),
            ("eased_rot", 0, 1),
        ],
    );
}

/// The transformation point moves in a straight line; with it off the
/// registration point, translations are within 2 twips.
#[test]
fn tpoint_probe_matches_flash_8() {
    let (doc, ours, flash) = load(&probes_dir().join("tpoint"));
    let r = compare("tpoint", &doc, &ours, &flash);
    check(
        "tpoint",
        &r,
        &[
            ("tp_corner_rot", 0, 1),
            ("tp_corner_scale", 0, 1),
            ("tp_corner_ease", 0, 2),
            ("tp_corner_cw", 100, 2),
            ("tp_corner_rot_tl", 0, 2),
        ],
    );
}

/// Classic ease at ±30/±70/±100 on moves, ±50/±100 on scales, ease 60 on
/// a turning, growing and fading clip, and custom ease curves: a..d and
/// colours exact. `scaleSelection` nudged the transformation point of the
/// scaled clips by 0.05 px, so their translations are within 1 twip;
/// `custom_and_classic` (a custom curve with a classic ease also set: the
/// curve wins) is 1 twip off on two frames.
#[test]
fn ease_probe_matches_flash_8() {
    let (doc, ours, flash) = load(&probes_dir().join("ease"));
    let r = compare("ease", &doc, &ours, &flash);
    check(
        "ease",
        &r,
        &[
            ("scale_m50", 0, 1),
            ("scale_p50", 0, 1),
            ("scale_m100", 0, 1),
            ("scale_p100", 0, 1),
            ("scale_custom", 0, 1),
            ("ease_all", 0, 1),
            ("custom_and_classic", 0, 1),
        ],
    );
}

/// Colour effects and filter lists: exact (Adjust Color matrices to f32
/// precision).
#[test]
fn color_probe_matches_flash_8() {
    let (doc, ours, flash) = load(&probes_dir().join("color"));
    let r = compare("color", &doc, &ours, &flash);
    check("color", &r, &[]);
}

/// The flattened `Counter` graphic shows the same frame as Flash 8 in and
/// after motion tweens, for every loop mode, sync on and off.
#[test]
fn graphics_probe_matches_flash_8() {
    let (_, ours, flash) = load(&probes_dir().join("graphics"));
    let bars = |m: &Movie| -> Vec<Vec<(i32, i32, i32)>> {
        m.frames
            .iter()
            .map(|f| {
                let mut v: Vec<(i32, i32, i32)> = f
                    .values()
                    .filter_map(|s| {
                        let w = m.shape_width.get(&s.char)?;
                        Some((s.matrix.ty.get(), s.matrix.tx.get(), *w))
                    })
                    .collect();
                v.sort_unstable();
                v
            })
            .collect()
    };
    let (o, e) = (bars(&ours), bars(&flash));
    assert_eq!(o.len(), e.len());
    for (f, (a, b)) in o.iter().zip(&e).enumerate() {
        assert_eq!(a, b, "graphics: frame {}", f + 1);
    }
}
