//! Tween progress: Flash 8's 16.16 linear progress, the classic ease and
//! custom ease curves.
//!
//! Calibrated on the `tweens` fixture and the `tween-probes/ease` probe:
//!
//! - The linear progress of step `k` of `n` is `round(k·65536/n)/65536`
//!   (the rounding decides which side of a sine table entry a rotation
//!   lands on, see [`super::trig`]).
//! - The classic ease (IDE value `e`, −100 = ease in) is the quadratic
//!   `t + s·t − s·t²` with `s = e/100`, computed in 16.16 integers with
//!   Flash's roundings: `s` and `t²` are rounded to 16.16, `s·t` is rounded,
//!   `s·t²` is not, and the sum is rounded (halves up throughout). This is
//!   exact on all 76 scale frames of the probe (±50, ±100) and on the
//!   fixture's eased rotations; the fixture's 2 → 0.5 ease-out scale is
//!   still 1/65536 off on 8 frames.
//! - A custom ease curve is evaluated at the *exact* progress `k/n` and
//!   truncated to 16.16 (exact on every probe frame).
//!
//! The progress applies to every property: matrix, colour and filters.

use jazari_xfl::model::MotionTween;

/// Converts the model's raw XFL `acceleration` to the Flash IDE's ease value
/// (−100 = ease in, +100 = ease out).
///
/// **The one place that decides the XFL ease sign.** In the JPEXS-converted
/// `tweens` fixture the IDE's ease −100 (a slow start in Flash 8's SWF) is
/// written `acceleration="100"`, and the `tween-probes/ease` probe confirms
/// it for −70, −30, 30 and 70, so the sign is flipped here. If
/// Animate-saved files turn out to use the IDE sign, change this function
/// (and its test) only.
pub fn ide_ease(xfl_acceleration: f64) -> f64 {
    if xfl_acceleration.is_finite() {
        -xfl_acceleration.clamp(-100.0, 100.0)
    } else {
        0.0
    }
}

/// `floor(v + 0.5)`: Flash 8 rounds halves up.
pub(super) fn round_half_up(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// Flash 8's linear progress of step `step` of `steps`, in 16.16.
pub(super) fn linear(step: u32, steps: u32) -> f64 {
    if steps == 0 {
        return 0.0;
    }
    let t = f64::from(step.min(steps)) * 65536.0 / f64::from(steps);
    round_half_up(t) / 65536.0
}

/// The eased progress of step `step` of `steps` (`0 < step < steps`).
pub(super) fn progress(tween: &MotionTween, step: u32, steps: u32) -> f64 {
    if steps == 0 {
        return 0.0;
    }
    if let Some(curve) = tween.custom_ease.as_deref() {
        let x = f64::from(step.min(steps)) / f64::from(steps);
        if let Some(y) = custom_ease(curve, x) {
            return (y * 65536.0).floor() / 65536.0;
        }
    }
    classic(linear(step, steps) * 65536.0, ide_ease(tween.ease)) / 65536.0
}

/// The classic ease of 16.16 progress `t` (an integer in 0..=65536) for the
/// IDE ease value `ease`, in 16.16 (see the module documentation).
fn classic(t: f64, ease: f64) -> f64 {
    let s = round_half_up(ease * 65536.0 / 100.0);
    let t2 = round_half_up(t * t / 65536.0);
    round_half_up(t + round_half_up(s * t / 65536.0) - s * t2 / 65536.0)
}

/// Evaluates a custom ease curve (points `[x, y]`, a piecewise cubic Bézier
/// of `3n + 1` points) at `x`. `None` for a malformed curve, or when no
/// segment covers `x`.
pub(super) fn custom_ease(points: &[[f64; 2]], x: f64) -> Option<f64> {
    if points.len() < 4
        || !(points.len() - 1).is_multiple_of(3)
        || !x.is_finite()
        || points.iter().flatten().any(|v| !v.is_finite())
    {
        return None;
    }
    for seg in points.windows(4).step_by(3) {
        let (x0, x3) = (seg[0][0], seg[3][0]);
        if x < x0.min(x3) || x > x0.max(x3) {
            continue;
        }
        let bez = |u: f64, i: usize| {
            let v = 1.0 - u;
            v * v * v * seg[0][i]
                + 3.0 * v * v * u * seg[1][i]
                + 3.0 * v * u * u * seg[2][i]
                + u * u * u * seg[3][i]
        };
        // Solve x(u) = x by bisection (x(u) is monotonic for curves the IDE
        // makes; for others this finds one of the solutions).
        let increasing = x3 >= x0;
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for _ in 0..64 {
            let m = (lo + hi) / 2.0;
            if (bez(m, 0) < x) == increasing {
                lo = m;
            } else {
                hi = m;
            }
        }
        let y = bez((lo + hi) / 2.0, 1);
        return y.is_finite().then_some(y);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(v: f64) -> i64 {
        (v * 65536.0).round() as i64
    }

    fn classic(acceleration: f64) -> MotionTween {
        MotionTween {
            ease: acceleration,
            ..MotionTween::default()
        }
    }

    #[test]
    fn linear_progress_is_rounded_to_16_16() {
        assert_eq!(fx(linear(1, 120)), 546);
        assert_eq!(fx(linear(4, 120)), 2185);
        assert_eq!(fx(linear(15, 120)), 8192);
        assert_eq!(linear(3, 0), 0.0);
        assert_eq!(linear(9, 3), 1.0);
    }

    #[test]
    fn classic_ease_matches_flash_8() {
        // tween-probes/ease, scale 1 -> 3 over 20 frames: Flash 8's 16.16
        // progress at frame 1 and 2 for the IDE's -100 and +100.
        let ease_in = classic(100.0);
        assert_eq!(fx(progress(&ease_in, 1, 20)), 164);
        assert_eq!(fx(progress(&ease_in, 2, 20)), 655);
        let ease_out = classic(-100.0);
        assert_eq!(fx(progress(&ease_out, 1, 20)), 6390);
        assert_eq!(fx(progress(&ease_out, 2, 20)), 12453);
        assert_eq!(fx(progress(&classic(0.0), 7, 20)), fx(linear(7, 20)));
        // The IDE's -50 and +50 at frame 4: 7865 and 18351 (the plain
        // quadratic gives 7864.18 and 18349.82).
        assert_eq!(fx(progress(&classic(50.0), 4, 20)), 7865);
        assert_eq!(fx(progress(&classic(-50.0), 4, 20)), 18351);
    }

    #[test]
    fn custom_ease_is_truncated_at_the_exact_progress() {
        // tween-probes/ease, layer scale_custom: frames 1 and 2.
        let curve = vec![
            [0.0, 0.0],
            [0.1, 0.4],
            [0.2, 0.6],
            [0.5, 0.5],
            [0.7, 0.4],
            [0.9, 0.9],
            [1.0, 1.0],
        ];
        let t = MotionTween {
            custom_ease: Some(curve),
            ..MotionTween::default()
        };
        assert_eq!(fx(progress(&t, 1, 20)), 11794);
        assert_eq!(fx(progress(&t, 2, 20)), 20557);
        assert_eq!(fx(progress(&t, 10, 20)), 32768);
    }

    #[test]
    fn malformed_curves_fall_back_to_the_classic_ease() {
        let t = MotionTween {
            custom_ease: Some(vec![[0.0, 0.0], [f64::NAN, 1.0], [1.0, 1.0]]),
            ..MotionTween::default()
        };
        assert_eq!(progress(&t, 1, 2), 0.5);
        assert_eq!(custom_ease(&[[0.0, 0.0]; 4], 2.0), None);
    }

    #[test]
    fn ease_sign_follows_the_fixtures() {
        assert_eq!(ide_ease(100.0), -100.0);
        assert_eq!(ide_ease(-50.0), 50.0);
        assert_eq!(ide_ease(f64::NAN), 0.0);
        assert_eq!(ide_ease(1e9), -100.0);
    }
}
