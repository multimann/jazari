//! Bitmaps: one DefineBitsLossless2, DefineBitsJPEG2 or DefineBitsJPEG3 per
//! library bitmap, bitmap fills in shapes, and bitmap instances.
//!
//! Calibrated on the `bitmaps` fixture (`tests/fixtures/MANIFEST.md`):
//!
//! - **Lossless data** becomes DefineBitsLossless2, format 5: the pixels as
//!   premultiplied A, R, G, B, zlib-compressed; an opaque bitmap becomes
//!   DefineBitsLossless (version 1) with the same plane, as a Flash 8 probe
//!   publishes it (its reserved byte is 255). `.dat` planes arrive already
//!   premultiplied exactly as Flash stores them (alpha `+1` quirk included)
//!   and are only reordered; PNG and palette data arrive straight and are
//!   premultiplied with `round(c × a / 255)`, which reproduces Flash 8's
//!   plane byte for byte from the fixture's `LIBRARY` PNG.
//! - **JPEG data** is embedded unchanged as DefineBitsJPEG2, or as
//!   DefineBitsJPEG3 with a zlib alpha plane when the item has alpha.
//! - **Compression settings are not re-applied**: Flash 8 re-encodes a
//!   lossless item set to "photo" compression as DefineBitsJPEG3 (JPEG plus
//!   alpha) and decodes a JPEG item set to "lossless" into
//!   DefineBitsLossless (Flash 8 probe), but the model carries neither the
//!   setting nor a JPEG encoder, so the stored data is published as it is:
//!   the same picture, without JPEG artefacts.
//! - **Smoothing**: `allowSmoothing` chooses the smoothed fill types
//!   (0x40/0x41) or the non-smoothed ones (0x42/0x43) Flash 8 writes when
//!   smoothing is off.
//! - **Fills**: `bitmapIsClipped` chooses clipped or repeating; the model's
//!   fill matrix (`a = d = 20` for one bitmap pixel per pixel, `tx`/`ty` in
//!   pixels) converts with [`units::matrix`].
//! - **Instances** are bitmap-filled rectangles merged into the surrounding
//!   shape, as Flash 8 publishes them: the rectangle is baked into the
//!   parent's space (its corners transformed by the instance matrix) and the
//!   clipped fill's matrix is the instance matrix scaled by 20, so the shape
//!   is placed with the identity matrix like the raw shapes beside it.
//!   Flash also writes an unused placeholder fill (bitmap id 65535) in each
//!   instance's style block; it draws nothing and is not reproduced.
//! - Each library bitmap is defined once (deduplicated by name) and written
//!   just before the first shape that uses it. A bitmap exported for
//!   ActionScript gets an ExportAssets tag after its definition.

use std::collections::HashMap;
use std::io::Write as _;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use jazari_as2::Origin;
use jazari_xfl::model::{
    BitmapData, BitmapFill, BitmapInstance, BitmapItem, Edge, FillStyle, Matrix, PathCmd, Pt, Shape,
};
use swf::{BitmapFormat, CharacterId, DefineBitsJpeg3, DefineBitsLossless, ExportedAsset, Tag};

use crate::context::{CharKind, Ctx};
use crate::error::PublishError;
use crate::tags::{self, RawTag};
use crate::units;

/// Per-publish state of the bitmap module.
#[derive(Debug, Default)]
pub struct BitmapState {
    /// Library name to character (`None`: it failed and was reported).
    defined: HashMap<String, Option<CharacterId>>,
}

/// The SWF fill for a bitmap fill, or `None` (reported) when the bitmap is
/// not published; the shape then draws the fill transparent.
pub fn fill_style(ctx: &mut Ctx<'_>, fill: &BitmapFill, origin: &Origin) -> Option<swf::FillStyle> {
    let id = define_bitmap(ctx, &fill.library_item, origin)?;
    let smoothed = ctx
        .doc
        .library
        .bitmaps
        .get(&fill.library_item)
        .is_some_and(|b| b.smoothing);
    let matrix = if fill.matrix.is_finite() {
        fill.matrix
    } else {
        ctx.warn(
            origin.clone(),
            format!(
                "the fill matrix of bitmap '{}' is not a number; one bitmap pixel per pixel is used",
                fill.library_item
            ),
        );
        Matrix::scale(20.0, 20.0)
    };
    Some(swf::FillStyle::Bitmap {
        id,
        matrix: units::matrix(&matrix),
        is_smoothed: smoothed,
        is_repeating: !fill.clipped,
    })
}

/// The shape that draws a bitmap instance: a rectangle the size of the
/// bitmap, baked into the parent's space, with a clipped bitmap fill. `None`
/// (reported) when the bitmap cannot be published.
pub fn instance_shape(ctx: &mut Ctx<'_>, inst: &BitmapInstance, origin: &Origin) -> Option<Shape> {
    let m = inst.matrix;
    if !m.is_finite() {
        ctx.error(
            origin.clone(),
            "the bitmap instance's matrix is not a number; it is left out",
        );
        return None;
    }
    define_bitmap(ctx, &inst.library_item, origin)?;
    let doc = ctx.doc;
    let item = doc.library.bitmaps.get(&inst.library_item)?;
    Some(instance_rect(item, &inst.library_item, &m))
}

/// The baked rectangle of a bitmap placed with `m` (see the module docs).
fn instance_rect(item: &BitmapItem, library_item: &str, m: &Matrix) -> Shape {
    let (w, h) = (f64::from(item.width) * 20.0, f64::from(item.height) * 20.0);
    // Edges are in twips; the matrix translation is in pixels.
    let to_parent = Matrix {
        tx: m.tx * 20.0,
        ty: m.ty * 20.0,
        ..*m
    };
    let corner = |x: f64, y: f64| to_parent.transform_point(Pt::new(x, y));
    // Flash's own order: bottom edge leftwards, then up, right and down.
    let start = corner(w, h);
    let path = vec![
        PathCmd::MoveTo(start),
        PathCmd::LineTo(corner(0.0, h)),
        PathCmd::LineTo(corner(0.0, 0.0)),
        PathCmd::LineTo(corner(w, 0.0)),
        PathCmd::LineTo(start),
    ];
    Shape {
        matrix: Matrix::IDENTITY,
        fills: vec![FillStyle::Bitmap(BitmapFill {
            library_item: library_item.to_owned(),
            matrix: Matrix {
                a: m.a * 20.0,
                b: m.b * 20.0,
                c: m.c * 20.0,
                d: m.d * 20.0,
                tx: m.tx,
                ty: m.ty,
            },
            clipped: true,
        })],
        strokes: Vec::new(),
        edges: vec![Edge {
            fill0: 0,
            fill1: 1,
            stroke: 0,
            path,
        }],
        is_drawing_object: true,
    }
}

/// The bitmap character for a library bitmap, defined on first use, or
/// `None` (reported once) when it cannot be published.
pub fn define_bitmap(
    ctx: &mut Ctx<'_>,
    library_item: &str,
    origin: &Origin,
) -> Option<CharacterId> {
    if let Some(known) = ctx.bitmap.defined.get(library_item) {
        return *known;
    }
    let id = define_new(ctx, library_item, origin);
    ctx.bitmap.defined.insert(library_item.to_owned(), id);
    id
}

/// Defines a bitmap not seen before.
fn define_new(ctx: &mut Ctx<'_>, name: &str, origin: &Origin) -> Option<CharacterId> {
    let doc = ctx.doc;
    let Some(item) = doc.library.bitmaps.get(name) else {
        ctx.error(
            origin.clone(),
            format!("bitmap '{name}' is not in the library; it is left out"),
        );
        return None;
    };
    let body = match bitmap_body(item) {
        Ok(body) => body,
        Err(why) => {
            ctx.error(
                origin.clone(),
                format!("bitmap '{name}' is left out: {why}"),
            );
            return None;
        }
    };
    if let BitmapData::Jpeg { alpha: Some(a), .. } = &item.data
        && body.dropped_alpha
    {
        ctx.warn(
            origin.clone(),
            format!(
                "bitmap '{name}': the alpha plane has {} bytes instead of {}; the JPEG is published opaque",
                a.len(),
                body.pixels
            ),
        );
    }
    let export = item
        .linkage
        .as_ref()
        .filter(|l| l.export_for_as && !l.identifier.trim().is_empty())
        .map(|l| l.identifier.clone());
    ctx.define(CharKind::Bitmap, Vec::new(), origin, |id| {
        let mut out = vec![body.encode(id)?];
        if let Some(name) = &export {
            out.push(tags::encode(&Tag::ExportAssets(vec![ExportedAsset {
                id,
                name: name.as_str().into(),
            }]))?);
        }
        Ok(out)
    })
}

/// The contents of a bitmap definition, before its id is known.
#[derive(Debug)]
struct BitmapBody {
    kind: BodyKind,
    /// Number of pixels.
    pixels: usize,
    /// Whether a JPEG's alpha plane was dropped (wrong size).
    dropped_alpha: bool,
}

#[derive(Debug)]
enum BodyKind {
    /// DefineBitsLossless (version 1, opaque) or DefineBitsLossless2
    /// (version 2), format 5: width, height, zlib ARGB plane.
    Lossless {
        version: u8,
        width: u16,
        height: u16,
        zlib: Vec<u8>,
    },
    /// DefineBitsJPEG2.
    Jpeg(Vec<u8>),
    /// DefineBitsJPEG3: JPEG data and the zlib alpha plane.
    JpegAlpha(Vec<u8>, Vec<u8>),
}

impl BitmapBody {
    /// The definition tag with character id `id`.
    fn encode(&self, id: CharacterId) -> Result<RawTag, PublishError> {
        match &self.kind {
            BodyKind::Lossless {
                version,
                width,
                height,
                zlib,
            } => tags::encode(&Tag::DefineBitsLossless(DefineBitsLossless {
                version: *version,
                id,
                format: BitmapFormat::Rgb32,
                width: *width,
                height: *height,
                data: zlib.as_slice().into(),
            })),
            BodyKind::Jpeg(bytes) => tags::encode(&Tag::DefineBitsJpeg2 {
                id,
                jpeg_data: bytes,
            }),
            BodyKind::JpegAlpha(bytes, alpha) => {
                tags::encode(&Tag::DefineBitsJpeg3(DefineBitsJpeg3 {
                    id,
                    version: 3,
                    deblocking: swf::Fixed8::ZERO,
                    data: bytes,
                    alpha_data: alpha,
                }))
            }
        }
    }
}

/// Checks a library bitmap and prepares its definition, or says why it
/// cannot be published.
fn bitmap_body(item: &BitmapItem) -> Result<BitmapBody, String> {
    let (Ok(width), Ok(height)) = (u16::try_from(item.width), u16::try_from(item.height)) else {
        return Err(format!(
            "{} × {} px exceeds the SWF limit of 65535 px per side",
            item.width, item.height
        ));
    };
    if width == 0 || height == 0 {
        return Err("it has no pixels".to_owned());
    }
    let pixels = usize::from(width) * usize::from(height);
    match &item.data {
        BitmapData::Rgba {
            premultiplied,
            pixels: rgba,
        } => {
            if rgba.is_empty() {
                return Err("its pixel data is missing".to_owned());
            }
            if rgba.len() != pixels * 4 {
                return Err(format!(
                    "its pixel data has {} bytes instead of {}",
                    rgba.len(),
                    pixels * 4
                ));
            }
            let plane = argb_plane(rgba, *premultiplied);
            // Flash 8 writes opaque bitmaps as DefineBitsLossless (the
            // reserved byte 255, which our plane already holds).
            let opaque = rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255);
            Ok(BitmapBody {
                kind: BodyKind::Lossless {
                    version: if opaque { 1 } else { 2 },
                    width,
                    height,
                    zlib: zlib(&plane)?,
                },
                pixels,
                dropped_alpha: false,
            })
        }
        BitmapData::Jpeg { bytes, alpha } => {
            if !bytes.starts_with(&[0xFF, 0xD8]) {
                return Err("its JPEG data is missing or damaged".to_owned());
            }
            match alpha {
                Some(a) if a.len() == pixels => Ok(BitmapBody {
                    kind: BodyKind::JpegAlpha(bytes.clone(), zlib(a)?),
                    pixels,
                    dropped_alpha: false,
                }),
                other => Ok(BitmapBody {
                    kind: BodyKind::Jpeg(bytes.clone()),
                    pixels,
                    dropped_alpha: other.is_some(),
                }),
            }
        }
    }
}

/// RGBA pixels to the premultiplied A, R, G, B plane of DefineBitsLossless2
/// format 5. Straight pixels are premultiplied with `round(c × a / 255)`.
pub fn argb_plane(rgba: &[u8], premultiplied: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for &[r, g, b, a] in rgba.as_chunks::<4>().0 {
        if premultiplied || a == 255 {
            out.extend_from_slice(&[a, r, g, b]);
        } else {
            out.extend_from_slice(&[a, premultiply(r, a), premultiply(g, a), premultiply(b, a)]);
        }
    }
    out
}

/// `round(c × a / 255)`, the rule that reproduces Flash 8's plane.
fn premultiply(c: u8, a: u8) -> u8 {
    let v = (u32::from(c) * u32::from(a) + 127) / 255;
    u8::try_from(v).unwrap_or(u8::MAX)
}

/// zlib at the best compression, as Flash writes bitmap data.
fn zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut enc = ZlibEncoder::new(Vec::with_capacity(data.len() / 4), Compression::best());
    enc.write_all(data)
        .and_then(|()| enc.finish())
        .map_err(|e| format!("cannot compress it: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BuildMode;
    use jazari_xfl::model::Linkage;
    use std::io::Read as _;

    fn doc_with(items: Vec<BitmapItem>) -> jazari_xfl::Document {
        let mut doc = jazari_xfl::Document::default();
        for it in items {
            doc.library.bitmaps.insert(it.name.clone(), it);
        }
        doc
    }

    fn rgba_item(name: &str, w: u32, h: u32, premultiplied: bool, px: [u8; 4]) -> BitmapItem {
        BitmapItem {
            name: name.into(),
            width: w,
            height: h,
            data: BitmapData::Rgba {
                premultiplied,
                pixels: px.repeat((w * h) as usize),
            },
            smoothing: false,
            linkage: None,
        }
    }

    fn inflate(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(data)
            .read_to_end(&mut out)
            .expect("zlib");
        out
    }

    fn parse(raw: &RawTag) -> Vec<u8> {
        let mut framed = Vec::new();
        framed.extend_from_slice(&((raw.code << 6) | 0x3f).to_le_bytes());
        framed.extend_from_slice(&(raw.body.len() as u32).to_le_bytes());
        framed.extend_from_slice(&raw.body);
        framed
    }

    #[test]
    fn straight_pixels_are_premultiplied_like_flash() {
        // 50 % white and the fixture's rounding rule.
        assert_eq!(
            argb_plane(&[255, 255, 255, 128], false),
            [128, 128, 128, 128]
        );
        assert_eq!(argb_plane(&[230, 40, 40, 111], false), [111, 100, 17, 17]);
        assert_eq!(argb_plane(&[1, 2, 3, 0], false), [0, 0, 0, 0]);
        // Premultiplied planes are only reordered.
        assert_eq!(argb_plane(&[200, 1, 2, 129], true), [129, 200, 1, 2]);
    }

    #[test]
    fn lossless_bitmaps_become_define_bits_lossless2_once() {
        let doc = doc_with(vec![rgba_item("a.png", 3, 2, false, [255, 0, 0, 128])]);
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let o = Origin::default();
        let id = define_bitmap(&mut ctx, "a.png", &o).expect("defined");
        assert_eq!(define_bitmap(&mut ctx, "a.png", &o), Some(id));
        assert_eq!(ctx.kind_of(id), Some(CharKind::Bitmap));
        let mut out = Vec::new();
        ctx.emit_definition(id, &mut out);
        assert_eq!(out.len(), 1);
        let framed = parse(&out[0]);
        let tag = swf::read::Reader::new(&framed, 8).read_tag().expect("tag");
        let Tag::DefineBitsLossless(b) = tag else {
            panic!("not lossless: {tag:?}");
        };
        assert_eq!((b.version, b.id, b.width, b.height), (2, id, 3, 2));
        assert_eq!(b.format, BitmapFormat::Rgb32);
        assert_eq!(inflate(&b.data), [128, 128, 0, 0].repeat(6));
        assert!(ctx.take_diagnostics().is_empty());
    }

    #[test]
    fn opaque_bitmaps_become_define_bits_lossless_version_1() {
        let doc = doc_with(vec![rgba_item("o.png", 2, 1, false, [10, 20, 30, 255])]);
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let id = define_bitmap(&mut ctx, "o.png", &Origin::default()).expect("defined");
        let mut out = Vec::new();
        ctx.emit_definition(id, &mut out);
        assert_eq!(out[0].code, swf::TagCode::DefineBitsLossless as u16);
        let framed = parse(&out[0]);
        let Tag::DefineBitsLossless(b) =
            swf::read::Reader::new(&framed, 8).read_tag().expect("tag")
        else {
            panic!("not lossless");
        };
        assert_eq!(b.version, 1);
        assert_eq!(inflate(&b.data), [255, 10, 20, 30].repeat(2));
    }

    #[test]
    fn jpegs_pass_through_and_alpha_makes_jpeg3() {
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 0xFF, 0xD9];
        let plain = BitmapItem {
            name: "p.jpg".into(),
            width: 2,
            height: 2,
            data: BitmapData::Jpeg {
                bytes: jpeg.clone(),
                alpha: None,
            },
            ..BitmapItem::default()
        };
        let with_alpha = BitmapItem {
            name: "a.jpg".into(),
            data: BitmapData::Jpeg {
                bytes: jpeg.clone(),
                alpha: Some(vec![0, 64, 128, 255]),
            },
            ..plain.clone()
        };
        let body = bitmap_body(&plain).expect("plain");
        let raw = body.encode(7).expect("encode");
        let framed = parse(&raw);
        match swf::read::Reader::new(&framed, 8).read_tag().expect("tag") {
            Tag::DefineBitsJpeg2 { id, jpeg_data } => {
                assert_eq!(id, 7);
                assert_eq!(jpeg_data, jpeg.as_slice());
            }
            other => panic!("not JPEG2: {other:?}"),
        }
        let body = bitmap_body(&with_alpha).expect("alpha");
        let raw = body.encode(8).expect("encode");
        let framed = parse(&raw);
        match swf::read::Reader::new(&framed, 8).read_tag().expect("tag") {
            Tag::DefineBitsJpeg3(j) => {
                assert_eq!((j.id, j.version), (8, 3));
                assert_eq!(j.data, jpeg.as_slice());
                assert_eq!(inflate(j.alpha_data), [0, 64, 128, 255]);
            }
            other => panic!("not JPEG3: {other:?}"),
        }
    }

    #[test]
    fn broken_bitmaps_are_reported_once_and_left_out() {
        let mut short = rgba_item("short", 4, 4, true, [0, 0, 0, 0]);
        short.data = BitmapData::Rgba {
            premultiplied: true,
            pixels: vec![0; 10],
        };
        let empty = BitmapItem {
            name: "empty".into(),
            width: 0,
            height: 5,
            ..BitmapItem::default()
        };
        let huge = BitmapItem {
            name: "huge".into(),
            width: 70_000,
            height: 1,
            ..BitmapItem::default()
        };
        let bad_jpeg = BitmapItem {
            name: "bad".into(),
            width: 1,
            height: 1,
            data: BitmapData::Jpeg {
                bytes: vec![1, 2, 3],
                alpha: None,
            },
            ..BitmapItem::default()
        };
        let doc = doc_with(vec![short, empty, huge, bad_jpeg]);
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let o = Origin::default();
        for name in ["short", "empty", "huge", "bad", "missing", "short"] {
            assert_eq!(define_bitmap(&mut ctx, name, &o), None, "{name}");
        }
        let diags = ctx.take_diagnostics();
        assert_eq!(diags.len(), 5, "{diags:?}");
        assert!(
            diags
                .iter()
                .all(|d| d.severity == jazari_as2::Severity::Error)
        );
    }

    #[test]
    fn fills_follow_smoothing_and_clipping() {
        let mut item = rgba_item("s.png", 1, 1, true, [0, 0, 0, 255]);
        item.smoothing = true;
        let doc = doc_with(vec![item, rgba_item("n.png", 1, 1, true, [0, 0, 0, 255])]);
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let o = Origin::default();
        let fill = |name: &str, clipped: bool| BitmapFill {
            library_item: name.into(),
            matrix: Matrix::new(20.0, 0.0, 0.0, 20.0, 385.0, 20.0),
            clipped,
        };
        match fill_style(&mut ctx, &fill("s.png", false), &o) {
            Some(swf::FillStyle::Bitmap {
                matrix,
                is_smoothed,
                is_repeating,
                ..
            }) => {
                assert!(is_smoothed && is_repeating);
                assert_eq!(matrix.a.get(), 20 << 16);
                assert_eq!(matrix.tx.get(), 7700);
            }
            other => panic!("{other:?}"),
        }
        match fill_style(&mut ctx, &fill("n.png", true), &o) {
            Some(swf::FillStyle::Bitmap {
                is_smoothed,
                is_repeating,
                ..
            }) => assert!(!is_smoothed && !is_repeating),
            other => panic!("{other:?}"),
        }
        assert!(fill_style(&mut ctx, &fill("nope", true), &o).is_none());
    }

    #[test]
    fn instances_are_baked_clipped_rectangles() {
        let doc = doc_with(vec![rgba_item("b", 128, 64, true, [0, 0, 0, 255])]);
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let inst = BitmapInstance {
            library_item: "b".into(),
            matrix: Matrix::new(0.0, 2.0, -2.0, 0.0, 26.0, 30.0),
        };
        let s = instance_shape(&mut ctx, &inst, &Origin::default()).expect("shape");
        assert_eq!(s.matrix, Matrix::IDENTITY);
        let FillStyle::Bitmap(f) = &s.fills[0] else {
            panic!("not a bitmap fill");
        };
        assert!(f.clipped);
        assert_eq!(f.matrix, Matrix::new(0.0, 40.0, -40.0, 0.0, 26.0, 30.0));
        // The bitmap's (128, 64) px corner lands at (26 - 128, 30 + 256) px.
        assert_eq!(
            s.edges[0].path[0],
            PathCmd::MoveTo(Pt::new((26.0 - 128.0) * 20.0, (30.0 + 256.0) * 20.0))
        );
        assert_eq!(s.edges[0].path.len(), 5);

        let nan = BitmapInstance {
            library_item: "b".into(),
            matrix: Matrix::new(f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0),
        };
        assert!(instance_shape(&mut ctx, &nan, &Origin::default()).is_none());
        let missing = BitmapInstance {
            library_item: "zz".into(),
            matrix: Matrix::IDENTITY,
        };
        assert!(instance_shape(&mut ctx, &missing, &Origin::default()).is_none());
        assert_eq!(ctx.take_diagnostics().len(), 2);
    }

    #[test]
    fn exported_bitmaps_carry_export_assets() {
        let mut item = rgba_item("e.png", 1, 1, true, [0, 0, 0, 255]);
        item.linkage = Some(Linkage {
            identifier: "logo".into(),
            export_for_as: true,
            ..Linkage::default()
        });
        let doc = doc_with(vec![item]);
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let id = define_bitmap(&mut ctx, "e.png", &Origin::default()).expect("id");
        let mut out = Vec::new();
        ctx.emit_definition(id, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].code, swf::TagCode::ExportAssets as u16);
    }
}
