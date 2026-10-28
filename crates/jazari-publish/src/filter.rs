//! Instance filters and blend modes to their SWF 8 forms (PlaceObject3,
//! button records).
//!
//! Calibrated on the `symbols` fixture: every filter has CompositeSource set
//! unless the object is hidden, passes = quality, strengths are truncated to
//! 1/256 (80 % becomes 0.796875), angles are radians. The `swf` crate already
//! writes BevelFilter colours highlight first, as Flash 8 does.

use jazari_as2::Origin;
use jazari_xfl::model::{
    AdjustColorFilter, BevelPlacement, BlendMode, Filter, FilterKind, GradientFilter,
};
use swf::{
    BevelFilterFlags, BlurFilterFlags, DropShadowFilterFlags, GlowFilterFlags, GradientFilterFlags,
    GradientRecord,
};

use crate::context::Ctx;
use crate::units;

/// Most filters one placement holds (the SWF count is a u8).
pub const MAX_FILTERS: usize = 255;

/// Converts the enabled filters of an instance. Unsupported parameters are
/// clamped with a warning at `origin`; filters past [`MAX_FILTERS`] are left
/// out with a warning.
pub fn filters(ctx: &mut Ctx<'_>, list: &[Filter], origin: &Origin) -> Vec<swf::Filter> {
    let mut out: Vec<swf::Filter> = list
        .iter()
        .filter(|f| f.enabled)
        .filter_map(|f| filter(ctx, &f.kind, origin))
        .collect();
    if out.len() > MAX_FILTERS {
        ctx.warn(
            origin.clone(),
            format!(
                "{} filters exceed the SWF limit of {MAX_FILTERS}; the rest are left out",
                out.len()
            ),
        );
        out.truncate(MAX_FILTERS);
    }
    out
}

/// The SWF blend mode.
pub fn blend_mode(b: BlendMode) -> swf::BlendMode {
    match b {
        BlendMode::Normal => swf::BlendMode::Normal,
        BlendMode::Layer => swf::BlendMode::Layer,
        BlendMode::Multiply => swf::BlendMode::Multiply,
        BlendMode::Screen => swf::BlendMode::Screen,
        BlendMode::Lighten => swf::BlendMode::Lighten,
        BlendMode::Darken => swf::BlendMode::Darken,
        BlendMode::Difference => swf::BlendMode::Difference,
        BlendMode::Add => swf::BlendMode::Add,
        BlendMode::Subtract => swf::BlendMode::Subtract,
        BlendMode::Invert => swf::BlendMode::Invert,
        BlendMode::Alpha => swf::BlendMode::Alpha,
        BlendMode::Erase => swf::BlendMode::Erase,
        BlendMode::Overlay => swf::BlendMode::Overlay,
        BlendMode::HardLight => swf::BlendMode::HardLight,
    }
}

fn passes(q: u8) -> u8 {
    q.clamp(1, 15)
}

fn blur(v: f64) -> swf::Fixed16 {
    units::fixed16(v.clamp(0.0, 255.0))
}

/// Degrees to radians in 16.16, truncated as Flash 8 writes it (45 degrees
/// is 51471/65536 in the `symbols` fixture).
fn angle(deg: f64) -> swf::Fixed16 {
    let v = deg.to_radians() * 65536.0;
    let t = if v.is_finite() {
        v.trunc().clamp(f64::from(i32::MIN), f64::from(i32::MAX))
    } else {
        0.0
    };
    swf::Fixed16::from_bits(t as i32)
}

fn strength(s: f64) -> swf::Fixed8 {
    units::fixed8_trunc(s.clamp(0.0, 255.0))
}

fn filter(ctx: &mut Ctx<'_>, f: &FilterKind, origin: &Origin) -> Option<swf::Filter> {
    Some(match f {
        FilterKind::DropShadow(d) => {
            let mut flags = DropShadowFilterFlags::from_passes(passes(d.quality));
            flags.set(DropShadowFilterFlags::INNER_SHADOW, d.inner);
            flags.set(DropShadowFilterFlags::KNOCKOUT, d.knockout);
            flags.set(DropShadowFilterFlags::COMPOSITE_SOURCE, !d.hide_object);
            swf::Filter::DropShadowFilter(Box::new(swf::DropShadowFilter {
                color: units::color(d.color),
                blur_x: blur(d.blur_x),
                blur_y: blur(d.blur_y),
                angle: angle(d.angle),
                distance: units::fixed16(d.distance),
                strength: strength(d.strength),
                flags,
            }))
        }
        FilterKind::Blur(b) => swf::Filter::BlurFilter(Box::new(swf::BlurFilter {
            blur_x: blur(b.blur_x),
            blur_y: blur(b.blur_y),
            flags: BlurFilterFlags::from_passes(passes(b.quality)),
        })),
        FilterKind::Glow(g) => {
            let mut flags = GlowFilterFlags::from_passes(passes(g.quality));
            flags.set(GlowFilterFlags::INNER_GLOW, g.inner);
            flags.set(GlowFilterFlags::KNOCKOUT, g.knockout);
            flags.set(GlowFilterFlags::COMPOSITE_SOURCE, true);
            swf::Filter::GlowFilter(Box::new(swf::GlowFilter {
                color: units::color(g.color),
                blur_x: blur(g.blur_x),
                blur_y: blur(g.blur_y),
                strength: strength(g.strength),
                flags,
            }))
        }
        FilterKind::Bevel(b) => {
            let mut flags = BevelFilterFlags::from_passes(passes(b.quality));
            flags.set(BevelFilterFlags::KNOCKOUT, b.knockout);
            flags.set(BevelFilterFlags::COMPOSITE_SOURCE, true);
            match b.placement {
                BevelPlacement::Inner => flags.insert(BevelFilterFlags::INNER_SHADOW),
                BevelPlacement::Outer => {}
                BevelPlacement::Full => flags.insert(BevelFilterFlags::ON_TOP),
            }
            swf::Filter::BevelFilter(Box::new(swf::BevelFilter {
                shadow_color: units::color(b.shadow_color),
                highlight_color: units::color(b.highlight_color),
                blur_x: blur(b.blur_x),
                blur_y: blur(b.blur_y),
                angle: angle(b.angle),
                distance: units::fixed16(b.distance),
                strength: strength(b.strength),
                flags,
            }))
        }
        FilterKind::GradientGlow(g) => {
            swf::Filter::GradientGlowFilter(Box::new(gradient_filter(ctx, g, origin)))
        }
        FilterKind::GradientBevel(g) => {
            swf::Filter::GradientBevelFilter(Box::new(gradient_filter(ctx, g, origin)))
        }
        FilterKind::ColorMatrix(m) => {
            swf::Filter::ColorMatrixFilter(Box::new(swf::ColorMatrixFilter {
                matrix: m.matrix.map(|v| v as f32),
            }))
        }
        FilterKind::AdjustColor(a) => {
            swf::Filter::ColorMatrixFilter(Box::new(swf::ColorMatrixFilter {
                matrix: adjust_color_matrix(a).map(|v| v as f32),
            }))
        }
        FilterKind::Convolution(c) => {
            let n = usize::from(c.matrix_x) * usize::from(c.matrix_y);
            if c.matrix.len() != n || n == 0 {
                ctx.warn(
                    origin.clone(),
                    "a convolution filter with a malformed matrix was left out",
                );
                return None;
            }
            let mut flags = swf::ConvolutionFilterFlags::empty();
            flags.set(swf::ConvolutionFilterFlags::CLAMP, c.clamp);
            flags.set(
                swf::ConvolutionFilterFlags::PRESERVE_ALPHA,
                c.preserve_alpha,
            );
            swf::Filter::ConvolutionFilter(Box::new(swf::ConvolutionFilter {
                num_matrix_rows: c.matrix_y,
                num_matrix_cols: c.matrix_x,
                matrix: c.matrix.iter().map(|v| *v as f32).collect(),
                divisor: c.divisor as f32,
                bias: c.bias as f32,
                default_color: units::color(c.color),
                flags,
            }))
        }
    })
}

fn gradient_filter(ctx: &mut Ctx<'_>, g: &GradientFilter, origin: &Origin) -> swf::GradientFilter {
    let mut stops = g.stops.clone();
    if stops.len() > 15 {
        ctx.warn(
            origin.clone(),
            "a gradient filter has more than 15 colours; extra ones were dropped",
        );
        stops.truncate(15);
    }
    let mut flags = GradientFilterFlags::from_passes(passes(g.quality));
    flags.set(GradientFilterFlags::KNOCKOUT, g.knockout);
    flags.set(GradientFilterFlags::COMPOSITE_SOURCE, true);
    match g.placement {
        BevelPlacement::Inner => flags.insert(GradientFilterFlags::INNER_SHADOW),
        BevelPlacement::Outer => {}
        BevelPlacement::Full => flags.insert(GradientFilterFlags::ON_TOP),
    }
    swf::GradientFilter {
        colors: stops
            .iter()
            .map(|s| GradientRecord {
                ratio: u8::try_from(
                    units::round_clamped(s.ratio.clamp(0.0, 1.0) * 255.0, 255.0).max(0),
                )
                .unwrap_or(255),
                color: units::color(s.color),
            })
            .collect(),
        blur_x: blur(g.blur_x),
        blur_y: blur(g.blur_y),
        angle: angle(g.angle),
        distance: units::fixed16(g.distance),
        strength: strength(g.strength),
        flags,
    }
}

/// Contrast scale steps for positive contrast, indexed by the whole
/// contrast value 0..=100 (Flash's Adjust Color table, as in
/// `fl.motion.AdjustColor`).
const CONTRAST_STEPS: [f64; 101] = [
    0.0, 0.01, 0.02, 0.04, 0.05, 0.06, 0.07, 0.08, 0.1, 0.11, 0.12, 0.14, 0.15, 0.16, 0.17, 0.18,
    0.20, 0.21, 0.22, 0.24, 0.25, 0.27, 0.28, 0.30, 0.32, 0.34, 0.36, 0.38, 0.40, 0.42, 0.44, 0.46,
    0.48, 0.5, 0.53, 0.56, 0.59, 0.62, 0.65, 0.68, 0.71, 0.74, 0.77, 0.80, 0.83, 0.86, 0.89, 0.92,
    0.95, 0.98, 1.0, 1.06, 1.12, 1.18, 1.24, 1.30, 1.36, 1.42, 1.48, 1.54, 1.60, 1.66, 1.72, 1.78,
    1.84, 1.90, 1.96, 2.0, 2.12, 2.25, 2.37, 2.50, 2.62, 2.75, 2.87, 3.0, 3.2, 3.4, 3.6, 3.8, 4.0,
    4.3, 4.7, 4.9, 5.0, 5.5, 6.0, 6.5, 6.8, 7.0, 7.3, 7.5, 7.8, 8.0, 8.4, 8.7, 9.0, 9.4, 9.6, 9.8,
    10.0,
];

/// Flash 8's Adjust Color filter as a 4×5 colour matrix (the algorithm
/// `fl.motion.AdjustColor` documents, checked against the
/// `tween-probes/color` probe to the f32 precision of the SWF):
///
/// - brightness `b` adds `b` to each colour channel;
/// - contrast `c` scales by `n / 127` around mid grey with offset
///   `(127 − n) / 2`, where `n = 127 + 127 · STEPS[trunc(c)]` for `c > 0`
///   and `127 + 1.27 · c` otherwise;
/// - saturation `s` is a luminance-weighted (0.3086, 0.6094, 0.0820)
///   saturation matrix of scale `1 + 3s/100` (`s > 0`) or `1 + s/100`;
/// - hue `h` (degrees) is the hue rotation with weights 0.213, 0.715,
///   0.072;
///
/// applied in that order (matrix = hue × saturation × contrast ×
/// brightness). Values are clamped to the IDE's ranges.
pub fn adjust_color_matrix(a: &AdjustColorFilter) -> [f64; 20] {
    type M = [[f64; 5]; 4];
    fn mul(x: &M, y: &M) -> M {
        // Treat each as a 5x5 with an implicit [0 0 0 0 1] last row.
        let mut out = [[0.0; 5]; 4];
        for (i, row) in out.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let mut s = 0.0;
                for k in 0..4 {
                    s += x[i][k] * y[k][j];
                }
                if j == 4 {
                    s += x[i][4];
                }
                *cell = s;
            }
        }
        out
    }
    let finite = |v: f64, lo: f64, hi: f64| if v.is_finite() { v.clamp(lo, hi) } else { 0.0 };

    let b = finite(a.brightness, -100.0, 100.0);
    let bri: M = [
        [1.0, 0.0, 0.0, 0.0, b],
        [0.0, 1.0, 0.0, 0.0, b],
        [0.0, 0.0, 1.0, 0.0, b],
        [0.0, 0.0, 0.0, 1.0, 0.0],
    ];

    let c = finite(a.contrast, -100.0, 100.0);
    let n = if c > 0.0 {
        // `c` is at most 100, so the index is in range.
        CONTRAST_STEPS[(c.trunc() as usize).min(100)] * 127.0 + 127.0
    } else {
        c / 100.0 * 127.0 + 127.0
    };
    let (k, off) = (n / 127.0, 0.5 * (127.0 - n));
    let con: M = [
        [k, 0.0, 0.0, 0.0, off],
        [0.0, k, 0.0, 0.0, off],
        [0.0, 0.0, k, 0.0, off],
        [0.0, 0.0, 0.0, 1.0, 0.0],
    ];

    let s = finite(a.saturation, -100.0, 100.0);
    let s = if s > 0.0 {
        1.0 + 3.0 * s / 100.0
    } else {
        1.0 + s / 100.0
    };
    let (lr, lg, lb) = (0.3086, 0.6094, 0.0820);
    let sat: M = [
        [lr * (1.0 - s) + s, lg * (1.0 - s), lb * (1.0 - s), 0.0, 0.0],
        [lr * (1.0 - s), lg * (1.0 - s) + s, lb * (1.0 - s), 0.0, 0.0],
        [lr * (1.0 - s), lg * (1.0 - s), lb * (1.0 - s) + s, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0, 0.0],
    ];

    let (sin, cos) = finite(a.hue, -180.0, 180.0).to_radians().sin_cos();
    let (hr, hg, hb) = (0.213, 0.715, 0.072);
    let hue: M = [
        [
            hr + cos * (1.0 - hr) - sin * hr,
            hg - cos * hg - sin * hg,
            hb - cos * hb + sin * (1.0 - hb),
            0.0,
            0.0,
        ],
        [
            hr - cos * hr + sin * 0.143,
            hg + cos * (1.0 - hg) + sin * 0.140,
            hb - cos * hb - sin * 0.283,
            0.0,
            0.0,
        ],
        [
            hr - cos * hr - sin * (1.0 - hr),
            hg - cos * hg + sin * hg,
            hb + cos * (1.0 - hb) + sin * hb,
            0.0,
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0, 0.0],
    ];

    let m = mul(&hue, &mul(&sat, &mul(&con, &bri)));
    let mut out = [0.0; 20];
    for (i, row) in m.iter().enumerate() {
        out[i * 5..i * 5 + 5].copy_from_slice(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use jazari_xfl::model::{DropShadowFilter, Rgba};

    #[test]
    fn identity_adjust_colour_is_identity() {
        let m = adjust_color_matrix(&AdjustColorFilter::default());
        let id = [
            1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0,
        ];
        for (a, b) in m.iter().zip(id.iter()) {
            assert!((a - b).abs() < 1e-9, "{m:?}");
        }
    }

    #[test]
    fn adjust_colour_matches_flash_8() {
        // tests/fixtures/tween-probes/color, f_adjust's end keyframe.
        let m = adjust_color_matrix(&AdjustColorFilter {
            brightness: 50.0,
            contrast: 30.0,
            saturation: -40.0,
            hue: 60.0,
        });
        let flash = [
            0.5424, 0.1249, 0.7727, 0.0, 44.06, 0.3768, 1.197, -0.1334, 0.0, 44.06, -0.3191, 1.195,
            0.5642, 0.0, 44.06, 0.0, 0.0, 0.0, 1.0, 0.0,
        ];
        for (a, b) in m.iter().zip(flash.iter()) {
            assert!((a - b).abs() < 6e-4, "{m:?}");
        }
    }

    #[test]
    fn drop_shadow_matches_flash_8() {
        let doc = jazari_xfl::Document::default();
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, crate::BuildMode::Release, &fonts, &as2);
        let f = Filter {
            enabled: true,
            kind: FilterKind::DropShadow(DropShadowFilter {
                blur_x: 6.0,
                blur_y: 6.0,
                quality: 3,
                angle: 45.0,
                distance: 8.0,
                strength: 0.8,
                color: Rgba::opaque(0x33, 0x33, 0x33),
                ..DropShadowFilter::default()
            }),
        };
        let out = filters(&mut ctx, &[f], &Origin::default());
        let swf::Filter::DropShadowFilter(d) = &out[0] else {
            panic!("{out:?}");
        };
        assert_eq!(d.strength.get(), 204);
        assert_eq!(d.flags.bits(), 0x23, "composite + 3 passes");
        assert_eq!(d.distance.get(), 8 << 16);
    }
}
