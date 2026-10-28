//! Classic motion tweens: the state of a tweened instance between two
//! keyframes, as the Flash 8 IDE bakes it.
//!
//! The timeline code calls [`tweened_state`] for every in-between frame of a
//! keyframe whose tween is [`Tween::Motion`](jazari_xfl::model::Tween) when
//! the next keyframe (starting right where this one ends) holds an instance
//! of the same symbol in the same slot. The result is placed with one
//! PlaceObject2 *Modify* per frame (Flash 8 bakes tweens the same way).
//!
//! # What Flash 8 does (and this module reproduces)
//!
//! Every rule below was read off SWFs the Flash 8 IDE published: the
//! `tweens` and `smoke` fixtures and the probes in
//! `tests/fixtures/tween-probes` (see their MANIFEST section).
//!
//! - **Progress** ([`ease`]): step `k` of `n` is `round(k·65536/n)/65536`,
//!   eased by the classic quadratic ease (or a custom ease curve), in 16.16.
//!   The same progress drives the matrix, the colour and the filters.
//! - **Matrix** ([`matrix`]): decomposed into scales and two angles,
//!   interpolated with the rotate mode, rebuilt with Flash's quarter-degree
//!   sine table ([`trig`]); the transformation point moves on a straight
//!   line. Rotate "none" keeps the start angle.
//! - **Colour**: both keyframes' colour transforms are first rounded to what
//!   the SWF stores (multipliers in 1/256, offsets in whole units); each of
//!   the eight numbers then moves linearly and is rounded half up.
//! - **Filters** ([`filters`]): aligned by kind, faded in and out from
//!   neutral filters, interpolated with Flash's integer truncations.
//! - **Graphic symbols** inside a tween keep playing from the start
//!   keyframe's first frame (`crate::graphic::inner_frame`, unchanged:
//!   the `tween-probes/graphics` probe confirms it, "sync" included, since
//!   the IDE already stores the synchronised first frame at each keyframe).

mod ease;
mod filters;
mod matrix;
mod trig;

#[cfg(test)]
mod tests;

use jazari_xfl::model::{ColorEffect, Filter, Matrix, MotionTween, SymbolInstance};

use crate::units;

/// The tweened properties of an instance at one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct TweenedState {
    /// Placement matrix (pixels). Exact in Fixed16 and twips.
    pub matrix: Matrix,
    /// Colour effect. Exact in the SWF's units.
    pub color: ColorEffect,
    /// Filters.
    pub filters: Vec<Filter>,
}

/// The state of a tweened instance `step` frames after its keyframe, where
/// the next keyframe is `steps` frames after it (`0 < step < steps`).
pub fn tweened_state(
    tween: &MotionTween,
    from: &SymbolInstance,
    to: &SymbolInstance,
    step: u32,
    steps: u32,
) -> TweenedState {
    if steps == 0 || step == 0 {
        return TweenedState {
            matrix: from.matrix,
            color: from.color,
            filters: from.filters.clone(),
        };
    }
    let q = ease::progress(tween, step, steps);
    TweenedState {
        matrix: matrix::interpolate(
            tween,
            &from.matrix,
            &to.matrix,
            from.transformation_point,
            to.transformation_point,
            q,
        ),
        color: interpolate_color(&from.color, &to.color, q),
        filters: filters::interpolate_list(&from.filters, &to.filters, q),
    }
}

/// Interpolates two colour effects at progress `q`, in the SWF's own units
/// (multipliers in 1/256, offsets in whole colour units), rounding halves
/// up. The result converts back to exactly those numbers.
fn interpolate_color(a: &ColorEffect, b: &ColorEffect, q: f64) -> ColorEffect {
    let ca = units::color_transform(a);
    let cb = units::color_transform(b);
    let mults = |c: &swf::ColorTransform| {
        [
            c.r_multiply.get(),
            c.g_multiply.get(),
            c.b_multiply.get(),
            c.a_multiply.get(),
        ]
    };
    let adds = |c: &swf::ColorTransform| [c.r_add, c.g_add, c.b_add, c.a_add];
    let step = |x: i16, y: i16| {
        let v = f64::from(x) + (f64::from(y) - f64::from(x)) * q;
        if v.is_finite() {
            ease::round_half_up(v).clamp(f64::from(i16::MIN), f64::from(i16::MAX))
        } else {
            f64::from(x)
        }
    };
    let (ma, mb, aa, ab) = (mults(&ca), mults(&cb), adds(&ca), adds(&cb));
    let mut out = ColorEffect::IDENTITY;
    for i in 0..4 {
        out.mult[i] = step(ma[i], mb[i]) / 256.0;
        out.add[i] = step(aa[i], ab[i]);
    }
    out
}
