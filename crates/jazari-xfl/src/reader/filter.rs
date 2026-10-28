//! Instance filters (`<filters>`), with the defaults of
//! `docs/research/xfl-format.md` §5 and §11.

use crate::WarningKind;
use crate::model::{
    AdjustColorFilter, BevelFilter, BevelPlacement, BlurFilter, ColorMatrixFilter,
    ConvolutionFilter, DropShadowFilter, Filter, FilterKind, GlowFilter, GradientFilter,
    GradientStop, Rgba,
};

use super::ctx::Ctx;
use super::values::{excerpt, parse_f64};
use super::xml::Node;

/// Reads the children of a `<filters>` element. Unknown filters are skipped
/// with a warning.
pub(crate) fn read_filters(ctx: &mut Ctx, container: &Node) -> Vec<Filter> {
    let mut out = Vec::new();
    for node in &container.children {
        let kind = match node.name.as_str() {
            "DropShadowFilter" => FilterKind::DropShadow(drop_shadow(ctx, node)),
            "BlurFilter" => FilterKind::Blur(BlurFilter {
                blur_x: ctx.f64(node, "blurX", 5.0),
                blur_y: ctx.f64(node, "blurY", 5.0),
                quality: quality(ctx, node),
            }),
            "GlowFilter" => FilterKind::Glow(glow(ctx, node)),
            "BevelFilter" => FilterKind::Bevel(bevel(ctx, node)),
            "GradientGlowFilter" => FilterKind::GradientGlow(gradient_filter(ctx, node)),
            "GradientBevelFilter" => FilterKind::GradientBevel(gradient_filter(ctx, node)),
            "AdjustColorFilter" => FilterKind::AdjustColor(AdjustColorFilter {
                brightness: ctx.f64(node, "brightness", 0.0),
                contrast: ctx.f64(node, "contrast", 0.0),
                saturation: ctx.f64(node, "saturation", 0.0),
                hue: ctx.f64(node, "hue", 0.0),
            }),
            "ColorMatrixFilter" => FilterKind::ColorMatrix(color_matrix(ctx, node)),
            "ConvolutionFilter" => FilterKind::Convolution(convolution(ctx, node)),
            other => {
                ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown filter <{other}> was skipped"),
                );
                continue;
            }
        };
        out.push(Filter {
            enabled: ctx.bool(node, "isEnabled", true),
            kind,
        });
    }
    out
}

/// `quality`: blur passes, clamped to 1..=3.
fn quality(ctx: &mut Ctx, node: &Node) -> u8 {
    // Clamped to 1..=3, so the cast cannot truncate.
    ctx.u32(node, "quality", 1).clamp(1, 3) as u8
}

fn drop_shadow(ctx: &mut Ctx, node: &Node) -> DropShadowFilter {
    DropShadowFilter {
        blur_x: ctx.f64(node, "blurX", 5.0),
        blur_y: ctx.f64(node, "blurY", 5.0),
        quality: quality(ctx, node),
        angle: ctx.f64(node, "angle", 45.0),
        distance: ctx.f64(node, "distance", 5.0),
        strength: ctx.f64(node, "strength", 1.0),
        color: ctx.color_alpha(node, "color", "alpha", Rgba::BLACK),
        knockout: ctx.bool(node, "knockout", false),
        inner: ctx.bool(node, "inner", false),
        hide_object: ctx.bool(node, "hideObject", false),
    }
}

fn glow(ctx: &mut Ctx, node: &Node) -> GlowFilter {
    GlowFilter {
        blur_x: ctx.f64(node, "blurX", 5.0),
        blur_y: ctx.f64(node, "blurY", 5.0),
        quality: quality(ctx, node),
        strength: ctx.f64(node, "strength", 1.0),
        // The default colour is disputed; red until a fixture decides (§11).
        color: ctx.color_alpha(node, "color", "alpha", Rgba::opaque(255, 0, 0)),
        knockout: ctx.bool(node, "knockout", false),
        inner: ctx.bool(node, "inner", false),
    }
}

/// `type`: inner, outer or full.
fn placement(ctx: &mut Ctx, node: &Node) -> BevelPlacement {
    match node.attr("type") {
        None | Some("inner") => BevelPlacement::Inner,
        Some("outer") => BevelPlacement::Outer,
        Some("full") => BevelPlacement::Full,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "<{}> type=\"{}\" is unknown; inner is used",
                    node.name,
                    excerpt(other)
                ),
            );
            BevelPlacement::Inner
        }
    }
}

fn bevel(ctx: &mut Ctx, node: &Node) -> BevelFilter {
    BevelFilter {
        blur_x: ctx.f64(node, "blurX", 5.0),
        blur_y: ctx.f64(node, "blurY", 5.0),
        quality: quality(ctx, node),
        angle: ctx.f64(node, "angle", 45.0),
        distance: ctx.f64(node, "distance", 5.0),
        strength: ctx.f64(node, "strength", 1.0),
        shadow_color: ctx.color_alpha(node, "shadowColor", "shadowAlpha", Rgba::BLACK),
        highlight_color: ctx.color_alpha(node, "highlightColor", "highlightAlpha", Rgba::WHITE),
        knockout: ctx.bool(node, "knockout", false),
        placement: placement(ctx, node),
    }
}

fn gradient_filter(ctx: &mut Ctx, node: &Node) -> GradientFilter {
    let stops = node
        .children_named("GradientEntry")
        .map(|e| GradientStop {
            ratio: ctx.f64(e, "ratio", 0.0),
            color: ctx.color_alpha(e, "color", "alpha", Rgba::BLACK),
        })
        .collect();
    GradientFilter {
        blur_x: ctx.f64(node, "blurX", 5.0),
        blur_y: ctx.f64(node, "blurY", 5.0),
        quality: quality(ctx, node),
        angle: ctx.f64(node, "angle", 45.0),
        distance: ctx.f64(node, "distance", 5.0),
        strength: ctx.f64(node, "strength", 1.0),
        stops,
        knockout: ctx.bool(node, "knockout", false),
        placement: placement(ctx, node),
    }
}

/// A comma- (or space-) separated list of numbers, or `None` if any is bad.
fn number_list(s: &str) -> Option<Vec<f64>> {
    s.split([',', ' ', '\t', '\n', '\r'])
        .filter(|t| !t.is_empty())
        .map(parse_f64)
        .collect()
}

fn color_matrix(ctx: &mut Ctx, node: &Node) -> ColorMatrixFilter {
    let mut filter = ColorMatrixFilter::default();
    if let Some(text) = node.attr("matrix") {
        match number_list(text) {
            Some(values) if values.len() == 20 => filter.matrix.copy_from_slice(&values),
            _ => ctx.warn(
                WarningKind::BadValue,
                format!(
                    "<ColorMatrixFilter> matrix=\"{}\" is not 20 numbers; the identity is used",
                    excerpt(text)
                ),
            ),
        }
    }
    filter
}

fn convolution(ctx: &mut Ctx, node: &Node) -> ConvolutionFilter {
    let mut filter = ConvolutionFilter::default();
    // Clamped to the SWF field range, so the casts cannot truncate.
    let mx = ctx.u32(node, "matrixX", 3).clamp(1, 255) as u8;
    let my = ctx.u32(node, "matrixY", 3).clamp(1, 255) as u8;
    if let Some(text) = node.attr("matrix") {
        match number_list(text) {
            Some(values) if values.len() == usize::from(mx) * usize::from(my) => {
                filter.matrix_x = mx;
                filter.matrix_y = my;
                filter.matrix = values;
            }
            _ => ctx.warn(
                WarningKind::BadValue,
                format!(
                    "<ConvolutionFilter> matrix=\"{}\" does not hold {mx} × {my} numbers; \
                     the identity kernel is used",
                    excerpt(text)
                ),
            ),
        }
    }
    filter.divisor = ctx.f64(node, "divisor", 1.0);
    filter.bias = ctx.f64(node, "bias", 0.0);
    filter.preserve_alpha = ctx.bool(node, "preserveAlpha", true);
    filter.clamp = ctx.bool(node, "clamp", true);
    filter.color = ctx.color_alpha(node, "color", "alpha", Rgba::BLACK);
    if !node.has_attr("color") && !node.has_attr("alpha") {
        filter.color = Rgba::TRANSPARENT;
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::xml::parse_str;

    fn filters(xml: &str) -> (Vec<Filter>, usize) {
        let mut ctx = Ctx::default();
        let f = read_filters(&mut ctx, &parse_str(xml).unwrap());
        (f, ctx.report.len())
    }

    #[test]
    fn fixture_filters() {
        let (f, w) = filters(
            r##"<filters>
                <DropShadowFilter alpha="1" angle="45.00000125223908" blurX="6" blurY="6" color="#333333" distance="8" hideObject="false" inner="false" isEnabled="true" knockout="false" quality="3" strength="0.8"/>
                <BlurFilter blurX="10" blurY="4" isEnabled="true" quality="2"/>
                <GlowFilter alpha="1" blurX="12" blurY="12" color="#FF0000" inner="false" isEnabled="false" knockout="false" quality="1" strength="2"/>
                <BevelFilter angle="45" blurX="5" blurY="5" distance="5" highlightAlpha="1" highlightColor="#FFFFFF" isEnabled="true" knockout="false" quality="1" shadowAlpha="0.5" shadowColor="#000000" strength="1" type="inner"/>
            </filters>"##,
        );
        assert_eq!(w, 0);
        assert_eq!(f.len(), 4);
        let FilterKind::DropShadow(d) = &f[0].kind else {
            panic!("drop shadow expected")
        };
        assert_eq!((d.blur_x, d.quality, d.distance), (6.0, 3, 8.0));
        assert_eq!(d.strength, 0.8);
        assert_eq!(d.color, Rgba::opaque(0x33, 0x33, 0x33));
        assert!((d.angle - 45.0).abs() < 1e-5);
        assert!(matches!(
            f[1].kind,
            FilterKind::Blur(BlurFilter {
                blur_x: 10.0,
                blur_y: 4.0,
                quality: 2
            })
        ));
        assert!(!f[2].enabled, "disabled filters are kept");
        let FilterKind::Bevel(b) = &f[3].kind else {
            panic!("bevel expected")
        };
        assert_eq!(b.shadow_color, Rgba::new(0, 0, 0, 128));
        assert_eq!(b.placement, BevelPlacement::Inner);
    }

    #[test]
    fn defaults_and_extras() {
        let (f, w) = filters(
            r##"<filters>
                <GlowFilter/>
                <GradientGlowFilter type="outer"><GradientEntry color="#FFFFFF" alpha="0" ratio="0"/><GradientEntry color="#FF0000" ratio="1"/></GradientGlowFilter>
                <ColorMatrixFilter matrix="1,0,0,0,0, 0,1,0,0,0, 0,0,1,0,0, 0,0,0,0.5,0"/>
                <ConvolutionFilter matrixX="1" matrixY="1" matrix="2" divisor="2"/>
                <AdjustColorFilter brightness="10" hue="-180"/>
                <SparkleFilter/>
            </filters>"##,
        );
        assert_eq!(w, 1, "the unknown filter");
        assert_eq!(f.len(), 5);
        let FilterKind::Glow(g) = &f[0].kind else {
            panic!()
        };
        assert_eq!(g.color, Rgba::opaque(255, 0, 0));
        assert_eq!((g.blur_x, g.quality, g.strength), (5.0, 1, 1.0));
        let FilterKind::GradientGlow(gg) = &f[1].kind else {
            panic!()
        };
        assert_eq!(gg.stops.len(), 2);
        assert_eq!(gg.stops[0].color, Rgba::new(255, 255, 255, 0));
        assert_eq!(gg.placement, BevelPlacement::Outer);
        let FilterKind::ColorMatrix(m) = &f[2].kind else {
            panic!()
        };
        assert_eq!(m.matrix[18], 0.5);
        let FilterKind::Convolution(c) = &f[3].kind else {
            panic!()
        };
        assert_eq!(
            (c.matrix_x, c.matrix.clone(), c.divisor),
            (1, vec![2.0], 2.0)
        );
        assert_eq!(c.color, Rgba::TRANSPARENT);
        let FilterKind::AdjustColor(a) = &f[4].kind else {
            panic!()
        };
        assert_eq!((a.brightness, a.hue), (10.0, -180.0));
    }

    #[test]
    fn bad_matrices_fall_back() {
        let (f, w) = filters(r#"<filters><ColorMatrixFilter matrix="1,2,3"/></filters>"#);
        assert_eq!(w, 1);
        assert_eq!(
            f[0].kind,
            FilterKind::ColorMatrix(ColorMatrixFilter::default())
        );
    }
}
