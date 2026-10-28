//! Static text: layout, fonts, records, bounds and diagnostics.

use std::collections::BTreeSet;

use jazari_as2::Origin;
use jazari_text::{FaceMatch, FontId, TextBlock, TextLayout};
use jazari_xfl::model::{Rgba, TextElement};
use swf::{CharacterId, CsmTextSettings, Matrix, Rectangle, Tag, Text, TextGridFit, Twips};

use super::font::{self, EM};
use super::records::{self, Placed};
use super::{clip, define_shared, scan};
use crate::context::{CharKind, Ctx};
use crate::tags;
use crate::units;

/// Defines the DefineText (or DefineText2) of a static text element and
/// its fonts. `None` for empty text, or when the text cannot be written
/// (reported).
pub fn define(ctx: &mut Ctx<'_>, el: &TextElement, origin: &Origin) -> Option<CharacterId> {
    scan::ensure(ctx);
    if el
        .runs
        .iter()
        .any(|r| r.attrs.url.as_deref().is_some_and(|u| !u.is_empty()))
    {
        ctx.warn(
            origin.clone(),
            "links on static text are not published (the text is published without them)",
        );
    }
    let block = TextBlock::from_element(el);
    let layout = scan::layout(ctx, &block);
    layout.glyphs().next()?;
    if el.selectable {
        ctx.info(
            origin.clone(),
            "selectable static text is published as glyphs and cannot be selected \
             (Flash 8 makes it a text field, which cannot shape Arabic)",
        );
    }
    report_fonts(ctx, &block, &layout, origin);

    // Glyphs the scan did not see (none, normally) share one new font.
    for g in layout.glyphs() {
        ctx.text.fonts.note(g.font, g.glyph);
    }
    let mut placed = Vec::new();
    let mut deps: Vec<CharacterId> = Vec::new();
    let mut ink = Ink::default();
    let mut alpha = false;
    for (line, l) in layout.lines.iter().enumerate() {
        for g in &l.glyphs {
            let slot = font::glyph_slot(ctx, g.font, g.glyph, origin)?;
            if !deps.contains(&slot.font) {
                deps.push(slot.font);
            }
            let fill = block.runs.get(g.run).map_or(Rgba::BLACK, |r| r.attrs.fill);
            alpha |= fill.a != 255;
            if let Some(b) = slot.bounds {
                ink.add(g.x, g.y, g.size, &b);
            }
            placed.push(Placed {
                font: slot.font,
                index: slot.index,
                x: g.x,
                y: g.y,
                advance: g.advance,
                height: g.size,
                color: units::color(fill),
                line,
            });
        }
    }
    let records = match records::build(&placed) {
        Ok(r) => r,
        Err(e) => {
            ctx.error(origin.clone(), format!("text left out: {e}"));
            return None;
        }
    };
    let (tx, ty) = (units::px_to_twips(el.left), units::px_to_twips(el.top));
    let text = Text {
        id: 0,
        bounds: ink.rect(tx, ty),
        matrix: Matrix {
            tx,
            ty,
            ..Matrix::IDENTITY
        },
        records,
    };
    let tag = if alpha {
        Tag::DefineText2(Box::new(text))
    } else {
        Tag::DefineText(Box::new(text))
    };
    let csm = Tag::CsmTextSettings(CsmTextSettings {
        id: 0,
        use_advanced_rendering: true,
        grid_fit: TextGridFit::SubPixel,
        thickness: 0.0,
        sharpness: 0.0,
    });
    let encoded = tags::encode(&tag).and_then(|t| Ok(vec![t, tags::encode(&csm)?]));
    match encoded {
        Ok(tags) => define_shared(ctx, CharKind::Text, tags, deps, origin),
        Err(e) => {
            ctx.error(origin.clone(), format!("text left out: {e}"));
            None
        }
    }
}

/// The union of the glyphs' ink, in the text's twips.
#[derive(Debug, Default)]
struct Ink {
    b: Option<(f64, f64, f64, f64)>,
}

impl Ink {
    /// Adds a glyph drawn at `(x, y)` with em size `size` (twips) whose
    /// bounds are in font units.
    fn add(&mut self, x: f64, y: f64, size: f64, b: &Rectangle<Twips>) {
        let k = size / EM;
        let (x0, x1) = (
            x + f64::from(b.x_min.get()) * k,
            x + f64::from(b.x_max.get()) * k,
        );
        let (y0, y1) = (
            y + f64::from(b.y_min.get()) * k,
            y + f64::from(b.y_max.get()) * k,
        );
        let r = self.b.get_or_insert((x0, x1, y0, y1));
        r.0 = r.0.min(x0);
        r.1 = r.1.max(x1);
        r.2 = r.2.min(y0);
        r.3 = r.3.max(y1);
    }

    /// The bounds after the DefineText matrix's translation, rounded
    /// outwards; an empty rectangle at the origin when nothing has ink.
    fn rect(&self, tx: Twips, ty: Twips) -> Rectangle<Twips> {
        let (x0, x1, y0, y1) = self.b.unwrap_or((0.0, 0.0, 0.0, 0.0));
        let t = |v: f64, off: Twips| {
            Twips::new(units::round_clamped(
                v + f64::from(off.get()),
                units::MAX_TWIPS,
            ))
        };
        Rectangle {
            x_min: t(x0.floor(), tx),
            x_max: t(x1.ceil(), tx),
            y_min: t(y0.floor(), ty),
            y_max: t(y1.ceil(), ty),
        }
    }
}

/// Reports font problems of one text: faces replaced by a fallback,
/// characters drawn by another face or by none, and licences that forbid
/// embedding.
fn report_fonts(ctx: &mut Ctx<'_>, block: &TextBlock, layout: &TextLayout, origin: &Origin) {
    let text = block.text();
    let lib = ctx.fonts;
    let family = |f: FontId| {
        lib.face_info(f)
            .map_or_else(|| format!("font {}", f.0), |i| format!("'{}'", i.family))
    };

    // Faces that are not installed, with the faces that drew their text
    // instead (Latin and Arabic may fall back to different fonts).
    let mut faces: Vec<&str> = Vec::new();
    for r in &block.runs {
        if !faces.contains(&r.attrs.face.as_str()) {
            faces.push(&r.attrs.face);
        }
    }
    for face in &faces {
        let res = lib.resolve_detailed(face);
        if res.matched != FaceMatch::Fallback {
            continue;
        }
        let mut drawn: Vec<FontId> = Vec::new();
        for g in layout.glyphs() {
            let ours = block.runs.get(g.run).is_some_and(|r| r.attrs.face == *face);
            if ours && g.glyph != 0 && !drawn.contains(&g.font) {
                drawn.push(g.font);
            }
        }
        if drawn.is_empty() {
            drawn.push(res.font);
        }
        let names: Vec<String> = drawn.iter().map(|&f| family(f)).collect();
        let what = if face.is_empty() {
            "the text names no font".to_owned()
        } else {
            format!("font '{}' is not installed", clip(face, 40))
        };
        let verb = if names.len() == 1 { "stands" } else { "stand" };
        ctx.warn(
            origin.clone(),
            format!(
                "{what}; {} {verb} in (put the font in a 'fonts' folder next to the document)",
                names.join(" and ")
            ),
        );
    }

    // Characters drawn by another face, or by none (.notdef).
    let mut fallback: Vec<(String, FontId, String)> = Vec::new();
    let mut missing = String::new();
    let mut used: BTreeSet<FontId> = BTreeSet::new();
    for g in layout.glyphs() {
        used.insert(g.font);
        let c = text.get(g.cluster..).and_then(|s| s.chars().next());
        if g.glyph == 0 {
            if let Some(c) = c.filter(|c| !missing.contains(*c)) {
                missing.push(c);
            }
            continue;
        }
        let Some(run) = block.runs.get(g.run) else {
            continue;
        };
        let res = lib.resolve_detailed(&run.attrs.face);
        if res.matched == FaceMatch::Fallback || res.font == g.font {
            continue;
        }
        let Some(c) = c.filter(|c| !c.is_whitespace()) else {
            continue;
        };
        match fallback
            .iter_mut()
            .find(|(face, f, _)| *face == run.attrs.face && *f == g.font)
        {
            Some((_, _, chars)) => {
                if !chars.contains(c) {
                    chars.push(c);
                }
            }
            None => fallback.push((run.attrs.face.clone(), g.font, c.to_string())),
        }
    }
    for (face, f, chars) in fallback {
        ctx.warn(
            origin.clone(),
            format!(
                "font '{}' cannot draw {}; {} draws them",
                clip(&face, 40),
                spaced(&chars),
                family(f)
            ),
        );
    }
    if !missing.is_empty() {
        ctx.warn(
            origin.clone(),
            format!(
                "no available font has {}; they show as the missing-glyph box (.notdef)",
                spaced(&missing)
            ),
        );
    }

    // Licences.
    for f in used {
        let Some(info) = lib.face_info(f) else {
            continue;
        };
        if !info.embedding_allowed() {
            ctx.warn(
                origin.clone(),
                format!(
                    "the licence of '{}' (OS/2 fsType 0x{:04X}) does not allow embedding its outlines; they are embedded anyway",
                    info.family, info.fs_type
                ),
            );
        } else if !info.subsetting_allowed() {
            ctx.info(
                origin.clone(),
                format!(
                    "the licence of '{}' asks not to embed a subset of its glyphs; only the glyphs used are embedded",
                    info.family
                ),
            );
        }
    }
}

/// Characters for a message, quoted and separated by spaces (so a list of
/// Latin letters does not read as a word, and Arabic letters show alone).
fn spaced(chars: &str) -> String {
    let list: Vec<String> = clip(chars, 24).chars().map(String::from).collect();
    format!("\"{}\"", list.join(" "))
}
