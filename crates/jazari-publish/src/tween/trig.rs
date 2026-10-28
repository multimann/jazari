//! Flash 8's sine and cosine for tween rotation.
//!
//! The Flash 8 IDE does not use exact trigonometry when it bakes a rotating
//! tween. It looks the sine up in a table with one entry per quarter degree
//! and interpolates linearly between entries, but the interpolation weight
//! is the distance from the entry *in degrees* (0 to 0.25), not the fraction
//! of the step (0 to 1). Within each quarter-degree cell the value therefore
//! climbs at a quarter of the right rate and jumps to the exact table value
//! at the next entry. The cosine is the sine of `90° − x`.
//!
//! This reproduces every rotation matrix of the `tweens` fixture and of the
//! `tween-probes/matrices` probe whose keyframe angles decompose exactly
//! (0°, ±90°, 180°, full turns), Fixed16 for Fixed16.

/// The sine of `x` degrees, Flash 8 style.
pub(super) fn sin_deg(x: f64) -> f64 {
    if !x.is_finite() {
        return 0.0;
    }
    let x = x.rem_euclid(360.0);
    if x <= 90.0 {
        quadrant(x)
    } else if x <= 180.0 {
        quadrant(180.0 - x)
    } else if x <= 270.0 {
        -quadrant(x - 180.0)
    } else {
        -quadrant(360.0 - x)
    }
}

/// The cosine of `x` degrees, Flash 8 style (`sin(90° − x)`).
pub(super) fn cos_deg(x: f64) -> f64 {
    if !x.is_finite() {
        return 1.0;
    }
    sin_deg(90.0 - x)
}

/// The table entry `i`: the exact sine of `i / 4` degrees.
fn entry(i: i64) -> f64 {
    (i as f64 * 0.25).to_radians().sin()
}

/// The sine of `x` in 0..=90 degrees.
fn quadrant(x: f64) -> f64 {
    let i = (x * 4.0).floor();
    if i >= 360.0 {
        return 1.0;
    }
    if i < 0.0 {
        return 0.0;
    }
    // 0 <= i < 360 here, so the cast is exact.
    let i = i as i64;
    let lo = entry(i);
    // The weight is in degrees (0..0.25), Flash 8's quirk.
    lo + (entry(i + 1) - lo) * (x - i as f64 / 4.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(v: f64) -> i64 {
        (v * 65536.0 + 0.5).floor() as i64
    }

    #[test]
    fn table_entries_are_exact() {
        assert_eq!(fx(sin_deg(30.0)), 32768);
        assert_eq!(fx(sin_deg(45.0)), 46341);
        assert_eq!(fx(sin_deg(90.0)), 65536);
        assert_eq!(fx(cos_deg(90.0)), 0);
        assert_eq!(fx(sin_deg(180.0)), 0);
        assert_eq!(fx(sin_deg(-90.0)), -65536);
    }

    #[test]
    fn cells_climb_at_a_quarter_rate() {
        // Flash 8 wrote b = 14, 29, 43, 57 for 0.05..0.2 degrees, then the
        // exact 286 at 0.25 (tween-probes/matrices, layer r0_6).
        let b: Vec<i64> = [0.05, 0.1, 0.15, 0.2, 0.25]
            .iter()
            .map(|x| fx(sin_deg(*x)))
            .collect();
        assert_eq!(b, [14, 29, 43, 57, 286]);
        // Step 1 of 23 of a full turn (progress 2849/65536): Flash 8's
        // (63083, 17555) in the tweens fixture.
        let a = 360.0 * 2849.0 / 65536.0;
        assert_eq!(fx(cos_deg(a)), 63083);
        assert_eq!(fx(sin_deg(a)), 17555);
    }

    #[test]
    fn hostile_angles_do_not_panic() {
        assert_eq!(sin_deg(f64::NAN), 0.0);
        assert_eq!(cos_deg(f64::INFINITY), 1.0);
        assert!(sin_deg(1e300).abs() <= 1.0);
        assert!(sin_deg(-1e-300).abs() <= 1.0);
    }
}
