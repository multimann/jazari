//! Filter lists in a motion tween, as Flash 8 bakes them.
//!
//! Calibrated on the `tween-probes/color` probe:
//!
//! - The two lists are aligned like a diff on the filter kinds (longest
//!   common subsequence). Matched filters interpolate; a filter only in the
//!   start list fades out towards a neutral copy of itself, one only in the
//!   end list fades in from one (blur, strength and distance 0; colours,
//!   angle and flags kept). Unmatched start filters come before unmatched
//!   end filters at the same place, so `[Blur, Glow] → [Glow, Blur]`
//!   becomes `[Blur → 0, Glow → Glow, 0 → Blur]`.
//! - Blur, distance and angle (degrees) interpolate linearly.
//! - Strength interpolates in whole percent, truncated (the IDE stores
//!   integer percentages): `trunc(100·s₀ + (100·s₁ − 100·s₀)·q) / 100`.
//! - Colour channels (alpha included) move by a truncated delta:
//!   `c₀ + trunc((c₁ − c₀)·q)`.
//! - Quality (blur passes) moves by a truncated delta too.
//! - Flags (inner, knockout, hide object, bevel placement) keep the start
//!   values until the end keyframe.
//! - Adjust Color interpolates its four parameters linearly, without
//!   rounding; the matrix (`crate::filter`) truncates the contrast to a
//!   whole value for its table. Gradient stops interpolate when both lists have the
//!   same number of stops; colour matrices and convolutions interpolate
//!   their numbers when the sizes match (no probe covers these three).

use jazari_xfl::model::{
    AdjustColorFilter, BevelFilter, BlurFilter, ColorMatrixFilter, ConvolutionFilter,
    DropShadowFilter, Filter, FilterKind, GlowFilter, GradientFilter, GradientStop, Rgba,
};

/// Most filters a list may have before alignment gives up (Flash allows
/// far fewer; this bounds the alignment table on hostile files).
const MAX_FILTERS: usize = 64;

fn lerp(a: f64, b: f64, q: f64) -> f64 {
    a + (b - a) * q
}

/// `trunc` that maps non-finite values to 0.
fn trunc(v: f64) -> f64 {
    if v.is_finite() { v.trunc() } else { 0.0 }
}

fn strength(a: f64, b: f64, q: f64) -> f64 {
    trunc(lerp(a * 100.0, b * 100.0, q)) / 100.0
}

fn channel(a: u8, b: u8, q: f64) -> u8 {
    let v = f64::from(a) + trunc((f64::from(b) - f64::from(a)) * q);
    v.clamp(0.0, 255.0) as u8
}

fn color(a: Rgba, b: Rgba, q: f64) -> Rgba {
    Rgba::new(
        channel(a.r, b.r, q),
        channel(a.g, b.g, q),
        channel(a.b, b.b, q),
        channel(a.a, b.a, q),
    )
}

fn quality(a: u8, b: u8, q: f64) -> u8 {
    let v = f64::from(a) + trunc((f64::from(b) - f64::from(a)) * q);
    v.clamp(0.0, 255.0) as u8
}

/// The same kind of filter, for alignment.
fn same_kind(a: &FilterKind, b: &FilterKind) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// The filter a fading filter starts from or ends at.
fn neutral(f: &FilterKind) -> FilterKind {
    match f {
        FilterKind::DropShadow(x) => FilterKind::DropShadow(DropShadowFilter {
            blur_x: 0.0,
            blur_y: 0.0,
            distance: 0.0,
            strength: 0.0,
            ..*x
        }),
        FilterKind::Blur(x) => FilterKind::Blur(BlurFilter {
            blur_x: 0.0,
            blur_y: 0.0,
            ..*x
        }),
        FilterKind::Glow(x) => FilterKind::Glow(GlowFilter {
            blur_x: 0.0,
            blur_y: 0.0,
            strength: 0.0,
            ..*x
        }),
        FilterKind::Bevel(x) => FilterKind::Bevel(BevelFilter {
            blur_x: 0.0,
            blur_y: 0.0,
            distance: 0.0,
            strength: 0.0,
            ..*x
        }),
        FilterKind::GradientGlow(x) => FilterKind::GradientGlow(neutral_gradient(x)),
        FilterKind::GradientBevel(x) => FilterKind::GradientBevel(neutral_gradient(x)),
        FilterKind::AdjustColor(_) => FilterKind::AdjustColor(AdjustColorFilter::default()),
        FilterKind::ColorMatrix(_) => FilterKind::ColorMatrix(ColorMatrixFilter::default()),
        FilterKind::Convolution(x) => FilterKind::Convolution(x.clone()),
    }
}

fn neutral_gradient(x: &GradientFilter) -> GradientFilter {
    GradientFilter {
        blur_x: 0.0,
        blur_y: 0.0,
        distance: 0.0,
        strength: 0.0,
        ..x.clone()
    }
}

fn gradient(x: &GradientFilter, y: &GradientFilter, q: f64) -> GradientFilter {
    let stops = if x.stops.len() == y.stops.len() {
        x.stops
            .iter()
            .zip(&y.stops)
            .map(|(s, t)| GradientStop {
                ratio: lerp(s.ratio, t.ratio, q),
                color: color(s.color, t.color, q),
            })
            .collect()
    } else {
        x.stops.clone()
    };
    GradientFilter {
        blur_x: lerp(x.blur_x, y.blur_x, q),
        blur_y: lerp(x.blur_y, y.blur_y, q),
        quality: quality(x.quality, y.quality, q),
        angle: lerp(x.angle, y.angle, q),
        distance: lerp(x.distance, y.distance, q),
        strength: strength(x.strength, y.strength, q),
        stops,
        ..x.clone()
    }
}

/// Interpolates two filters of the same kind; `None` for different kinds.
fn interpolate(a: &FilterKind, b: &FilterKind, q: f64) -> Option<FilterKind> {
    Some(match (a, b) {
        (FilterKind::DropShadow(x), FilterKind::DropShadow(y)) => {
            FilterKind::DropShadow(DropShadowFilter {
                blur_x: lerp(x.blur_x, y.blur_x, q),
                blur_y: lerp(x.blur_y, y.blur_y, q),
                quality: quality(x.quality, y.quality, q),
                angle: lerp(x.angle, y.angle, q),
                distance: lerp(x.distance, y.distance, q),
                strength: strength(x.strength, y.strength, q),
                color: color(x.color, y.color, q),
                ..*x
            })
        }
        (FilterKind::Blur(x), FilterKind::Blur(y)) => FilterKind::Blur(BlurFilter {
            blur_x: lerp(x.blur_x, y.blur_x, q),
            blur_y: lerp(x.blur_y, y.blur_y, q),
            quality: quality(x.quality, y.quality, q),
        }),
        (FilterKind::Glow(x), FilterKind::Glow(y)) => FilterKind::Glow(GlowFilter {
            blur_x: lerp(x.blur_x, y.blur_x, q),
            blur_y: lerp(x.blur_y, y.blur_y, q),
            quality: quality(x.quality, y.quality, q),
            strength: strength(x.strength, y.strength, q),
            color: color(x.color, y.color, q),
            ..*x
        }),
        (FilterKind::Bevel(x), FilterKind::Bevel(y)) => FilterKind::Bevel(BevelFilter {
            blur_x: lerp(x.blur_x, y.blur_x, q),
            blur_y: lerp(x.blur_y, y.blur_y, q),
            quality: quality(x.quality, y.quality, q),
            angle: lerp(x.angle, y.angle, q),
            distance: lerp(x.distance, y.distance, q),
            strength: strength(x.strength, y.strength, q),
            shadow_color: color(x.shadow_color, y.shadow_color, q),
            highlight_color: color(x.highlight_color, y.highlight_color, q),
            ..*x
        }),
        (FilterKind::GradientGlow(x), FilterKind::GradientGlow(y)) => {
            FilterKind::GradientGlow(gradient(x, y, q))
        }
        (FilterKind::GradientBevel(x), FilterKind::GradientBevel(y)) => {
            FilterKind::GradientBevel(gradient(x, y, q))
        }
        (FilterKind::AdjustColor(x), FilterKind::AdjustColor(y)) => {
            FilterKind::AdjustColor(AdjustColorFilter {
                brightness: lerp(x.brightness, y.brightness, q),
                contrast: lerp(x.contrast, y.contrast, q),
                saturation: lerp(x.saturation, y.saturation, q),
                hue: lerp(x.hue, y.hue, q),
            })
        }
        (FilterKind::ColorMatrix(x), FilterKind::ColorMatrix(y)) => {
            let mut matrix = x.matrix;
            for (m, (s, t)) in matrix.iter_mut().zip(x.matrix.iter().zip(&y.matrix)) {
                *m = lerp(*s, *t, q);
            }
            FilterKind::ColorMatrix(ColorMatrixFilter { matrix })
        }
        (FilterKind::Convolution(x), FilterKind::Convolution(y)) => {
            if x.matrix.len() != y.matrix.len() {
                FilterKind::Convolution(x.clone())
            } else {
                FilterKind::Convolution(ConvolutionFilter {
                    matrix: x
                        .matrix
                        .iter()
                        .zip(&y.matrix)
                        .map(|(s, t)| lerp(*s, *t, q))
                        .collect(),
                    divisor: lerp(x.divisor, y.divisor, q),
                    bias: lerp(x.bias, y.bias, q),
                    color: color(x.color, y.color, q),
                    ..x.clone()
                })
            }
        }
        _ => return None,
    })
}

/// One entry of the aligned lists.
enum Step<'f> {
    /// In both lists.
    Both(&'f Filter, &'f Filter),
    /// Only in the start list (fades out).
    Start(&'f Filter),
    /// Only in the end list (fades in).
    End(&'f Filter),
}

/// Aligns two filter lists by kind (longest common subsequence, removals
/// before insertions).
fn align<'f>(a: &'f [Filter], b: &'f [Filter]) -> Vec<Step<'f>> {
    let (n, m) = (a.len(), b.len());
    // lcs[i][j] = LCS length of a[i..] and b[j..].
    let mut lcs = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if same_kind(&a[i].kind, &b[j].kind) {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(n + m);
    while i < n || j < m {
        if i < n && j < m && same_kind(&a[i].kind, &b[j].kind) && lcs[i][j] == lcs[i + 1][j + 1] + 1
        {
            out.push(Step::Both(&a[i], &b[j]));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push(Step::Start(&a[i]));
            i += 1;
        } else {
            out.push(Step::End(&b[j]));
            j += 1;
        }
    }
    out
}

/// The filter list `q` of the way from `a` to `b`.
pub(super) fn interpolate_list(a: &[Filter], b: &[Filter], q: f64) -> Vec<Filter> {
    if a.len() > MAX_FILTERS || b.len() > MAX_FILTERS {
        return a.to_vec();
    }
    align(a, b)
        .into_iter()
        .map(|step| match step {
            Step::Both(x, y) => Filter {
                enabled: x.enabled,
                kind: interpolate(&x.kind, &y.kind, q).unwrap_or_else(|| x.kind.clone()),
            },
            Step::Start(x) => Filter {
                enabled: x.enabled,
                kind: interpolate(&x.kind, &neutral(&x.kind), q).unwrap_or_else(|| x.kind.clone()),
            },
            Step::End(y) => Filter {
                enabled: y.enabled,
                kind: interpolate(&neutral(&y.kind), &y.kind, q).unwrap_or_else(|| y.kind.clone()),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blur(x: f64) -> Filter {
        Filter {
            enabled: true,
            kind: FilterKind::Blur(BlurFilter {
                blur_x: x,
                blur_y: x,
                quality: 1,
            }),
        }
    }

    fn glow(x: f64, strength: f64, color: Rgba) -> Filter {
        Filter {
            enabled: true,
            kind: FilterKind::Glow(GlowFilter {
                blur_x: x,
                blur_y: x,
                strength,
                color,
                ..GlowFilter::default()
            }),
        }
    }

    fn blur_of(f: &Filter) -> f64 {
        match &f.kind {
            FilterKind::Blur(b) => b.blur_x,
            FilterKind::Glow(g) => g.blur_x,
            _ => f64::NAN,
        }
    }

    #[test]
    fn reordered_lists_fade_like_flash_8() {
        // tween-probes/color, layer f_order: [Blur 2, Glow 6] -> [Glow 18,
        // Blur 10] is [Blur 2 -> 0, Glow 6 -> 18, Blur 0 -> 10].
        let magenta = Rgba::opaque(255, 0, 255);
        let a = [blur(2.0), glow(6.0, 1.0, magenta)];
        let b = [glow(18.0, 1.0, magenta), blur(10.0)];
        let mid = interpolate_list(&a, &b, 0.5);
        let blurs: Vec<f64> = mid.iter().map(blur_of).collect();
        assert_eq!(blurs, [1.0, 12.0, 5.0]);
        assert!(matches!(mid[1].kind, FilterKind::Glow(_)));
    }

    #[test]
    fn added_filters_fade_in_from_nothing() {
        // f_add: none -> Glow(16, 200 %, #00AA00); Flash 8's frame 1 has
        // strength 10 % (25/256) and blur 0.8.
        let green = Rgba::opaque(0, 170, 0);
        let q = 3277.0 / 65536.0;
        let out = interpolate_list(&[], &[glow(16.0, 2.0, green)], q);
        let FilterKind::Glow(g) = &out[0].kind else {
            panic!("{out:?}")
        };
        assert_eq!(g.strength, 0.1);
        assert_eq!(g.color, green);
        assert!((g.blur_x - 0.80005).abs() < 1e-4);
    }

    #[test]
    fn strengths_and_colours_truncate_like_flash_8() {
        // f_drop frame 1: strength 100 % -> 50 % gives 97 %, colour
        // #000000/255 -> #FF0000/128 gives #0C0000/249.
        let q = 3277.0 / 65536.0;
        assert_eq!(strength(1.0, 0.5, q), 0.97);
        assert_eq!(
            color(Rgba::BLACK, Rgba::new(255, 0, 0, 128), q),
            Rgba::new(12, 0, 0, 249)
        );
        assert_eq!(quality(1, 3, 0.45), 1);
        assert_eq!(quality(1, 3, 0.5), 2);
    }

    #[test]
    fn hostile_lists_are_bounded() {
        let many: Vec<Filter> = (0..100).map(|i| blur(f64::from(i))).collect();
        assert_eq!(interpolate_list(&many, &[], 0.5).len(), 100);
        let nan = [blur(f64::NAN)];
        assert_eq!(interpolate_list(&nan, &[blur(1.0)], 0.5).len(), 1);
    }
}
