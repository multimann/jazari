//! Matrix interpolation as Flash 8 bakes it.
//!
//! Calibrated on the `tweens` fixture and the `tween-probes/matrices` and
//! `tween-probes/tpoint` probes:
//!
//! 1. Each keyframe matrix (16.16, as Flash stores it) is decomposed into
//!    `sx = |(a, b)|`, `sy = |(c, d)|`, the rotation `ry = atan2(b, a)` and
//!    the skew angle `rx = atan2(−c, d)`, in degrees. Scales stay positive:
//!    a mirrored keyframe has `ry` and `rx` 180° apart.
//! 2. The rotation turns by [`rotation_delta`]; `rx` turns by the same amount
//!    plus the short-way change of `rx − ry` (the skew).
//! 3. Scales interpolate linearly, or stay at the start values when the
//!    tween's "scale" option is off.
//! 4. The matrix is rebuilt with Flash's table trigonometry
//!    ([`super::trig`]): each sine or cosine is rounded to 16.16 first, then
//!    multiplied by the scale and rounded again (halves up).
//! 5. The *transformation point* moves on a straight line between its two
//!    keyframe positions, and the translation is whatever puts the start
//!    keyframe's (local) transformation point there. With the point at the
//!    registration point (the usual centred symbol) this is a linear
//!    interpolation of `tx, ty`.
//!
//! Known residuals (see the probe tests): keyframe angles that are not
//! axis-aligned decompose in Flash 8 to values ~1e-4° away from `atan2`, so
//! an in-between angle that lands on a quarter-degree table entry can fall on
//! the other side of it (one frame off by up to 0.19°); scales of such
//! keyframes can differ by 1 to 3 units of 1/65536; translations with an
//! off-centre transformation point can be 1 twip (rarely 2) off.

use jazari_xfl::model::{Matrix, MotionTween, Pt, RotateMode};

use super::ease::round_half_up;
use super::trig::{cos_deg, sin_deg};

/// A keyframe matrix split the way Flash 8 tweens it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Parts {
    /// Length of the x axis.
    pub sx: f64,
    /// Length of the y axis.
    pub sy: f64,
    /// Angle of the y axis from vertical, degrees.
    pub rx: f64,
    /// Angle of the x axis from horizontal (the rotation), degrees.
    pub ry: f64,
}

/// Rounds to the nearest 1/65536 (Flash stores matrices in 16.16).
fn q16(v: f64) -> f64 {
    (v * 65536.0).round() / 65536.0
}

/// Decomposes a matrix (its linear part) into [`Parts`].
pub(super) fn decompose(m: &Matrix) -> Parts {
    let (a, b, c, d) = (q16(m.a), q16(m.b), q16(m.c), q16(m.d));
    Parts {
        sx: a.hypot(b),
        sy: c.hypot(d),
        rx: (-c).atan2(d).to_degrees(),
        ry: b.atan2(a).to_degrees(),
    }
}

/// An angle in degrees, normalised to (−180, 180].
pub(super) fn normalize(a: f64) -> f64 {
    if !a.is_finite() {
        return 0.0;
    }
    let mut a = a % 360.0;
    if a <= -180.0 {
        a += 360.0;
    } else if a > 180.0 {
        a -= 360.0;
    }
    a
}

/// The angle (degrees) the rotation turns by from `from` to `to`:
///
/// - auto: the short way;
/// - none: no turn at all (the angle stays at the start keyframe's, as in
///   the `tween-probes/matrices` probe);
/// - clockwise / counter-clockwise: the short way if it goes in that
///   direction, else the long way, plus `rotate_times` whole turns (equal
///   angles with one turn make exactly one revolution).
///
/// Positive is clockwise on screen.
pub(super) fn rotation_delta(tween: &MotionTween, from: f64, to: f64) -> f64 {
    let d = normalize(to - from);
    let turns = f64::from(tween.rotate_times.min(1000)) * 360.0;
    match tween.rotate {
        RotateMode::Auto => d,
        RotateMode::None => 0.0,
        RotateMode::Clockwise => {
            let cw = if d < 0.0 { d + 360.0 } else { d };
            cw + turns
        }
        RotateMode::CounterClockwise => {
            let ccw = if d > 0.0 { d - 360.0 } else { d };
            ccw - turns
        }
    }
}

/// Where a matrix puts a local point (pixels).
fn apply(m: &Matrix, p: Pt) -> (f64, f64) {
    (m.a * p.x + m.c * p.y + m.tx, m.b * p.x + m.d * p.y + m.ty)
}

/// Interpolates between two keyframe placements at eased progress `q`.
/// `tp_from` and `tp_to` are the keyframes' transformation points (local).
/// The result is exact in Fixed16 and twips: `a..d` are multiples of
/// 1/65536 and `tx, ty` multiples of 1/20 px.
pub(super) fn interpolate(
    tween: &MotionTween,
    from: &Matrix,
    to: &Matrix,
    tp_from: Pt,
    tp_to: Pt,
    q: f64,
) -> Matrix {
    let finite = |p: Pt| p.x.is_finite() && p.y.is_finite();
    if !from.is_finite() || !to.is_finite() || !q.is_finite() || !finite(tp_from) || !finite(tp_to)
    {
        return *from;
    }
    let p0 = decompose(from);
    let p1 = decompose(to);
    let turn = rotation_delta(tween, p0.ry, p1.ry);
    let skew = normalize((p1.rx - p1.ry) - (p0.rx - p0.ry));
    let ry = p0.ry + turn * q;
    let rx = p0.rx + (turn + skew) * q;
    let (sx, sy) = if tween.scale {
        (p0.sx + (p1.sx - p0.sx) * q, p0.sy + (p1.sy - p0.sy) * q)
    } else {
        (p0.sx, p0.sy)
    };
    // Each sine and cosine is rounded to 16.16, then scaled and rounded.
    let fixed = |v: f64| round_half_up(v * 65536.0);
    let a = round_half_up(fixed(cos_deg(ry)) * sx) / 65536.0;
    let b = round_half_up(fixed(sin_deg(ry)) * sx) / 65536.0;
    let c = round_half_up(-fixed(sin_deg(rx)) * sy) / 65536.0;
    let d = round_half_up(fixed(cos_deg(rx)) * sy) / 65536.0;
    // The transformation point travels in a straight line.
    let (x0, y0) = apply(from, tp_from);
    let (x1, y1) = apply(to, tp_to);
    let (px, py) = (x0 + (x1 - x0) * q, y0 + (y1 - y0) * q);
    let tx = round_half_up((px - (a * tp_from.x + c * tp_from.y)) * 20.0) / 20.0;
    let ty = round_half_up((py - (b * tp_from.x + d * tp_from.y)) * 20.0) / 20.0;
    let m = Matrix { a, b, c, d, tx, ty };
    if m.is_finite() { m } else { *from }
}
