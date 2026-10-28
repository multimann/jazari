//! Unit conversions from the model (pixels and ratios as `f64`) to SWF fixed
//! point values.
//!
//! The `swf` crate's own helpers (`Twips::from_pixels`, `Fixed16::from_f64`,
//! `Matrix::rotate`, matrix products) truncate. Everything here rounds to the
//! nearest value once, after all maths has been done in `f64`
//! (`docs/research/swf-writer.md`, F2 and the missed points). Non-finite input
//! becomes 0 and out-of-range input saturates; nothing here panics.

use jazari_xfl::model::{ColorEffect, Matrix, Rgba};
use swf::{Color, ColorTransform, Fixed8, Fixed16, Twips};

/// Largest coordinate magnitude we emit, in twips. Far inside `i32` and the
/// 31-bit move fields, so later deltas cannot overflow.
pub const MAX_TWIPS: f64 = 1_073_741_823.0;

/// Rounds a finite value to the nearest `i32` within `±limit`; NaN gives 0.
pub fn round_clamped(v: f64, limit: f64) -> i32 {
    if v.is_nan() {
        return 0;
    }
    // `as` saturates, and the clamp keeps the result inside `limit`.
    v.round().clamp(-limit, limit) as i32
}

/// Pixels to twips, rounded to the nearest twip.
pub fn px_to_twips(px: f64) -> Twips {
    Twips::new(round_clamped(px * 20.0, MAX_TWIPS))
}

/// A value already in twips (shape edges), rounded to the nearest twip.
pub fn twips(v: f64) -> Twips {
    Twips::new(round_clamped(v, MAX_TWIPS))
}

/// Largest raw Fixed16 magnitude we emit: matrix fields are written with
/// at most 31 bits (a 5-bit size field), so `i32::MAX` would not fit.
pub const MAX_FIXED16_BITS: f64 = 1_073_741_823.0;

/// Largest colour transform field magnitude: CXFORM fields are written with
/// at most 15 bits (a 4-bit size field). Multipliers are 8.8, so this is
/// about ×64; offsets are colour units.
pub const MAX_CXFORM_FIELD: f64 = 16_383.0;

/// A ratio to Fixed16 (16.16), rounded and clamped to what a matrix field
/// holds (about ±16384).
pub fn fixed16(v: f64) -> Fixed16 {
    Fixed16::from_bits(round_clamped(v * 65536.0, MAX_FIXED16_BITS))
}

/// A ratio to Fixed8 (8.8), rounded.
pub fn fixed8(v: f64) -> Fixed8 {
    Fixed8::from_bits(round_clamped(v * 256.0, f64::from(i16::MAX)) as i16)
}

/// A ratio to Fixed8 (8.8), truncated toward zero, as Flash 8 writes filter
/// strengths (80 % becomes 204/256 = 0.796875 in the `symbols` fixture).
pub fn fixed8_trunc(v: f64) -> Fixed8 {
    let scaled = v * 256.0;
    let t = if scaled.is_nan() {
        0.0
    } else {
        scaled
            .trunc()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX))
    };
    Fixed8::from_bits(t as i16)
}

/// A model matrix (pixels) to an SWF matrix (Fixed16 and twips).
pub fn matrix(m: &Matrix) -> swf::Matrix {
    swf::Matrix {
        a: fixed16(m.a),
        b: fixed16(m.b),
        c: fixed16(m.c),
        d: fixed16(m.d),
        tx: px_to_twips(m.tx),
        ty: px_to_twips(m.ty),
    }
}

/// A model colour to an SWF colour (RGBA).
pub fn color(c: Rgba) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

/// A normalised colour effect to an SWF colour transform. Multipliers round
/// to 1/256 (Flash's alpha 40 % is already 102/256 in the model), offsets to
/// whole colour units. Both are clamped to the 15 bits a CXFORM field holds
/// ([`MAX_CXFORM_FIELD`]).
pub fn color_transform(c: &ColorEffect) -> ColorTransform {
    // Both limits fit an i16, so the casts are exact.
    let add = |v: f64| round_clamped(v, MAX_CXFORM_FIELD) as i16;
    let mult = |v: f64| Fixed8::from_bits(round_clamped(v * 256.0, MAX_CXFORM_FIELD) as i16);
    ColorTransform {
        r_multiply: mult(c.mult[0]),
        g_multiply: mult(c.mult[1]),
        b_multiply: mult(c.mult[2]),
        a_multiply: mult(c.mult[3]),
        r_add: add(c.add[0]),
        g_add: add(c.add[1]),
        b_add: add(c.add[2]),
        a_add: add(c.add[3]),
    }
}

/// Composes two colour effects: `outer` applied after `inner`
/// (`out = (in × mi + ai) × mo + ao`).
pub fn compose_color(outer: &ColorEffect, inner: &ColorEffect) -> ColorEffect {
    let mut out = ColorEffect::IDENTITY;
    for i in 0..4 {
        out.mult[i] = inner.mult[i] * outer.mult[i];
        out.add[i] = inner.add[i] * outer.mult[i] + outer.add[i];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_is_to_nearest_and_saturating() {
        assert_eq!(px_to_twips(5.45).get(), 109);
        assert_eq!(px_to_twips(-0.026).get(), -1);
        assert_eq!(px_to_twips(f64::NAN).get(), 0);
        assert_eq!(px_to_twips(1e300).get(), MAX_TWIPS as i32);
        assert_eq!(fixed16(0.5).get(), 32768);
        assert_eq!(fixed8(0.3984375).get(), 102);
        assert_eq!(fixed8(1e9).get(), i16::MAX);
        assert_eq!(fixed8_trunc(0.8).get(), 204);
        assert_eq!(fixed8_trunc(2.0).get(), 512);
        assert_eq!(fixed16(1e12).get(), MAX_FIXED16_BITS as i32);
        assert_eq!(fixed16(f64::NEG_INFINITY).get(), -(MAX_FIXED16_BITS as i32));
    }

    #[test]
    fn colour_transform_fields_fit_fifteen_bits() {
        let ct = color_transform(&ColorEffect {
            mult: [100.0, -100.0, 1.0, f64::NAN],
            add: [1e6, -1e6, 0.0, f64::INFINITY],
        });
        assert_eq!(ct.r_multiply.get(), 16_383);
        assert_eq!(ct.g_multiply.get(), -16_383);
        assert_eq!(ct.a_multiply.get(), 0);
        assert_eq!((ct.r_add, ct.g_add, ct.a_add), (16_383, -16_383, 16_383));
    }

    #[test]
    fn colour_transform_matches_flash_values() {
        // Tint #0000FF 60 %: mult 0.4 (102/256), add (0, 0, 153).
        let tint = ColorEffect {
            mult: [0.4, 0.4, 0.4, 1.0],
            add: [0.0, 0.0, 153.0, 0.0],
        };
        let ct = color_transform(&tint);
        assert_eq!(ct.r_multiply.get(), 102);
        assert_eq!(ct.b_add, 153);
        assert_eq!(ct.a_multiply.get(), 256);
    }

    #[test]
    fn colour_composition_applies_inner_first() {
        let outer = ColorEffect {
            mult: [0.5; 4],
            add: [10.0; 4],
        };
        let inner = ColorEffect {
            mult: [1.0; 4],
            add: [100.0; 4],
        };
        let c = compose_color(&outer, &inner);
        assert_eq!(c.mult[0], 0.5);
        assert_eq!(c.add[0], 60.0);
    }
}
