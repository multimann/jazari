//! Colour effects (`<color><Color .../></color>`), normalised to multipliers
//! and offsets.
//!
//! Flash writes either explicit channel attributes (`alphaMultiplier`,
//! `redOffset`, …), a shorthand (`brightness`, or `tintMultiplier` with
//! `tintColor`), or both. The shorthand is converted first (F21):
//!
//! - brightness `b ≥ 0`: RGB multiplier `1 − b`, RGB offset `255 b`;
//! - brightness `b < 0`: RGB multiplier `1 + b`, offset 0;
//! - tint `t` with colour `C`: RGB multiplier `1 − t`, offset `C · t`.
//!
//! Then every explicit attribute that is present overrides its channel, so
//! Flash's own rounded values (`alphaMultiplier="0.3984375"`) are kept exactly.

use crate::WarningKind;
use crate::model::{ColorEffect, Rgba};

use super::ctx::Ctx;
use super::xml::Node;

/// Explicit channel attributes in R, G, B, A order.
const MULTIPLIERS: [&str; 4] = [
    "redMultiplier",
    "greenMultiplier",
    "blueMultiplier",
    "alphaMultiplier",
];
const OFFSETS: [&str; 4] = ["redOffset", "greenOffset", "blueOffset", "alphaOffset"];

/// Reads a `<Color>` element.
pub(crate) fn read_color_effect(ctx: &mut Ctx, node: &Node) -> ColorEffect {
    let mut effect = ColorEffect::IDENTITY;

    if node.has_attr("tintMultiplier") || node.has_attr("tintColor") {
        let t = ctx.f64(node, "tintMultiplier", 0.0);
        let c = ctx.color(node, "tintColor", Rgba::BLACK);
        let rgb = [c.r, c.g, c.b];
        for (i, channel) in rgb.into_iter().enumerate() {
            effect.mult[i] = 1.0 - t;
            effect.add[i] = f64::from(channel) * t;
        }
        if node.has_attr("brightness") {
            ctx.warn(
                WarningKind::BadValue,
                "<Color> has both a tint and a brightness; the tint is used",
            );
        }
    } else if let Some(b) = ctx.opt_f64(node, "brightness") {
        let b = b.clamp(-1.0, 1.0);
        let (mult, add) = if b >= 0.0 {
            (1.0 - b, 255.0 * b)
        } else {
            (1.0 + b, 0.0)
        };
        effect.mult[..3].fill(mult);
        effect.add[..3].fill(add);
    }

    for (i, (mult, offset)) in MULTIPLIERS.iter().zip(OFFSETS.iter()).enumerate() {
        if let Some(m) = ctx.opt_f64(node, mult) {
            effect.mult[i] = m;
        }
        if let Some(a) = ctx.opt_f64(node, offset) {
            effect.add[i] = a;
        }
    }
    effect
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::xml::parse_str;

    fn effect(xml: &str) -> (ColorEffect, usize) {
        let mut ctx = Ctx::default();
        let e = read_color_effect(&mut ctx, &parse_str(xml).unwrap());
        (e, ctx.report.len())
    }

    #[test]
    fn alpha_keeps_flash_rounding() {
        let (e, w) = effect(r#"<Color alphaMultiplier="0.3984375"/>"#);
        assert_eq!(e.mult, [1.0, 1.0, 1.0, 0.3984375]);
        assert_eq!(e.add, [0.0; 4]);
        assert_eq!(w, 0);
    }

    #[test]
    fn tint_becomes_multiplier_and_offset() {
        let (e, _) = effect(r##"<Color tintColor="#0000FF" tintMultiplier="0.6"/>"##);
        for m in &e.mult[..3] {
            assert!((m - 0.4).abs() < 1e-12);
        }
        assert_eq!(e.mult[3], 1.0);
        assert_eq!(e.add[0], 0.0);
        assert_eq!(e.add[1], 0.0);
        assert!((e.add[2] - 153.0).abs() < 1e-9);
        assert_eq!(e.add[3], 0.0);
    }

    #[test]
    fn brightness_both_ways() {
        let (dark, _) = effect(r#"<Color brightness="-0.5"/>"#);
        assert_eq!(dark.mult, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(dark.add, [0.0; 4]);
        let (light, _) = effect(r#"<Color brightness="0.25"/>"#);
        assert_eq!(light.mult, [0.75, 0.75, 0.75, 1.0]);
        assert_eq!(light.add, [63.75, 63.75, 63.75, 0.0]);
    }

    #[test]
    fn advanced_attributes_are_taken_as_written() {
        let (e, _) = effect(
            r#"<Color alphaMultiplier="0.80078125" alphaOffset="0" blueMultiplier="0.19921875" blueOffset="0" greenMultiplier="1" greenOffset="-60" redMultiplier="0.5" redOffset="100"/>"#,
        );
        assert_eq!(e.mult, [0.5, 1.0, 0.19921875, 0.80078125]);
        assert_eq!(e.add, [100.0, -60.0, 0.0, 0.0]);
    }

    #[test]
    fn explicit_channels_override_the_shorthand() {
        // Flash may write the shorthand and the exact channels together.
        let (e, _) = effect(
            r##"<Color tintColor="#0000FF" tintMultiplier="0.6" redMultiplier="0.3984375" blueOffset="153"/>"##,
        );
        assert_eq!(e.mult[0], 0.3984375);
        assert!((e.mult[1] - 0.4).abs() < 1e-12);
        assert_eq!(e.add[2], 153.0);
    }

    #[test]
    fn empty_color_is_identity() {
        let (e, w) = effect("<Color/>");
        assert!(e.is_identity());
        assert_eq!(w, 0);
        let (e, w) = effect(r#"<Color redMultiplier="x"/>"#);
        assert!(e.is_identity());
        assert_eq!(w, 1);
    }
}
