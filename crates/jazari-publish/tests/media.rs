//! Bitmaps, groups and masks against Flash 8's `flash8.swf` of the
//! `bitmaps`, `groups` and `masks` fixtures, compared structurally (no
//! Ruffle): bitmap data, fill styles, baked edge bounds, instance matrices,
//! drawing order, clip depths and the tweened mask's matrices.

mod common;

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

use common::{errors, fixtures_dir, flash8, publish_doc, publish_fixture, with_parsed};
use jazari_publish::{BuildMode, Severity};
use swf::{CharacterId, FillStyle, Matrix, PlaceObjectAction, ShapeRecord, Tag};

// ---------------------------------------------------------------- helpers

/// What is at one depth.
#[derive(Debug, Clone, PartialEq)]
struct Placed {
    char: CharacterId,
    matrix: Matrix,
    name: Option<String>,
    clip_depth: Option<u16>,
    /// The frame (0-based) of its first placement.
    since: usize,
}

/// The display list after each frame of a tag list (root or sprite).
fn frames_of(tags: &[Tag<'_>]) -> Vec<BTreeMap<u16, Placed>> {
    let mut dl: BTreeMap<u16, Placed> = BTreeMap::new();
    let mut out = Vec::new();
    for t in tags {
        match t {
            Tag::RemoveObject(r) => {
                dl.remove(&r.depth);
            }
            Tag::PlaceObject(p) => {
                if let PlaceObjectAction::Place(id) = p.action {
                    dl.insert(
                        p.depth,
                        Placed {
                            char: id,
                            matrix: Matrix::IDENTITY,
                            name: None,
                            clip_depth: None,
                            since: out.len(),
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
                    if let Some(n) = p.name {
                        e.name = Some(n.to_str_lossy(swf::UTF_8).into_owned());
                    }
                    if p.clip_depth.is_some() {
                        e.clip_depth = p.clip_depth;
                    }
                }
            }
            Tag::ShowFrame => out.push(dl.clone()),
            _ => {}
        }
    }
    out
}

/// What each character is, and the frames of every sprite.
#[derive(Debug, Default)]
struct Movie {
    root: Vec<BTreeMap<u16, Placed>>,
    sprites: BTreeMap<CharacterId, Vec<BTreeMap<u16, Placed>>>,
    shapes: BTreeMap<CharacterId, swf::Shape>,
    bitmaps: BTreeMap<CharacterId, &'static str>,
}

fn movie(bytes: &[u8]) -> Movie {
    with_parsed(bytes, |s| {
        let mut m = Movie {
            root: frames_of(&s.tags),
            ..Movie::default()
        };
        for t in &s.tags {
            match t {
                Tag::DefineSprite(sp) => {
                    m.sprites.insert(sp.id, frames_of(&sp.tags));
                }
                Tag::DefineShape(sh) => {
                    m.shapes.insert(sh.id, (**sh).clone());
                }
                Tag::DefineBitsLossless(b) => {
                    m.bitmaps.insert(b.id, "lossless");
                }
                Tag::DefineBitsJpeg2 { id, .. } => {
                    m.bitmaps.insert(*id, "jpeg");
                }
                Tag::DefineBitsJpeg3(j) => {
                    m.bitmaps.insert(j.id, "jpeg+alpha");
                }
                _ => {}
            }
        }
        m
    })
}

/// Every named placement, in the root and in sprites, by name.
fn named(m: &Movie) -> BTreeMap<String, Matrix> {
    let mut out = BTreeMap::new();
    for frames in std::iter::once(&m.root).chain(m.sprites.values()) {
        for p in frames.iter().flat_map(|f| f.values()) {
            if let Some(n) = &p.name {
                out.entry(n.clone()).or_insert(p.matrix);
            }
        }
    }
    out
}

/// Solid fill colours (RGBA) to the edge bounds (twips, anchor and control
/// points, as swfdump computes them) of every style block that uses them.
fn fill_bounds(m: &Movie) -> BTreeMap<[u8; 4], Vec<[i32; 4]>> {
    let mut out: BTreeMap<[u8; 4], Vec<[i32; 4]>> = BTreeMap::new();
    for shape in m.shapes.values() {
        let mut styles = shape.styles.fill_styles.clone();
        let mut block: BTreeMap<u32, [i32; 4]> = BTreeMap::new();
        let flush = |styles: &[FillStyle],
                     block: &mut BTreeMap<u32, [i32; 4]>,
                     out: &mut BTreeMap<[u8; 4], Vec<[i32; 4]>>| {
            for (idx, b) in std::mem::take(block) {
                if let Some(FillStyle::Color(c)) = styles.get(idx as usize - 1) {
                    out.entry([c.r, c.g, c.b, c.a]).or_default().push(b);
                }
            }
        };
        let (mut x, mut y) = (0i32, 0i32);
        let (mut f0, mut f1) = (0u32, 0u32);
        let grow = |block: &mut BTreeMap<u32, [i32; 4]>, f0: u32, f1: u32, pts: &[(i32, i32)]| {
            for f in [f0, f1].into_iter().filter(|f| *f != 0) {
                let b = block
                    .entry(f)
                    .or_insert([i32::MAX, i32::MAX, i32::MIN, i32::MIN]);
                for &(px, py) in pts {
                    b[0] = b[0].min(px);
                    b[1] = b[1].min(py);
                    b[2] = b[2].max(px);
                    b[3] = b[3].max(py);
                }
            }
        };
        for rec in &shape.shape {
            match rec {
                ShapeRecord::StyleChange(sc) => {
                    if let Some(ns) = &sc.new_styles {
                        flush(&styles, &mut block, &mut out);
                        styles = ns.fill_styles.clone();
                        f0 = 0;
                        f1 = 0;
                    }
                    if let Some(p) = sc.move_to {
                        x = p.x.get();
                        y = p.y.get();
                    }
                    if let Some(v) = sc.fill_style_0 {
                        f0 = v;
                    }
                    if let Some(v) = sc.fill_style_1 {
                        f1 = v;
                    }
                }
                ShapeRecord::StraightEdge { delta } => {
                    let from = (x, y);
                    x += delta.dx.get();
                    y += delta.dy.get();
                    grow(&mut block, f0, f1, &[from, (x, y)]);
                }
                ShapeRecord::CurvedEdge {
                    control_delta,
                    anchor_delta,
                } => {
                    let from = (x, y);
                    let c = (x + control_delta.dx.get(), y + control_delta.dy.get());
                    x = c.0 + anchor_delta.dx.get();
                    y = c.1 + anchor_delta.dy.get();
                    grow(&mut block, f0, f1, &[from, c, (x, y)]);
                }
            }
        }
        flush(&styles, &mut block, &mut out);
    }
    for v in out.values_mut() {
        v.sort_unstable();
    }
    out
}

fn inflate(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .expect("zlib");
    out
}

/// Bitmap payloads, decompressed: ("lossless", w, h, ARGB plane) and
/// ("jpeg", 0, 0, bytes), sorted.
fn bitmap_payloads(bytes: &[u8]) -> Vec<(&'static str, u16, u16, Vec<u8>)> {
    with_parsed(bytes, |s| {
        let mut out = Vec::new();
        for t in &s.tags {
            match t {
                Tag::DefineBitsLossless(b) => {
                    out.push(("lossless", b.width, b.height, inflate(&b.data)))
                }
                Tag::DefineBitsJpeg2 { jpeg_data, .. } => {
                    out.push(("jpeg", 0, 0, jpeg_data.to_vec()))
                }
                _ => {}
            }
        }
        out.sort();
        out
    })
}

fn matrix_close(a: &Matrix, b: &Matrix, twips: i32) -> bool {
    let fixed = |x: swf::Fixed16, y: swf::Fixed16| (x.get() - y.get()).abs() <= 1;
    fixed(a.a, b.a)
        && fixed(a.b, b.b)
        && fixed(a.c, b.c)
        && fixed(a.d, b.d)
        && (a.tx.get() - b.tx.get()).abs() <= twips
        && (a.ty.get() - b.ty.get()).abs() <= twips
}

fn copy_dir_without(from: &Path, to: &Path, skip: &str) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from)
        .expect("read_dir")
        .filter_map(Result::ok)
    {
        let path = e.path();
        if e.file_name() == skip {
            continue;
        }
        if path.is_dir() {
            copy_dir_without(&path, &to.join(e.file_name()), skip);
        } else {
            std::fs::copy(&path, to.join(e.file_name())).expect("copy");
        }
    }
}

// ---------------------------------------------------------------- bitmaps

/// The lossless plane and the JPEG are exactly the data Flash 8 published,
/// with no diagnostic.
#[test]
fn bitmaps_publish_flash_8_s_data() {
    let out = publish_fixture("bitmaps", BuildMode::Release);
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
    let ours = out.swf.expect("swf");
    let theirs = flash8("bitmaps");
    let (o, e) = (bitmap_payloads(&ours), bitmap_payloads(&theirs));
    assert_eq!(o.len(), 2);
    assert_eq!(o, e);
}

/// Straight PNG pixels (the `LIBRARY` file, read when `bin/` is missing) are
/// premultiplied into the same plane Flash 8 published.
#[test]
fn png_pixels_are_premultiplied_like_flash_8() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("media-bitmaps-png");
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir_without(&fixtures_dir().join("bitmaps").join("xfl"), &dir, "bin");
    let (doc, _) = jazari_xfl::open(&dir).expect("open the copy");
    let png = doc.library.bitmaps.get("alpha-disc.png").expect("item");
    assert!(matches!(
        png.data,
        jazari_xfl::model::BitmapData::Rgba {
            premultiplied: false,
            ..
        }
    ));
    let out = publish_doc(&doc, BuildMode::Release);
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    let ours = bitmap_payloads(&out.swf.expect("swf"));
    assert_eq!(ours, bitmap_payloads(&flash8("bitmaps")));
}

/// Fill types (clipped/repeating, smoothed or not), bitmap kinds and fill
/// matrices equal Flash 8's (its placeholder fills with id 65535 aside).
#[test]
fn bitmap_fills_match_flash_8() {
    let fills = |bytes: &[u8]| {
        let m = movie(bytes);
        let mut v: Vec<String> = Vec::new();
        for shape in m.shapes.values() {
            let mut all = shape.styles.fill_styles.clone();
            for rec in &shape.shape {
                if let ShapeRecord::StyleChange(sc) = rec
                    && let Some(ns) = &sc.new_styles
                {
                    all.extend(ns.fill_styles.iter().cloned());
                }
            }
            for f in all {
                if let FillStyle::Bitmap {
                    id,
                    matrix,
                    is_smoothed,
                    is_repeating,
                } = f
                    && id != 65535
                {
                    v.push(format!(
                        "{} smoothed={is_smoothed} repeating={is_repeating} {matrix:?}",
                        m.bitmaps.get(&id).copied().unwrap_or("?")
                    ));
                }
            }
        }
        v.sort();
        v
    };
    let ours = publish_fixture("bitmaps", BuildMode::Release)
        .swf
        .expect("swf");
    let o = fills(&ours);
    assert_eq!(o.len(), 6, "{o:#?}");
    assert_eq!(o, fills(&flash8("bitmaps")));
}

/// Bitmap instances are rectangles baked into the stage: the bounds of
/// every bitmap-filled shape together equal Flash 8's single shape, to the
/// twip (Flash rounds a rotated corner up by one twip where we round to the
/// nearest).
#[test]
fn bitmap_instances_are_baked_rectangles() {
    let union = |bytes: &[u8]| {
        let m = movie(bytes);
        let mut u = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        for frame in &m.root {
            for p in frame.values() {
                assert_eq!(p.matrix, Matrix::IDENTITY, "shapes are placed as is");
                if let Some(s) = m.shapes.get(&p.char) {
                    let b = s.shape_bounds;
                    u[0] = u[0].min(b.x_min.get());
                    u[1] = u[1].min(b.y_min.get());
                    u[2] = u[2].max(b.x_max.get());
                    u[3] = u[3].max(b.y_max.get());
                }
            }
        }
        u
    };
    let ours = publish_fixture("bitmaps", BuildMode::Release)
        .swf
        .expect("swf");
    let (o, e) = (union(&ours), union(&flash8("bitmaps")));
    for i in 0..4 {
        assert!((o[i] - e[i]).abs() <= 1, "{o:?} vs {e:?}");
    }
}

// ----------------------------------------------------------------- groups

/// Every grouped shape lands where Flash 8 baked it: per fill colour, the
/// edge bounds of each style block equal Flash's to one twip (member
/// matrices are absolute and never composed with the groups).
#[test]
fn group_members_land_where_flash_8_baked_them() {
    let out = publish_fixture("groups", BuildMode::Release);
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
    let o = fill_bounds(&movie(&out.swf.expect("swf")));
    let e = fill_bounds(&movie(&flash8("groups")));
    assert_eq!(o.keys().collect::<Vec<_>>(), e.keys().collect::<Vec<_>>());
    for (colour, theirs) in &e {
        let ours = &o[colour];
        assert_eq!(ours.len(), theirs.len(), "{colour:?}");
        for (a, b) in ours.iter().zip(theirs) {
            assert!(
                a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1),
                "{colour:?}: {a:?} vs {b:?}"
            );
        }
    }
}

/// Instances inside groups keep their names and Flash 8's exact matrices.
#[test]
fn grouped_instances_keep_flash_8_matrices() {
    let ours = named(&movie(
        &publish_fixture("groups", BuildMode::Release)
            .swf
            .expect("swf"),
    ));
    let theirs = named(&movie(&flash8("groups")));
    assert_eq!(
        ours.keys().collect::<Vec<_>>(),
        ["clip_mc", "inclip_mc", "ingroup_mc", "nested_mc"]
    );
    assert_eq!(ours, theirs);
}

/// The merge-mode raw shape of a frame or group is drawn under the groups,
/// drawing objects and instances beside it, as Flash 8 draws it.
#[test]
fn raw_shapes_are_drawn_under_their_siblings() {
    // Colours in drawing order (by depth, then style block), and the depth
    // of every named instance.
    let order = |bytes: &[u8]| {
        let m = movie(bytes);
        let mut colours = Vec::new();
        let mut depths = BTreeMap::new();
        for (depth, p) in m.root.last().expect("a frame") {
            if let Some(n) = &p.name {
                depths.insert(n.clone(), *depth);
            }
            let Some(s) = m.shapes.get(&p.char) else {
                continue;
            };
            let mut blocks = vec![s.styles.fill_styles.clone()];
            for rec in &s.shape {
                if let ShapeRecord::StyleChange(sc) = rec
                    && let Some(ns) = &sc.new_styles
                {
                    blocks.push(ns.fill_styles.clone());
                }
            }
            for f in blocks.into_iter().flatten() {
                if let FillStyle::Color(c) = f {
                    colours.push((format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b), *depth));
                }
            }
        }
        (colours, depths)
    };
    let ours = publish_fixture("groups", BuildMode::Release)
        .swf
        .expect("swf");
    for (colours, depths) in [order(&ours), order(&flash8("groups"))] {
        let stack: Vec<&str> = colours
            .iter()
            .map(|(c, _)| c.as_str())
            .filter(|c| ["DDDDDD", "FF00FF", "00FFFF", "FFAA00"].contains(c))
            .collect();
        assert_eq!(stack, ["DDDDDD", "FF00FF", "00FFFF", "FFAA00"]);
        let grey = colours
            .iter()
            .find(|(c, _)| c == "999999")
            .map(|(_, d)| *d)
            .expect("#999999");
        assert!(grey < depths["ingroup_mc"], "{colours:?} {depths:?}");
    }
}

// ------------------------------------------------------------------ masks

/// For every mask of a display list: whether the mask is a shape or a
/// sprite, the names of the instances it clips and how many unnamed
/// characters it clips.
fn clip_summary(
    frame: &BTreeMap<u16, Placed>,
    m: &Movie,
) -> Vec<(&'static str, Vec<String>, usize)> {
    let mut out = Vec::new();
    for (depth, p) in frame {
        let Some(clip) = p.clip_depth else {
            continue;
        };
        let kind = if m.shapes.contains_key(&p.char) {
            "shape"
        } else if m.sprites.contains_key(&p.char) {
            "sprite"
        } else {
            "other"
        };
        let clipped: Vec<&Placed> = frame.range(depth + 1..=clip).map(|(_, c)| c).collect();
        let mut names: Vec<String> = clipped.iter().filter_map(|c| c.name.clone()).collect();
        names.sort();
        let unnamed = clipped.iter().filter(|c| c.name.is_none()).count();
        out.push((kind, names, unnamed));
    }
    out.sort();
    out
}

/// Each mask clips exactly what Flash 8's clips (the same masked content,
/// root and inside `MaskedClip`), and raw masks are DefineShapes placed
/// directly, as Flash places them.
#[test]
fn masks_clip_what_flash_8_clips() {
    let ours = movie(
        &publish_fixture("masks", BuildMode::Release)
            .swf
            .expect("swf"),
    );
    let theirs = movie(&flash8("masks"));
    for f in [0, 9, 19] {
        let o = clip_summary(&ours.root[f], &ours);
        assert_eq!(o.len(), 7, "frame {}: {o:?}", f + 1);
        assert_eq!(o, clip_summary(&theirs.root[f], &theirs), "frame {}", f + 1);
    }
    let in_sprites = |m: &Movie| -> Vec<_> {
        m.sprites
            .values()
            .filter_map(|frames| frames.first())
            .flat_map(|f| clip_summary(f, m))
            .collect()
    };
    assert_eq!(in_sprites(&ours), in_sprites(&theirs));
    assert_eq!(
        in_sprites(&ours),
        [("shape", vec!["inner_box_mc".to_owned()], 1)]
    );
}

/// Only the backmost element of a mask layer is published, as in Flash 8,
/// and each one left out is reported where it is.
#[test]
fn dropped_mask_elements_are_reported_not_published() {
    let out = publish_fixture("masks", BuildMode::Release);
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    for (layer, name) in [("d_mask", "maskdisc_mc"), ("h_mask", "h_disc2_mc")] {
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.severity == Severity::Warning
                    && d.origin.layer.as_deref() == Some(layer)
                    && d.message.contains(name)),
            "{layer}: {:#?}",
            out.diagnostics
        );
    }
    for m in [
        movie(out.swf.as_deref().expect("swf")),
        movie(&flash8("masks")),
    ] {
        let names = named(&m);
        assert!(!names.contains_key("maskdisc_mc") && !names.contains_key("h_disc2_mc"));
    }
}

/// The motion-tweened mask moves like Flash 8's at every frame, whether it
/// is placed directly or plays inside a generated wrapper sprite.
#[test]
fn tweened_mask_follows_flash_8() {
    // The matrix of `name` at each root frame: a root placement, or a
    // child of the clipping sprite at its own frame.
    let track = |m: &Movie, name: &str| -> Vec<Matrix> {
        m.root
            .iter()
            .enumerate()
            .map(|(f, frame)| {
                for p in frame.values() {
                    if p.name.as_deref() == Some(name) {
                        return p.matrix;
                    }
                    if p.clip_depth.is_none() {
                        continue;
                    }
                    let Some(inner) = m.sprites.get(&p.char) else {
                        continue;
                    };
                    let k = (f - p.since).min(inner.len().saturating_sub(1));
                    if let Some(c) = inner
                        .get(k)
                        .and_then(|fr| fr.values().find(|c| c.name.as_deref() == Some(name)))
                    {
                        assert_eq!(p.matrix, Matrix::IDENTITY, "wrappers are placed as is");
                        return c.matrix;
                    }
                }
                panic!("{name} not found at frame {}", f + 1);
            })
            .collect()
    };
    let ours = movie(
        &publish_fixture("masks", BuildMode::Release)
            .swf
            .expect("swf"),
    );
    let o = track(&ours, "tweenmask_mc");
    let e = track(&movie(&flash8("masks")), "tweenmask_mc");
    assert_eq!(o.len(), 20);
    for (f, (a, b)) in o.iter().zip(&e).enumerate() {
        assert!(matrix_close(a, b, 1), "frame {}: {a:?} vs {b:?}", f + 1);
    }
}

/// Flash 8 places instance masks directly, with their names, so
/// ActionScript reaches them.
#[test]
fn instance_masks_are_placed_directly_with_their_names() {
    let ours = movie(
        &publish_fixture("masks", BuildMode::Release)
            .swf
            .expect("swf"),
    );
    let theirs = movie(&flash8("masks"));
    let masks = |m: &Movie| -> BTreeMap<String, Matrix> {
        m.root[0]
            .values()
            .filter(|p| p.clip_depth.is_some())
            .filter_map(|p| Some((p.name.clone()?, p.matrix)))
            .collect()
    };
    let o = masks(&ours);
    assert_eq!(
        o.keys().collect::<Vec<_>>(),
        ["h_disc1_mc", "maskclip_mc", "tweenmask_mc"]
    );
    assert_eq!(o, masks(&theirs));
}
