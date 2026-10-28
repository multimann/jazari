//! Fonts: DefineFont3 for static text (glyph outlines), and glyph-less
//! DefineFont3 for the device fonts of dynamic and input text.
//!
//! # Embedded fonts
//!
//! Glyphs are noted per face ([`Fonts::note`]), first by the document scan
//! and then by each text, and gathered into fonts ("chunks") when a text
//! first needs them. A chunk is a DefineFont3 of at most [`FONT_BUDGET`]
//! bytes, because the `swf` writer silently corrupts larger ones (it never
//! sets `HAS_WIDE_OFFSETS`; research `text-arabic.md`, F11). Glyphs are
//! assigned to chunks in first-use order, so the glyphs of one word tend to
//! share a font, and each chunk lists its glyphs by code, ascending.
//!
//! Each glyph is the face's outline in 20480 units per em, y down, rounded
//! to whole units; its first contour sets fill style 1, as Flash writes it.
//! Flags are `HAS_WIDE_CODES | HAS_LAYOUT` plus bold and italic, never
//! `IS_ANSI`; the language is Latin and the name is the family name with a
//! terminating NUL, as Flash 8 writes them.

use std::collections::HashMap;

use indexmap::{IndexMap, IndexSet};
use jazari_as2::Origin;
use jazari_text::{FontId, FontLibrary};
use jazari_xfl::model::{PathCmd, Pt};
use swf::{
    CharacterId, Font, FontFlag, FontLayout, Glyph, Language, Point, PointDelta, Rectangle,
    ShapeRecord, StyleChangeData, SwfStr, Tag, Twips,
};

use crate::context::{CharKind, Ctx};
use crate::error::PublishError;
use crate::tags::{self, RawTag};
use crate::units::round_clamped;

/// DefineFont3 units per em.
pub const EM: f64 = 20480.0;

/// Largest DefineFont3 body we write, in bytes (the writer's 16-bit offsets
/// break at 64 KiB).
pub const FONT_BUDGET: usize = 60_000;

/// Whether embedded fonts carry the layout table (ascent, descent, leading,
/// advances and bounds). The architecture asks for it. Flash 8 itself
/// leaves it out of fonts used only by static text (the `text` fixture's
/// fonts have flags `HAS_WIDE_CODES` alone); static text never reads it.
pub const FONT_HAS_LAYOUT: bool = true;

/// Coordinates of glyph outlines are kept within this many units.
const MAX_UNITS: f64 = 1_000_000.0;

/// The embedded fonts of one publish.
#[derive(Debug, Default)]
pub struct Fonts {
    faces: IndexMap<FontId, Face>,
}

/// One face's glyphs and chunks.
#[derive(Debug, Default)]
struct Face {
    /// Glyphs noted but not yet in a chunk, in first-use order.
    pending: IndexSet<u16>,
    /// Glyph id to (chunk, index in the chunk).
    slots: HashMap<u16, (usize, u16)>,
    chunks: Vec<Chunk>,
}

/// One DefineFont3 of a face.
#[derive(Debug)]
struct Chunk {
    /// Glyphs ordered by code.
    glyphs: Vec<Glyph>,
    /// The character, once defined (`Some(None)`: definition failed).
    id: Option<Option<CharacterId>>,
}

impl Fonts {
    /// Notes that a text uses a glyph of a face.
    pub fn note(&mut self, font: FontId, glyph: u16) {
        let face = self.faces.entry(font).or_default();
        if !face.slots.contains_key(&glyph) {
            face.pending.insert(glyph);
        }
    }
}

/// Where a glyph is drawn from: the font character, the glyph's index in
/// it, and the glyph's ink bounds in font units (20480 per em).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    /// The DefineFont3 character.
    pub font: CharacterId,
    /// The glyph's index in that font.
    pub index: u16,
    /// Ink bounds, or `None` for an empty glyph (a space).
    pub bounds: Option<Rectangle<Twips>>,
}

/// The font character and glyph index that draw `glyph` of `font`, defining
/// the font on first use. `None` when the font could not be defined (the
/// error is reported at `origin`).
pub fn glyph_slot(ctx: &mut Ctx<'_>, font: FontId, glyph: u16, origin: &Origin) -> Option<Slot> {
    ctx.text.fonts.note(font, glyph);
    seal(ctx.fonts, &mut ctx.text.fonts, font);
    let face = ctx.text.fonts.faces.get(&font)?;
    let (chunk, index) = *face.slots.get(&glyph)?;
    let data = face.chunks.get(chunk)?.glyphs.get(usize::from(index))?;
    let bounds = (!data.shape_records.is_empty())
        .then_some(data.bounds)
        .flatten();
    let state = face.chunks.get(chunk)?.id;
    let id = match state {
        Some(Some(id)) => Some(id),
        Some(None) => {
            ctx.error(
                origin.clone(),
                "text left out: its font could not be written (see the first error about it)",
            );
            None
        }
        None => {
            let glyphs = face.chunks.get(chunk)?.glyphs.clone();
            let lib = ctx.fonts;
            let id = ctx.define(CharKind::Font, Vec::new(), origin, |id| {
                encode_font(lib, font, id, glyphs)
            });
            if let Some(c) = ctx
                .text
                .fonts
                .faces
                .get_mut(&font)
                .and_then(|f| f.chunks.get_mut(chunk))
            {
                c.id = Some(id);
            }
            id
        }
    };
    Some(Slot {
        font: id?,
        index,
        bounds,
    })
}

/// Gathers a face's pending glyphs into new chunks.
fn seal(lib: &FontLibrary, fonts: &mut Fonts, font: FontId) {
    let Some(face) = fonts.faces.get_mut(&font) else {
        return;
    };
    if face.pending.is_empty() {
        return;
    }
    let pending: Vec<u16> = face.pending.drain(..).collect();
    let empty = encoded_len(lib, font, Vec::new()).unwrap_or(0);
    let mut current: Vec<(Glyph, u16)> = Vec::new();
    let mut size = empty;
    let mut groups: Vec<Vec<(Glyph, u16)>> = Vec::new();
    for g in pending {
        let data = glyph_data(lib, font, g);
        let cost = encoded_len(lib, font, vec![data.clone()])
            .map_or(FONT_BUDGET, |n| n.saturating_sub(empty));
        if !current.is_empty() && size + cost > FONT_BUDGET {
            groups.push(std::mem::take(&mut current));
            size = empty;
        }
        size += cost;
        current.push((data, g));
    }
    if !current.is_empty() {
        groups.push(current);
    }
    for mut group in groups {
        group.sort_by_key(|(data, g)| (data.code, *g));
        let chunk = face.chunks.len();
        for (i, (_, g)) in group.iter().enumerate() {
            face.slots
                .insert(*g, (chunk, u16::try_from(i).unwrap_or(u16::MAX)));
        }
        face.chunks.push(Chunk {
            glyphs: group.into_iter().map(|(data, _)| data).collect(),
            id: None,
        });
    }
}

/// The body length of a DefineFont3 of these glyphs, if it encodes.
fn encoded_len(lib: &FontLibrary, font: FontId, glyphs: Vec<Glyph>) -> Option<usize> {
    encode_font(lib, font, 0, glyphs)
        .ok()
        .and_then(|t| t.into_iter().next())
        .map(|t| t.body.len())
}

/// Encodes one DefineFont3 of a face.
fn encode_font(
    lib: &FontLibrary,
    font: FontId,
    id: CharacterId,
    glyphs: Vec<Glyph>,
) -> Result<Vec<RawTag>, PublishError> {
    let info = lib.face_info(font);
    let name = info.as_ref().map_or("Unknown", |i| i.family.as_str());
    let mut flags = FontFlag::HAS_WIDE_CODES;
    if info.as_ref().is_some_and(|i| i.bold) {
        flags |= FontFlag::IS_BOLD;
    }
    if info.as_ref().is_some_and(|i| i.italic) {
        flags |= FontFlag::IS_ITALIC;
    }
    let layout = FONT_HAS_LAYOUT.then(|| {
        let m = lib.metrics(font);
        FontLayout {
            ascent: em_u16(m.ascent),
            descent: em_u16(m.descent),
            leading: round_clamped(m.leading * EM, f64::from(i16::MAX)) as i16,
            kerning: Vec::new(),
        }
    });
    if layout.is_some() {
        flags |= FontFlag::HAS_LAYOUT;
    }
    encode_font_tag(id, name, flags, layout, glyphs).map(|t| vec![t])
}

/// Encodes a DefineFont3 with a NUL-terminated name.
fn encode_font_tag(
    id: CharacterId,
    name: &str,
    flags: FontFlag,
    layout: Option<FontLayout>,
    glyphs: Vec<Glyph>,
) -> Result<RawTag, PublishError> {
    let name = font_name(name);
    let f = Font {
        version: 3,
        id,
        name: SwfStr::from_bytes(name.as_bytes()),
        language: Language::Latin,
        layout,
        glyphs,
        flags,
    };
    let mut raw = tags::encode(&Tag::DefineFont2(Box::new(f)))?;
    nul_terminate_name(&mut raw);
    Ok(raw)
}

/// A font name that fits the tag: no NUL, at most 254 bytes (one is left
/// for the terminator), cut at a character boundary.
fn font_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().filter(|&c| c != '\0') {
        if out.len() + c.len_utf8() > 254 {
            break;
        }
        out.push(c);
    }
    out
}

/// Appends the NUL Flash 8 writes after a font name. The `swf` writer
/// cannot write it (its reader strips it, so the round trip would differ);
/// the offset and code tables count from after the name, so inserting a
/// byte there changes nothing else.
fn nul_terminate_name(raw: &mut RawTag) {
    let Some(&len) = raw.body.get(4) else {
        return;
    };
    let end = 5 + usize::from(len);
    if len < 255 && raw.body.len() >= end {
        raw.body[4] = len + 1;
        raw.body.insert(end, 0);
    }
}

/// The glyph-less DefineFont3 that names a device font for dynamic and
/// input text, shared by every field with the same name and style.
pub fn device_font(
    ctx: &mut Ctx<'_>,
    name: &str,
    bold: bool,
    italic: bool,
    origin: &Origin,
) -> Option<CharacterId> {
    let key = (name.to_owned(), bold, italic);
    if let Some(&id) = ctx.text.device_fonts.get(&key) {
        return id;
    }
    let mut flags = FontFlag::HAS_WIDE_CODES;
    if bold {
        flags |= FontFlag::IS_BOLD;
    }
    if italic {
        flags |= FontFlag::IS_ITALIC;
    }
    let id = ctx.define(CharKind::Font, Vec::new(), origin, |id| {
        encode_font_tag(id, name, flags, None, Vec::new()).map(|t| vec![t])
    });
    ctx.text.device_fonts.insert(key, id);
    id
}

/// A glyph of a face as DefineFont3 data: outline, code, advance and
/// bounds.
fn glyph_data(lib: &FontLibrary, font: FontId, glyph: u16) -> Glyph {
    let upem = f64::from(lib.metrics(font).units_per_em);
    let (shape_records, bounds) = outline_records(&lib.glyph_outline(font, glyph), upem);
    Glyph {
        shape_records,
        code: lib.glyph_code(font, glyph),
        advance: em_u16(lib.glyph_advance(font, glyph)),
        bounds: Some(bounds),
    }
}

/// Ems to DefineFont3 units, as an unsigned 16-bit value.
fn em_u16(v: f64) -> u16 {
    round_clamped(v * EM, f64::from(u16::MAX)).clamp(0, i32::from(u16::MAX)) as u16
}

/// One outline point (ems, y down) in DefineFont3 units, on the face's own
/// design grid when `upem` is known.
///
/// Flash 8 keeps glyphs on the design grid: Arial and Times New Roman
/// (2048 units per em) come out in multiples of 10 units of 20480. TrueType
/// implied on-curve points (the midpoint of two control points) can fall
/// on half a design unit; Flash rounds them up in its y-down space (the
/// `text` fixture), which is `floor(u + 0.5)`.
fn units(p: Pt, upem: f64) -> (i32, i32) {
    let snap = |v: f64| {
        if (16.0..=16384.0).contains(&upem) {
            // The epsilon absorbs the error of em = units / upem.
            (v * upem + 0.5 + 1e-6).floor() * EM / upem
        } else {
            v * EM
        }
    };
    (
        round_clamped(snap(p.x), MAX_UNITS),
        round_clamped(snap(p.y), MAX_UNITS),
    )
}

/// Shape records of an outline (ems, y down) and their exact bounds. `upem`
/// is the face's design units per em (see [`units`]; 0 when unknown).
///
/// A contour that ends with a curve back to its start point gets a
/// zero-length closing line, as Flash 8 writes it (it closes every contour
/// with a line, which has no length when the last curve already closed it).
/// With that, TrueType glyphs come out exactly as Flash 8 wrote Arial and
/// Times New Roman in the `text` fixture.
pub fn outline_records(cmds: &[PathCmd], upem: f64) -> (Vec<ShapeRecord>, Rectangle<Twips>) {
    let mut recs = Vec::new();
    let mut bounds = Bounds::default();
    let mut cur = (0, 0);
    let mut start: Option<(i32, i32)> = None;
    let mut first_fill = true;
    let close = |recs: &mut Vec<ShapeRecord>, cur: (i32, i32), start: Option<(i32, i32)>| {
        if start == Some(cur) && matches!(recs.last(), Some(ShapeRecord::CurvedEdge { .. })) {
            recs.push(ShapeRecord::StraightEdge {
                delta: delta(cur, cur),
            });
        }
    };
    let mut move_to = |recs: &mut Vec<ShapeRecord>, p: (i32, i32)| {
        recs.push(ShapeRecord::StyleChange(Box::new(StyleChangeData {
            move_to: Some(Point::new(Twips::new(p.0), Twips::new(p.1))),
            fill_style_0: first_fill.then_some(1),
            fill_style_1: None,
            line_style: None,
            new_styles: None,
        })));
        first_fill = false;
    };
    for cmd in cmds {
        match *cmd {
            PathCmd::MoveTo(p) => {
                close(&mut recs, cur, start);
                cur = units(p, upem);
                move_to(&mut recs, cur);
                start = Some(cur);
                bounds.add(cur);
            }
            PathCmd::LineTo(p) | PathCmd::QuadTo(_, p) => {
                if start.is_none() {
                    move_to(&mut recs, cur);
                    start = Some(cur);
                    bounds.add(cur);
                }
                let to = units(p, upem);
                let control = match *cmd {
                    PathCmd::QuadTo(c, _) => Some(units(c, upem)),
                    _ => None,
                };
                match control {
                    Some(c) if c != cur && c != to => {
                        recs.push(ShapeRecord::CurvedEdge {
                            control_delta: delta(cur, c),
                            anchor_delta: delta(c, to),
                        });
                        bounds.add_quad(cur, c, to);
                        cur = to;
                    }
                    _ if to != cur => {
                        recs.push(ShapeRecord::StraightEdge {
                            delta: delta(cur, to),
                        });
                        bounds.add(to);
                        cur = to;
                    }
                    _ => {}
                }
            }
        }
    }
    close(&mut recs, cur, start);
    (recs, bounds.rect())
}

fn delta(from: (i32, i32), to: (i32, i32)) -> PointDelta<Twips> {
    PointDelta::new(Twips::new(to.0 - from.0), Twips::new(to.1 - from.1))
}

/// Exact bounds of lines and quadratic curves.
#[derive(Debug, Default)]
struct Bounds {
    b: Option<(f64, f64, f64, f64)>,
}

impl Bounds {
    fn add_f(&mut self, x: f64, y: f64) {
        let b = self.b.get_or_insert((x, x, y, y));
        b.0 = b.0.min(x);
        b.1 = b.1.max(x);
        b.2 = b.2.min(y);
        b.3 = b.3.max(y);
    }

    fn add(&mut self, p: (i32, i32)) {
        self.add_f(f64::from(p.0), f64::from(p.1));
    }

    /// Adds a quadratic curve whose start point is already in.
    fn add_quad(&mut self, p0: (i32, i32), c: (i32, i32), p1: (i32, i32)) {
        self.add(p1);
        // The extremum of each coordinate, where the derivative is zero
        // inside the curve. The start and end points keep the other
        // coordinate inside the box.
        let extremum = |a: i32, b: i32, d: i32| -> Option<f64> {
            let (a, b, d) = (f64::from(a), f64::from(b), f64::from(d));
            let den = a - 2.0 * b + d;
            let t = (a - b) / den;
            (den != 0.0 && t > 0.0 && t < 1.0)
                .then_some((1.0 - t) * (1.0 - t) * a + 2.0 * t * (1.0 - t) * b + t * t * d)
        };
        if let Some(x) = extremum(p0.0, c.0, p1.0) {
            self.add_f(x, f64::from(p1.1));
        }
        if let Some(y) = extremum(p0.1, c.1, p1.1) {
            self.add_f(f64::from(p1.0), y);
        }
    }

    fn rect(&self) -> Rectangle<Twips> {
        match self.b {
            Some((x0, x1, y0, y1)) => Rectangle {
                x_min: Twips::new(x0.floor() as i32),
                x_max: Twips::new(x1.ceil() as i32),
                y_min: Twips::new(y0.floor() as i32),
                y_max: Twips::new(y1.ceil() as i32),
            },
            None => Rectangle::ZERO,
        }
    }
}
