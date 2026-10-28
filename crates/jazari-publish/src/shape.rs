//! Shapes: model [`Shape`]s to DefineShape3/4.
//!
//! Several shapes of one keyframe (a "run" of consecutive shapes with the
//! same matrix) become one character, one style block per shape, back to
//! front: Ruffle draws each block's fills then its strokes, and a new block
//! starts a new layer, so stacking is kept (research, missed point on
//! z-order). Flash 8 merges raw shapes the same way.
//!
//! Rules (`docs/research/swf-writer.md`, F6-F11, F26):
//! - fill and stroke selectors are 1-based positions and map 1:1; out of
//!   range selectors become 0; an edge with the same fill on both sides keeps
//!   only its stroke; edges with nothing to draw are dropped;
//! - every block after the first starts with a StyleChange carrying
//!   `new_styles` with all three selectors cleared, followed by one that sets
//!   them (selectors are written with the previous block's bit widths, so
//!   they cannot share the record); an empty StyleChange (which would be read
//!   as the end of the shape) is never written;
//! - absolute positions are rounded to twips before deltas are taken, and
//!   edges whose deltas exceed 65535 twips are split;
//! - gradients keep at most 15 records;
//! - shape bounds are the edge bounds (curve extrema included), each stroked
//!   edge grown by half its stroke width, as Flash computes them;
//! - DefineShape4 is used when any stroke or focal gradient is present (as
//!   Flash 8 does), DefineShape3 otherwise;
//! - identical shapes are defined once.

use jazari_xfl::model::{
    Caps, Edge, FillStyle, Gradient, Interp, Joints, PathCmd, Pt, ScaleMode, Shape, Spread,
    StrokeStyle,
};
use swf::{
    CharacterId, FillStyle as SwfFill, Fixed8, GradientInterpolation, GradientRecord,
    GradientSpread, LineCapStyle, LineJoinStyle, LineStyle, Point, PointDelta, Rectangle,
    ShapeFlag, ShapeRecord, ShapeStyles, StyleChangeData, Tag, Twips,
};

use jazari_as2::Origin;

use crate::bitmap;
use crate::context::{CharKind, Ctx};
use crate::tags;
use crate::units;

/// The most gradient records SWF 8 can hold (the writer masks the count to
/// 4 bits).
pub const MAX_GRADIENT_RECORDS: usize = 15;

/// The largest edge delta an SWF edge record holds, in twips.
pub const MAX_EDGE_DELTA: i32 = 65535;

/// Defines one shape character from a run of shapes drawn back to front
/// (`shapes[0]` at the bottom). Edges stay in the shapes' local space: the
/// caller places the character with the run's matrix. Returns `None` (after
/// reporting) when nothing drawable is left or the definition fails.
pub fn define_shapes(ctx: &mut Ctx<'_>, shapes: &[&Shape], origin: &Origin) -> Option<CharacterId> {
    let built = build_shape(ctx, shapes, origin)?;
    let mut deps = built.deps;
    deps.sort_unstable();
    deps.dedup();
    let shape = built.shape;
    let raw = match tags::encode(&Tag::DefineShape(Box::new(shape))) {
        Ok(raw) => raw,
        Err(e) => {
            ctx.error(origin.clone(), format!("shape left out: {e}"));
            return None;
        }
    };
    if let Some(&id) = ctx.shape_dedup.get(&raw.body) {
        return Some(id);
    }
    let key = raw.body.clone();
    let id = ctx.reserve_id(origin)?;
    let mut raw = raw;
    raw.set_leading_id(id);
    ctx.set_definition(id, CharKind::Shape, vec![raw], deps);
    ctx.shape_dedup.insert(key, id);
    Some(id)
}

/// A built shape (id 0) and the characters its fills use.
#[derive(Debug)]
pub struct BuiltShape {
    /// The shape, with id 0.
    pub shape: swf::Shape,
    /// Bitmap characters used by fills.
    pub deps: Vec<CharacterId>,
}

/// Builds the SWF shape (with id 0) for a run of shapes, or `None` when
/// nothing drawable remains.
pub fn build_shape(ctx: &mut Ctx<'_>, shapes: &[&Shape], origin: &Origin) -> Option<BuiltShape> {
    let mut b = Builder::default();
    for (i, s) in shapes.iter().enumerate() {
        b.block(ctx, s, i == 0, origin);
    }
    if b.records.is_empty() {
        return None;
    }
    let version = if b.needs_v4 { 4 } else { 3 };
    let edge_bounds = b.edge_bounds.unwrap_or(ZERO_RECT);
    let shape_bounds = b.shape_bounds.unwrap_or(ZERO_RECT);
    let mut flags = ShapeFlag::empty();
    if version == 4 {
        if b.scaling_strokes {
            flags |= ShapeFlag::HAS_SCALING_STROKES;
        }
        if b.non_scaling_strokes {
            flags |= ShapeFlag::HAS_NON_SCALING_STROKES;
        }
    } else {
        // What the reader synthesizes for shapes below version 4.
        flags = ShapeFlag::HAS_NON_SCALING_STROKES;
    }
    Some(BuiltShape {
        shape: swf::Shape {
            version,
            id: 0,
            shape_bounds,
            edge_bounds: if version == 4 {
                edge_bounds
            } else {
                shape_bounds
            },
            flags,
            styles: b.first_styles.unwrap_or(ShapeStyles {
                fill_styles: Vec::new(),
                line_styles: Vec::new(),
            }),
            shape: b.records,
        },
        deps: b.deps,
    })
}

const ZERO_RECT: Rectangle<Twips> = Rectangle {
    x_min: Twips::ZERO,
    x_max: Twips::ZERO,
    y_min: Twips::ZERO,
    y_max: Twips::ZERO,
};

#[derive(Default)]
struct Builder {
    records: Vec<ShapeRecord>,
    first_styles: Option<ShapeStyles>,
    deps: Vec<CharacterId>,
    needs_v4: bool,
    scaling_strokes: bool,
    non_scaling_strokes: bool,
    edge_bounds: Option<Rectangle<Twips>>,
    shape_bounds: Option<Rectangle<Twips>>,
    /// The pen, in absolute twips.
    pen: (i32, i32),
}

/// The selectors of the current style state.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Selectors {
    fill0: u32,
    fill1: u32,
    line: u32,
}

/// Per-block emission state.
struct Block {
    /// `new_styles` still to be written with the first record of the block.
    pending_styles: Option<ShapeStyles>,
    /// Selectors in effect (`None` before the first record of the block).
    current: Option<Selectors>,
    /// Half stroke widths in twips, by 1-based line index.
    half_widths: Vec<f64>,
}

impl Builder {
    fn block(&mut self, ctx: &mut Ctx<'_>, s: &Shape, first: bool, origin: &Origin) {
        let fill_styles: Vec<SwfFill> = s
            .fills
            .iter()
            .map(|f| self.fill_style(ctx, f, origin))
            .collect();
        let line_styles: Vec<LineStyle> = s
            .strokes
            .iter()
            .map(|st| self.line_style(ctx, st, origin))
            .collect();
        let half_widths = s
            .strokes
            .iter()
            .map(|st| {
                if st.hairline {
                    0.0
                } else {
                    stroke_width_twips(st) as f64 / 2.0
                }
            })
            .collect();
        let styles = ShapeStyles {
            fill_styles,
            line_styles,
        };
        let mut block = Block {
            pending_styles: None,
            current: None,
            half_widths,
        };
        if first {
            self.first_styles = Some(styles);
        } else {
            block.pending_styles = Some(styles);
        }
        let (nf, nl) = (s.fills.len(), s.strokes.len());
        for edge in &s.edges {
            let Some(sel) = sanitize(edge, nf, nl) else {
                continue;
            };
            self.edge(edge, sel, &mut block);
        }
    }

    fn edge(&mut self, edge: &Edge, sel: Selectors, block: &mut Block) {
        let mut pending_move: Option<(i32, i32)> = None;
        // The edge's own pen, in exact (unrounded) twips, for subdivision.
        let mut from = Pt::new(f64::from(self.pen.0), f64::from(self.pen.1));
        for cmd in &edge.path {
            match *cmd {
                PathCmd::MoveTo(p) => {
                    let r = round_pt(p);
                    pending_move = Some(r);
                    from = p;
                }
                PathCmd::LineTo(p) => {
                    self.begin_segment(sel, block, &mut pending_move);
                    self.line_to(p, sel, block);
                    from = p;
                }
                PathCmd::QuadTo(c, p) => {
                    self.begin_segment(sel, block, &mut pending_move);
                    self.quad_to(from, c, p, sel, block, 0);
                    from = p;
                }
            }
        }
    }

    /// Writes the StyleChange a segment needs (move, selectors, new styles),
    /// if any.
    fn begin_segment(
        &mut self,
        sel: Selectors,
        block: &mut Block,
        pending_move: &mut Option<(i32, i32)>,
    ) {
        if let Some(styles) = block.pending_styles.take() {
            // Selectors in a record are written with the bit widths of the
            // styles in effect *before* its new_styles, but Ruffle applies
            // them to the new lists. So the new_styles record clears all
            // three selectors (0 fits any width) and the next record sets
            // them with the new widths.
            self.records
                .push(ShapeRecord::StyleChange(Box::new(StyleChangeData {
                    move_to: None,
                    fill_style_0: Some(0),
                    fill_style_1: Some(0),
                    line_style: Some(0),
                    new_styles: Some(styles),
                })));
            block.current = None;
        }
        let move_to = pending_move.take().filter(|m| *m != self.pen);
        let all = block.current.is_none();
        let cur = block.current;
        let differs = |f: fn(&Selectors) -> u32| all || cur.map(|c| f(&c)) != Some(f(&sel));
        let fill_style_0 = differs(|s| s.fill0).then_some(sel.fill0);
        let fill_style_1 = differs(|s| s.fill1).then_some(sel.fill1);
        let line_style = differs(|s| s.line).then_some(sel.line);
        if move_to.is_none()
            && fill_style_0.is_none()
            && fill_style_1.is_none()
            && line_style.is_none()
        {
            return; // nothing changes: never write an empty StyleChange
        }
        if let Some(m) = move_to {
            self.pen = m;
        }
        block.current = Some(sel);
        self.records
            .push(ShapeRecord::StyleChange(Box::new(StyleChangeData {
                move_to: move_to.map(|(x, y)| Point::new(Twips::new(x), Twips::new(y))),
                fill_style_0,
                fill_style_1,
                line_style,
                new_styles: None,
            })));
    }

    fn line_to(&mut self, p: Pt, sel: Selectors, block: &Block) {
        let to = round_pt(p);
        let (x0, y0) = self.pen;
        let (dx, dy) = (
            i64::from(to.0) - i64::from(x0),
            i64::from(to.1) - i64::from(y0),
        );
        if dx == 0 && dy == 0 {
            return;
        }
        let biggest = dx.abs().max(dy.abs());
        let n = (biggest + i64::from(MAX_EDGE_DELTA) - 1) / i64::from(MAX_EDGE_DELTA);
        let mut prev = (i64::from(x0), i64::from(y0));
        for i in 1..=n {
            // Integer division keeps every piece within the limit.
            let px = i64::from(x0) + dx * i / n;
            let py = i64::from(y0) + dy * i / n;
            let delta = PointDelta::new(
                Twips::new(clamp_i32(px - prev.0)),
                Twips::new(clamp_i32(py - prev.1)),
            );
            self.records.push(ShapeRecord::StraightEdge { delta });
            self.grow_line(prev, (px, py), sel, block);
            prev = (px, py);
        }
        self.pen = to;
    }

    fn quad_to(&mut self, from: Pt, c: Pt, p: Pt, sel: Selectors, block: &Block, depth: u32) {
        let (rc, rp) = (round_pt(c), round_pt(p));
        let (x0, y0) = self.pen;
        let cd = (
            i64::from(rc.0) - i64::from(x0),
            i64::from(rc.1) - i64::from(y0),
        );
        let ad = (
            i64::from(rp.0) - i64::from(rc.0),
            i64::from(rp.1) - i64::from(rc.1),
        );
        let limit = i64::from(MAX_EDGE_DELTA);
        let fits = [cd.0, cd.1, ad.0, ad.1].iter().all(|v| v.abs() <= limit);
        if !fits && depth < 40 {
            // de Casteljau at t = 0.5, in exact coordinates.
            let m1 = mid(from, c);
            let m2 = mid(c, p);
            let m = mid(m1, m2);
            self.quad_to(from, m1, m, sel, block, depth + 1);
            self.quad_to(m, m2, p, sel, block, depth + 1);
            return;
        }
        if cd == (0, 0) && ad == (0, 0) {
            return;
        }
        if !fits {
            // Unreachable for coordinates within MAX_TWIPS; fall back to a line.
            self.line_to(p, sel, block);
            return;
        }
        self.records.push(ShapeRecord::CurvedEdge {
            control_delta: PointDelta::new(
                Twips::new(clamp_i32(cd.0)),
                Twips::new(clamp_i32(cd.1)),
            ),
            anchor_delta: PointDelta::new(Twips::new(clamp_i32(ad.0)), Twips::new(clamp_i32(ad.1))),
        });
        let start = (i64::from(x0), i64::from(y0));
        let ctrl = (i64::from(rc.0), i64::from(rc.1));
        let end = (i64::from(rp.0), i64::from(rp.1));
        let (lo, hi) = quad_extent(start, ctrl, end);
        self.grow_box(lo, hi, sel, block);
        self.pen = rp;
    }

    fn grow_line(&mut self, a: (i64, i64), b: (i64, i64), sel: Selectors, block: &Block) {
        let lo = (a.0.min(b.0) as f64, a.1.min(b.1) as f64);
        let hi = (a.0.max(b.0) as f64, a.1.max(b.1) as f64);
        self.grow_box(lo, hi, sel, block);
    }

    /// Adds a segment's box to the edge bounds, and grown by half its stroke
    /// width to the shape bounds.
    fn grow_box(&mut self, lo: (f64, f64), hi: (f64, f64), sel: Selectors, block: &Block) {
        let r = rect(lo, hi, 0.0);
        self.edge_bounds = Some(match self.edge_bounds {
            Some(e) => e.union(&r),
            None => r,
        });
        let half = usize::try_from(sel.line)
            .ok()
            .and_then(|i| i.checked_sub(1))
            .and_then(|i| block.half_widths.get(i))
            .copied()
            .unwrap_or(0.0);
        let g = rect(lo, hi, half);
        self.shape_bounds = Some(match self.shape_bounds {
            Some(s) => s.union(&g),
            None => g,
        });
    }

    fn fill_style(&mut self, ctx: &mut Ctx<'_>, f: &FillStyle, origin: &Origin) -> SwfFill {
        match f {
            FillStyle::Solid(c) => SwfFill::Color(units::color(*c)),
            FillStyle::Linear(g) => gradient_fill(ctx, g, false, origin, &mut self.needs_v4),
            FillStyle::Radial(g) => gradient_fill(ctx, g, true, origin, &mut self.needs_v4),
            FillStyle::Bitmap(b) => match bitmap::fill_style(ctx, b, origin) {
                Some(style) => {
                    if let SwfFill::Bitmap { id, .. } = &style {
                        self.deps.push(*id);
                    }
                    style
                }
                // Not published: keep the selector valid but draw nothing.
                None => SwfFill::Color(swf::Color::TRANSPARENT),
            },
        }
    }

    fn line_style(&mut self, ctx: &mut Ctx<'_>, st: &StrokeStyle, origin: &Origin) -> LineStyle {
        // Flash 8 writes every stroked shape as DefineShape4 (LineStyle2).
        self.needs_v4 = true;
        if st.kind != "SolidStroke" {
            ctx.warn(
                origin.clone(),
                format!("{} strokes are published as solid strokes", st.kind),
            );
        }
        let width = if st.hairline {
            0
        } else {
            stroke_width_twips(st)
        };
        if !st.hairline && st.weight * 20.0 > f64::from(u16::MAX) {
            ctx.warn(
                origin.clone(),
                format!(
                    "stroke weight {} px exceeds the SWF maximum of 3276.75 px and was clamped",
                    st.weight
                ),
            );
        }
        let cap = match st.caps {
            Caps::None => LineCapStyle::None,
            Caps::Round => LineCapStyle::Round,
            Caps::Square => LineCapStyle::Square,
        };
        let join = match st.joints {
            Joints::Miter => LineJoinStyle::Miter(units::fixed8(st.miter_limit.max(0.0))),
            Joints::Round => LineJoinStyle::Round,
            Joints::Bevel => LineJoinStyle::Bevel,
        };
        let (scale_x, scale_y) = scale_flags(st.scale_mode);
        if scale_x && scale_y {
            self.scaling_strokes = true;
        } else {
            self.non_scaling_strokes = true;
        }
        let ls = LineStyle::new()
            .with_width(Twips::new(i32::from(width)))
            .with_start_cap(cap)
            .with_end_cap(cap)
            .with_join_style(join)
            .with_allow_scale_x(scale_x)
            .with_allow_scale_y(scale_y)
            .with_is_pixel_hinted(st.pixel_hinting);
        match &st.fill {
            FillStyle::Solid(c) => ls.with_color(units::color(*c)),
            other => {
                let fill = self.fill_style(ctx, other, origin);
                ls.with_fill_style(fill)
            }
        }
    }
}

/// The stroke width in twips, clamped to the u16 the format stores.
fn stroke_width_twips(st: &StrokeStyle) -> u16 {
    let w = units::round_clamped(st.weight * 20.0, f64::from(u16::MAX));
    u16::try_from(w.max(0)).unwrap_or(u16::MAX)
}

/// `(allow_scale_x, allow_scale_y)` for a stroke scale mode.
///
/// The horizontal/vertical mapping follows the model's documentation
/// (horizontal = `NO_V_SCALE`) and is unverified against Flash (research F8
/// correction).
pub fn scale_flags(mode: ScaleMode) -> (bool, bool) {
    match mode {
        ScaleMode::Normal => (true, true),
        ScaleMode::Horizontal => (true, false),
        ScaleMode::Vertical => (false, true),
        ScaleMode::None => (false, false),
    }
}

/// The SWF Fixed8 focal point for the model's `focal_point`.
///
/// JPEXS writes XFL `focalPointRatio = focalPoint / 255` while Flash 8's SWF
/// stores `focalPoint / 256` (MANIFEST, gradients), so the raw value is
/// `round(ratio × 255)`, clamped to ±255. This is the one place to change if
/// an Animate-saved file shows the ratio is exact.
pub fn focal_point_bits(ratio: f64) -> Fixed8 {
    Fixed8::from_bits(units::round_clamped(ratio * 255.0, 255.0) as i16)
}

/// A gradient fill (or a solid stand-in for degenerate gradients).
fn gradient_fill(
    ctx: &mut Ctx<'_>,
    g: &Gradient,
    radial: bool,
    origin: &Origin,
    needs_v4: &mut bool,
) -> SwfFill {
    let mut stops: Vec<_> = g.stops.clone();
    if stops.is_empty() {
        ctx.warn(
            origin.clone(),
            "a gradient without colours was left transparent",
        );
        return SwfFill::Color(swf::Color::TRANSPARENT);
    }
    if stops.len() == 1 {
        return SwfFill::Color(units::color(stops[0].color));
    }
    if stops.len() > MAX_GRADIENT_RECORDS {
        ctx.warn(
            origin.clone(),
            format!(
                "a gradient has {} colours; SWF 8 allows {MAX_GRADIENT_RECORDS}, the middle ones were dropped",
                stops.len()
            ),
        );
        let last = stops[stops.len() - 1];
        stops.truncate(MAX_GRADIENT_RECORDS - 1);
        stops.push(last);
    }
    let mut prev = 0u8;
    let records = stops
        .iter()
        .map(|s| {
            let r = units::round_clamped(s.ratio.clamp(0.0, 1.0) * 255.0, 255.0);
            let ratio = u8::try_from(r.max(0)).unwrap_or(255).max(prev);
            prev = ratio;
            GradientRecord {
                ratio,
                color: units::color(s.color),
            }
        })
        .collect();
    let gradient = swf::Gradient {
        matrix: gradient_matrix(&g.matrix),
        spread: match g.spread {
            Spread::Pad => GradientSpread::Pad,
            Spread::Reflect => GradientSpread::Reflect,
            Spread::Repeat => GradientSpread::Repeat,
        },
        interpolation: match g.interpolation {
            Interp::Rgb => GradientInterpolation::Rgb,
            Interp::LinearRgb => GradientInterpolation::LinearRgb,
        },
        records,
    };
    if !radial {
        return SwfFill::LinearGradient(gradient);
    }
    let focal = focal_point_bits(g.focal_point);
    if focal.get() == 0 {
        SwfFill::RadialGradient(gradient)
    } else {
        *needs_v4 = true;
        SwfFill::FocalGradient {
            gradient,
            focal_point: focal,
        }
    }
}

/// The model gradient matrix (a..d raw, tx/ty in pixels) to SWF.
fn gradient_matrix(m: &jazari_xfl::model::Matrix) -> swf::Matrix {
    units::matrix(m)
}

/// Checks an edge's selectors against the style counts. Returns `None` when
/// the edge draws nothing.
fn sanitize(edge: &Edge, fills: usize, strokes: usize) -> Option<Selectors> {
    let ok = |v: u32, n: usize| {
        if usize::try_from(v).is_ok_and(|v| v <= n) {
            v
        } else {
            0
        }
    };
    let mut fill0 = ok(edge.fill0, fills);
    let mut fill1 = ok(edge.fill1, fills);
    let line = ok(edge.stroke, strokes);
    if fill0 == fill1 {
        fill0 = 0;
        fill1 = 0;
    }
    (fill0 != 0 || fill1 != 0 || line != 0).then_some(Selectors { fill0, fill1, line })
}

fn round_pt(p: Pt) -> (i32, i32) {
    (
        units::round_clamped(p.x, units::MAX_TWIPS),
        units::round_clamped(p.y, units::MAX_TWIPS),
    )
}

fn mid(a: Pt, b: Pt) -> Pt {
    Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

fn clamp_i32(v: i64) -> i32 {
    i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX })
}

/// The bounding box of a quadratic Bézier, extrema included.
fn quad_extent(p0: (i64, i64), p1: (i64, i64), p2: (i64, i64)) -> ((f64, f64), (f64, f64)) {
    let axis = |a: i64, b: i64, c: i64| -> (f64, f64) {
        let (a, b, c) = (a as f64, b as f64, c as f64);
        let mut lo = a.min(c);
        let mut hi = a.max(c);
        let den = a - 2.0 * b + c;
        if den != 0.0 {
            let t = (a - b) / den;
            if t > 0.0 && t < 1.0 {
                let v = (1.0 - t) * (1.0 - t) * a + 2.0 * (1.0 - t) * t * b + t * t * c;
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        (lo, hi)
    };
    let (x0, x1) = axis(p0.0, p1.0, p2.0);
    let (y0, y1) = axis(p0.1, p1.1, p2.1);
    ((x0, y0), (x1, y1))
}

fn rect(lo: (f64, f64), hi: (f64, f64), grow: f64) -> Rectangle<Twips> {
    Rectangle {
        x_min: units::twips((lo.0 - grow).floor()),
        x_max: units::twips((hi.0 + grow).ceil()),
        y_min: units::twips((lo.1 - grow).floor()),
        y_max: units::twips((hi.1 + grow).ceil()),
    }
}

#[cfg(test)]
mod tests;
