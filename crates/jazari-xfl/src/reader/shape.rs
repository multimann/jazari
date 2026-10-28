//! Shapes (`DOMShape`), their fill and stroke styles, and the CS4+ primitive
//! objects (`DOMRectangleObject`, `DOMOvalObject`), whose outlines are
//! rebuilt from their parameters.

use crate::WarningKind;
use crate::model::{
    BitmapFill, Caps, Edge, FillStyle, Gradient, GradientStop, Interp, Joints, PathCmd, Pt, Rgba,
    ScaleMode, Shape, Spread, StrokeStyle,
};

use super::ctx::Ctx;
use super::edges::parse_edges;
use super::values::excerpt;
use super::xml::Node;

/// Stroke element names. `SolidStroke` is published as is; the others are
/// published as solid lines with a warning.
const STROKE_KINDS: [&str; 6] = [
    "SolidStroke",
    "DashedStroke",
    "DottedStroke",
    "RaggedStroke",
    "StippleStroke",
    "HatchedStroke",
];

/// Maps XFL style indices (`FillStyle@index`) to 1-based positions.
struct StyleMap(Vec<u32>);

impl StyleMap {
    /// The 1-based position of XFL index `index`, 0 for "none".
    fn position(&self, ctx: &mut Ctx, attr: &str, index: u32) -> u32 {
        if index == 0 {
            return 0;
        }
        match self.0.iter().position(|&i| i == index) {
            // Positions are bounded by the number of styles in one shape.
            Some(p) => u32::try_from(p + 1).unwrap_or(0),
            None => {
                ctx.warn(
                    WarningKind::BadValue,
                    format!("<Edge> {attr}=\"{index}\" names no style of this shape; none is used"),
                );
                0
            }
        }
    }
}

/// Reads a `DOMShape`.
pub(crate) fn read_shape(ctx: &mut Ctx, node: &Node) -> Shape {
    ctx.check_children(
        node,
        &["matrix", "transformationPoint", "fills", "strokes", "edges"],
    );
    let matrix = ctx.matrix(node).unwrap_or_default();
    let (fills, fill_map) = read_fill_styles(ctx, node.child("fills"));
    let (strokes, stroke_map) = read_stroke_styles(ctx, node.child("strokes"));
    let mut edges = Vec::new();
    if let Some(container) = node.child("edges") {
        for e in &container.children {
            if e.name != "Edge" {
                ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown <{}> inside <edges> was skipped", e.name),
                );
                continue;
            }
            let f0 = ctx.u32(e, "fillStyle0", 0);
            let f1 = ctx.u32(e, "fillStyle1", 0);
            let st = ctx.u32(e, "strokeStyle", 0);
            let path = match e.attr("edges") {
                Some(text) => match parse_edges(text) {
                    Ok(path) => path,
                    Err(err) => {
                        ctx.warn(
                            WarningKind::BadValue,
                            format!(
                                "<Edge> edges: {} at byte {} of \"{}\"; the rest of the edge is skipped",
                                err.message,
                                err.offset,
                                excerpt(text)
                            ),
                        );
                        err.partial
                    }
                },
                None => {
                    if e.has_attr("cubics") {
                        ctx.warn(
                            WarningKind::Unsupported,
                            "an <Edge> with only cubic curves (no edges attribute) was skipped",
                        );
                    }
                    Vec::new()
                }
            };
            edges.push(Edge {
                fill0: fill_map.position(ctx, "fillStyle0", f0),
                fill1: fill_map.position(ctx, "fillStyle1", f1),
                stroke: stroke_map.position(ctx, "strokeStyle", st),
                path,
            });
        }
    }
    Shape {
        matrix,
        fills,
        strokes,
        edges,
        is_drawing_object: ctx.bool(node, "isDrawingObject", false),
    }
}

/// Reads `<fills>`: `FillStyle index=N` wrappers around one fill each.
fn read_fill_styles(ctx: &mut Ctx, container: Option<&Node>) -> (Vec<FillStyle>, StyleMap) {
    let mut fills = Vec::new();
    let mut map = Vec::new();
    for (pos, wrapper) in container
        .map(|c| c.children.as_slice())
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        if wrapper.name != "FillStyle" {
            ctx.warn(
                WarningKind::UnknownElement,
                format!("unknown <{}> inside <fills> was skipped", wrapper.name),
            );
            continue;
        }
        let index = style_index(ctx, wrapper, pos);
        if map.contains(&index) {
            ctx.warn(
                WarningKind::BadValue,
                format!("two fill styles have index {index}; edges use the first"),
            );
        }
        map.push(index);
        fills.push(read_fill_in(ctx, wrapper));
    }
    (fills, StyleMap(map))
}

/// Reads `<strokes>`: `StrokeStyle index=N` wrappers around one stroke each.
fn read_stroke_styles(ctx: &mut Ctx, container: Option<&Node>) -> (Vec<StrokeStyle>, StyleMap) {
    let mut strokes = Vec::new();
    let mut map = Vec::new();
    for (pos, wrapper) in container
        .map(|c| c.children.as_slice())
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        if wrapper.name != "StrokeStyle" {
            ctx.warn(
                WarningKind::UnknownElement,
                format!("unknown <{}> inside <strokes> was skipped", wrapper.name),
            );
            continue;
        }
        let index = style_index(ctx, wrapper, pos);
        if map.contains(&index) {
            ctx.warn(
                WarningKind::BadValue,
                format!("two stroke styles have index {index}; edges use the first"),
            );
        }
        map.push(index);
        strokes.push(read_stroke_in(ctx, wrapper));
    }
    (strokes, StyleMap(map))
}

/// `index` of a style wrapper, or its 1-based position when absent.
fn style_index(ctx: &mut Ctx, wrapper: &Node, pos: usize) -> u32 {
    let fallback = u32::try_from(pos + 1).unwrap_or(u32::MAX);
    ctx.u32(wrapper, "index", fallback)
}

/// The fill inside a container (`FillStyle`, a stroke's `<fill>`, or a
/// primitive's `<fill>`). Opaque black when there is none.
pub(crate) fn read_fill_in(ctx: &mut Ctx, container: &Node) -> FillStyle {
    let Some(node) = container.children.first() else {
        return FillStyle::default();
    };
    read_fill(ctx, node).unwrap_or_else(|| {
        ctx.warn(
            WarningKind::UnknownElement,
            format!("unknown fill <{}>; solid black is used", node.name),
        );
        FillStyle::default()
    })
}

/// Reads one fill element, or `None` when the element is not a fill.
fn read_fill(ctx: &mut Ctx, node: &Node) -> Option<FillStyle> {
    Some(match node.name.as_str() {
        "SolidColor" => FillStyle::Solid(ctx.color_alpha(node, "color", "alpha", Rgba::BLACK)),
        "LinearGradient" => FillStyle::Linear(read_gradient(ctx, node)),
        "RadialGradient" => FillStyle::Radial(read_gradient(ctx, node)),
        "BitmapFill" | "ClippedBitmapFill" => {
            let clipped_default = node.name == "ClippedBitmapFill";
            let library_item = Ctx::text(node, "bitmapPath").unwrap_or_default();
            if library_item.is_empty() {
                ctx.warn(WarningKind::MissingItem, "a bitmap fill names no bitmap");
            }
            FillStyle::Bitmap(BitmapFill {
                library_item,
                matrix: ctx.matrix(node).unwrap_or_default(),
                // Absent means repeating (§11), despite the schema default.
                clipped: ctx.bool(node, "bitmapIsClipped", clipped_default),
            })
        }
        _ => return None,
    })
}

/// Reads a linear or radial gradient.
fn read_gradient(ctx: &mut Ctx, node: &Node) -> Gradient {
    let stops = node
        .children_named("GradientEntry")
        .map(|e| GradientStop {
            ratio: ctx.f64(e, "ratio", 0.0).clamp(0.0, 1.0),
            color: ctx.color_alpha(e, "color", "alpha", Rgba::BLACK),
        })
        .collect();
    let spread = match node.attr("spreadMethod") {
        None | Some("pad") => Spread::Pad,
        Some("reflect") => Spread::Reflect,
        Some("repeat") => Spread::Repeat,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "spreadMethod=\"{}\" is unknown; pad is used",
                    excerpt(other)
                ),
            );
            Spread::Pad
        }
    };
    let interpolation = match node.attr("interpolationMethod") {
        None | Some("rgb") => Interp::Rgb,
        Some("linearRGB") => Interp::LinearRgb,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "interpolationMethod=\"{}\" is unknown; rgb is used",
                    excerpt(other)
                ),
            );
            Interp::Rgb
        }
    };
    Gradient {
        matrix: ctx.matrix(node).unwrap_or_default(),
        stops,
        spread,
        interpolation,
        focal_point: ctx.f64(node, "focalPointRatio", 0.0).clamp(-1.0, 1.0),
    }
}

/// The stroke inside a container (`StrokeStyle` or a primitive's `<stroke>`).
fn read_stroke_in(ctx: &mut Ctx, container: &Node) -> StrokeStyle {
    match container.children.first() {
        Some(node) if STROKE_KINDS.contains(&node.name.as_str()) => read_stroke(ctx, node),
        Some(node) => {
            ctx.warn(
                WarningKind::UnknownElement,
                format!("unknown stroke <{}>; a 1 px black line is used", node.name),
            );
            StrokeStyle::default()
        }
        None => StrokeStyle::default(),
    }
}

/// Reads one stroke element (`SolidStroke`, `DashedStroke`, …).
fn read_stroke(ctx: &mut Ctx, node: &Node) -> StrokeStyle {
    if node.name != "SolidStroke" {
        ctx.warn(
            WarningKind::Unsupported,
            format!("<{}> is published as a solid line", node.name),
        );
    }
    if node.child("VariablePointWidth").is_some() {
        ctx.warn(
            WarningKind::AnimateFeature,
            "a variable-width stroke is published with a constant width",
        );
    }
    let caps = match node.attr("caps") {
        None | Some("round") => Caps::Round,
        Some("none") => Caps::None,
        Some("square") => Caps::Square,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!("caps=\"{}\" is unknown; round is used", excerpt(other)),
            );
            Caps::Round
        }
    };
    let joints = match node.attr("joints") {
        None | Some("round") => Joints::Round,
        Some("miter") => Joints::Miter,
        Some("bevel") => Joints::Bevel,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!("joints=\"{}\" is unknown; round is used", excerpt(other)),
            );
            Joints::Round
        }
    };
    let scale_mode = match node.attr("scaleMode") {
        // Absent is read as normal (§11), not the schema's "none".
        None | Some("normal") => ScaleMode::Normal,
        Some("horizontal") => ScaleMode::Horizontal,
        Some("vertical") => ScaleMode::Vertical,
        Some("none") => ScaleMode::None,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "scaleMode=\"{}\" is unknown; normal is used",
                    excerpt(other)
                ),
            );
            ScaleMode::Normal
        }
    };
    let fill = match node.child("fill") {
        Some(f) => read_fill_in(ctx, f),
        None => FillStyle::default(),
    };
    StrokeStyle {
        weight: ctx.f64(node, "weight", 1.0).max(0.0),
        fill,
        caps,
        joints,
        miter_limit: ctx.f64(node, "miterLimit", 3.0),
        scale_mode,
        pixel_hinting: ctx.bool(node, "pixelHinting", false),
        hairline: node.attr("solidStyle") == Some("hairline"),
        kind: node.name.clone(),
    }
}

/// The single fill and stroke of a primitive object (`<fill>`, `<stroke>`);
/// each list is empty when the primitive has none.
fn primitive_styles(ctx: &mut Ctx, node: &Node) -> (Vec<FillStyle>, Vec<StrokeStyle>) {
    let fills = node
        .child("fill")
        .filter(|f| !f.children.is_empty())
        .map(|f| vec![read_fill_in(ctx, f)])
        .unwrap_or_default();
    let strokes = node
        .child("stroke")
        .filter(|s| !s.children.is_empty())
        .map(|s| vec![read_stroke_in(ctx, s)])
        .unwrap_or_default();
    (fills, strokes)
}

/// Pixels to twips.
fn tw(px: f64) -> f64 {
    px * 20.0
}

/// Reads a `DOMRectangleObject` into a shape with one closed edge.
pub(crate) fn read_rectangle(ctx: &mut Ctx, node: &Node) -> Shape {
    let x = ctx.f64(node, "x", 0.0);
    let y = ctx.f64(node, "y", 0.0);
    let w = ctx.f64(node, "objectWidth", 0.0).abs();
    let h = ctx.f64(node, "objectHeight", 0.0).abs();
    let locked = ctx.bool(node, "lockFlag", false);
    let tl = ctx.f64(node, "topLeftRadius", 0.0);
    let radius = |ctx: &mut Ctx, name: &str| {
        if locked && !node.has_attr(name) {
            tl
        } else {
            ctx.f64(node, name, 0.0)
        }
    };
    let tr = radius(ctx, "topRightRadius");
    let br = radius(ctx, "bottomRightRadius");
    let bl = radius(ctx, "bottomLeftRadius");
    if [tl, tr, br, bl].iter().any(|r| *r < 0.0) {
        ctx.warn(
            WarningKind::Unsupported,
            "negative (inverted) corner radii are drawn as square corners",
        );
    }
    let max_r = w.min(h) / 2.0;
    let [tl, tr, br, bl] = [tl, tr, br, bl].map(|r| r.clamp(0.0, max_r));

    let (l, t, r, b) = (x, y, x + w, y + h);
    let mut path = vec![PathCmd::MoveTo(Pt::new(tw(l + tl), tw(t)))];
    let line = |path: &mut Vec<PathCmd>, px: f64, py: f64| {
        path.push(PathCmd::LineTo(Pt::new(tw(px), tw(py))));
    };
    // Clockwise on a y-down screen: the inside is on the right (fillStyle1).
    line(&mut path, r - tr, t);
    corner(&mut path, (r - tr, t + tr), tr, -90.0);
    line(&mut path, r, b - br);
    corner(&mut path, (r - br, b - br), br, 0.0);
    line(&mut path, l + bl, b);
    corner(&mut path, (l + bl, b - bl), bl, 90.0);
    line(&mut path, l, t + tl);
    corner(&mut path, (l + tl, t + tl), tl, 180.0);
    path.dedup();

    primitive_shape(ctx, node, vec![path], true)
}

/// Appends a 90° clockwise corner arc (two quadratic pieces) of radius `r`
/// around `center`, starting at `from_deg` (0° = 3 o'clock, clockwise).
fn corner(path: &mut Vec<PathCmd>, center: (f64, f64), r: f64, from_deg: f64) {
    if r <= 0.0 {
        return;
    }
    arc(path, center, (r, r), from_deg, 90.0);
}

/// Appends an elliptical arc as quadratic pieces of at most 45°. The arc
/// starts at the current point (`from_deg` on the ellipse) and sweeps
/// `sweep_deg` (positive = clockwise on screen). Coordinates are pixels in,
/// twips out.
fn arc(path: &mut Vec<PathCmd>, c: (f64, f64), radii: (f64, f64), from_deg: f64, sweep_deg: f64) {
    let pieces = (sweep_deg.abs() / 45.0).ceil().max(1.0);
    let step = sweep_deg.to_radians() / pieces;
    let mut a0 = from_deg.to_radians();
    // `pieces` is at most 8 for the sweeps used here.
    for _ in 0..pieces as u32 {
        let a1 = a0 + step;
        let mid = (a0 + a1) / 2.0;
        let k = 1.0 / (step / 2.0).cos();
        let ctrl = Pt::new(
            tw(c.0 + radii.0 * mid.cos() * k),
            tw(c.1 + radii.1 * mid.sin() * k),
        );
        let end = Pt::new(tw(c.0 + radii.0 * a1.cos()), tw(c.1 + radii.1 * a1.sin()));
        path.push(PathCmd::QuadTo(ctrl, end));
        a0 = a1;
    }
}

/// Reads a `DOMOvalObject` into a shape: a full ellipse, a ring, a pie slice
/// or an open arc.
pub(crate) fn read_oval(ctx: &mut Ctx, node: &Node) -> Shape {
    let x = ctx.f64(node, "x", 0.0);
    let y = ctx.f64(node, "y", 0.0);
    let w = ctx.f64(node, "objectWidth", 0.0).abs();
    let h = ctx.f64(node, "objectHeight", 0.0).abs();
    let start = ctx.f64(node, "startAngle", 0.0);
    let end = ctx.f64(node, "endAngle", 0.0);
    // innerRadius is a percentage of the outer radius.
    let inner = ctx.f64(node, "innerRadius", 0.0).clamp(0.0, 99.0) / 100.0;
    let close = ctx.bool(node, "closePath", true);

    let c = (x + w / 2.0, y + h / 2.0);
    let outer = (w / 2.0, h / 2.0);
    let hole = (outer.0 * inner, outer.1 * inner);
    let on = |r: (f64, f64), deg: f64| {
        let a = deg.to_radians();
        Pt::new(tw(c.0 + r.0 * a.cos()), tw(c.1 + r.1 * a.sin()))
    };
    let mut sweep = (end - start).rem_euclid(360.0);
    if sweep == 0.0 {
        sweep = 360.0;
    }

    let mut subpaths = Vec::new();
    let mut outer_path = vec![PathCmd::MoveTo(on(outer, start))];
    arc(&mut outer_path, c, outer, start, sweep);
    let mut filled = true;
    if sweep >= 360.0 {
        subpaths.push(outer_path);
        if inner > 0.0 {
            // The hole runs the other way, so the ring stays on the right.
            let mut hole_path = vec![PathCmd::MoveTo(on(hole, start))];
            arc(&mut hole_path, c, hole, start, -360.0);
            subpaths.push(hole_path);
        }
    } else if close {
        if inner > 0.0 {
            outer_path.push(PathCmd::LineTo(on(hole, start + sweep)));
            arc(&mut outer_path, c, hole, start + sweep, -sweep);
        } else {
            outer_path.push(PathCmd::LineTo(Pt::new(tw(c.0), tw(c.1))));
        }
        outer_path.push(PathCmd::LineTo(on(outer, start)));
        subpaths.push(outer_path);
    } else {
        filled = false;
        subpaths.push(outer_path);
    }
    primitive_shape(ctx, node, subpaths, filled)
}

/// Builds the shape of a primitive: one edge per subpath group, styled with
/// the primitive's single fill (right side) and stroke.
fn primitive_shape(ctx: &mut Ctx, node: &Node, subpaths: Vec<Vec<PathCmd>>, filled: bool) -> Shape {
    let matrix = ctx.matrix(node).unwrap_or_default();
    let (fills, strokes) = primitive_styles(ctx, node);
    let fill1 = u32::from(filled && !fills.is_empty());
    let stroke = u32::from(!strokes.is_empty());
    let path: Vec<PathCmd> = subpaths.into_iter().flatten().collect();
    Shape {
        matrix,
        fills,
        strokes,
        edges: vec![Edge {
            fill0: 0,
            fill1,
            stroke,
            path,
        }],
        is_drawing_object: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::xml::parse_str;

    fn shape(xml: &str) -> (Shape, Vec<String>) {
        let mut ctx = Ctx::default();
        let s = read_shape(&mut ctx, &parse_str(xml).unwrap());
        (s, ctx.report.iter().map(|w| w.message.clone()).collect())
    }

    #[test]
    fn style_indices_are_remapped_to_positions() {
        let (s, w) = shape(
            r##"<DOMShape>
              <fills>
                <FillStyle index="3"><SolidColor color="#FF0000"/></FillStyle>
                <FillStyle index="7"><SolidColor color="#00FF00" alpha="0.5"/></FillStyle>
              </fills>
              <strokes><StrokeStyle index="2"><SolidStroke weight="4" caps="none" joints="miter" miterLimit="1.5"><fill><SolidColor color="#0000FF"/></fill></SolidStroke></StrokeStyle></strokes>
              <edges>
                <Edge fillStyle0="7" fillStyle1="3" strokeStyle="2" edges="!0 0|20 0"/>
                <Edge fillStyle1="9" edges="!0 0|1 1"/>
              </edges>
            </DOMShape>"##,
        );
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("fillStyle1=\"9\""));
        assert_eq!(s.fills[1], FillStyle::Solid(Rgba::new(0, 255, 0, 128)));
        assert_eq!(
            (s.edges[0].fill0, s.edges[0].fill1, s.edges[0].stroke),
            (2, 1, 1)
        );
        assert_eq!(s.edges[1].fill1, 0);
        let st = &s.strokes[0];
        assert_eq!(
            (st.weight, st.caps, st.joints, st.miter_limit),
            (4.0, Caps::None, Joints::Miter, 1.5)
        );
        assert_eq!(st.fill, FillStyle::Solid(Rgba::opaque(0, 0, 255)));
        assert_eq!(st.scale_mode, ScaleMode::Normal);
        assert_eq!(st.kind, "SolidStroke");
    }

    #[test]
    fn gradients_and_bitmap_fills() {
        let (s, w) = shape(
            r##"<DOMShape><fills>
              <FillStyle index="1"><RadialGradient focalPointRatio="0.7058823529411765" interpolationMethod="linearRGB" spreadMethod="reflect">
                <matrix><Matrix a="0.079345703125" d="0.079345703125" tx="385" ty="245"/></matrix>
                <GradientEntry alpha="1" color="#FFFF66" ratio="0"/><GradientEntry alpha="0" color="#660000" ratio="1"/>
              </RadialGradient></FillStyle>
              <FillStyle index="2"><BitmapFill bitmapPath="bars.jpg"><matrix><Matrix a="20" d="20" tx="385" ty="20"/></matrix></BitmapFill></FillStyle>
              <FillStyle index="3"><ClippedBitmapFill bitmapPath="bars.jpg"/></FillStyle>
            </fills><edges/></DOMShape>"##,
        );
        assert!(w.is_empty(), "{w:?}");
        let FillStyle::Radial(g) = &s.fills[0] else {
            panic!("radial expected")
        };
        assert_eq!(
            (g.spread, g.interpolation),
            (Spread::Reflect, Interp::LinearRgb)
        );
        assert!((g.focal_point - 0.705_882_352_941_176_5).abs() < 1e-15);
        assert_eq!(g.stops[1].color, Rgba::new(0x66, 0, 0, 0));
        assert_eq!(g.matrix.tx, 385.0);
        let FillStyle::Bitmap(b) = &s.fills[1] else {
            panic!("bitmap expected")
        };
        assert_eq!(
            (b.library_item.as_str(), b.matrix.a, b.clipped),
            ("bars.jpg", 20.0, false)
        );
        let FillStyle::Bitmap(c) = &s.fills[2] else {
            panic!("bitmap expected")
        };
        assert!(c.clipped);
    }

    #[test]
    fn odd_strokes_and_bad_edges_warn() {
        let (s, w) = shape(
            r##"<DOMShape><strokes>
              <StrokeStyle index="1"><DashedStroke weight="2" dash1="3" dash2="4"><fill><SolidColor/></fill></DashedStroke></StrokeStyle>
              <StrokeStyle index="2"><SolidStroke solidStyle="hairline" scaleMode="none"/></StrokeStyle>
            </strokes><edges><Edge strokeStyle="1" edges="!0 0|5 5 ?"/><Edge cubics="!0 0(;1,1 2,2 3,3);"/></edges></DOMShape>"##,
        );
        assert_eq!(w.len(), 3, "{w:?}");
        assert_eq!(s.strokes[0].kind, "DashedStroke");
        assert!(s.strokes[1].hairline);
        assert_eq!(s.strokes[1].scale_mode, ScaleMode::None);
        assert_eq!(
            s.edges[0].path.len(),
            2,
            "the commands before the error are kept"
        );
        assert!(s.edges[1].path.is_empty());
    }

    fn path_bounds(path: &[PathCmd]) -> (f64, f64, f64, f64) {
        let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for cmd in path {
            let pts: Vec<Pt> = match *cmd {
                PathCmd::MoveTo(p) | PathCmd::LineTo(p) => vec![p],
                PathCmd::QuadTo(_, p) => vec![p],
            };
            for p in pts {
                b = (b.0.min(p.x), b.1.min(p.y), b.2.max(p.x), b.3.max(p.y));
            }
        }
        b
    }

    #[test]
    fn rectangle_primitive_is_rebuilt() {
        let mut ctx = Ctx::default();
        let n = parse_str(
            r##"<DOMRectangleObject x="10" y="20" objectWidth="100" objectHeight="50" lockFlag="true" topLeftRadius="10">
              <fill><SolidColor color="#123456"/></fill>
              <stroke><SolidStroke weight="2"><fill><SolidColor/></fill></SolidStroke></stroke>
            </DOMRectangleObject>"##,
        )
        .unwrap();
        let s = read_rectangle(&mut ctx, &n);
        assert!(ctx.report.is_empty());
        assert_eq!(s.fills.len(), 1);
        assert_eq!(s.strokes.len(), 1);
        let e = &s.edges[0];
        assert_eq!((e.fill0, e.fill1, e.stroke), (0, 1, 1));
        assert_eq!(e.path[0], PathCmd::MoveTo(Pt::new(400.0, 400.0)));
        let b = path_bounds(&e.path);
        assert!((b.0 - 200.0).abs() < 1e-9 && (b.1 - 400.0).abs() < 1e-9);
        assert!((b.2 - 2200.0).abs() < 1e-9 && (b.3 - 1400.0).abs() < 1e-9);
        // Four rounded corners of two quadratic pieces each.
        let quads = e
            .path
            .iter()
            .filter(|c| matches!(c, PathCmd::QuadTo(..)))
            .count();
        assert_eq!(quads, 8);
    }

    #[test]
    fn oval_primitives() {
        let mut ctx = Ctx::default();
        let full = parse_str(r#"<DOMOvalObject objectWidth="40" objectHeight="20"><fill><SolidColor/></fill></DOMOvalObject>"#).unwrap();
        let s = read_oval(&mut ctx, &full);
        let path = &s.edges[0].path;
        assert_eq!(path.len(), 9, "move + 8 quads");
        assert_eq!(path[0], PathCmd::MoveTo(Pt::new(800.0, 200.0)));
        let PathCmd::QuadTo(_, last) = path[8] else {
            panic!()
        };
        assert!((last.x - 800.0).abs() < 1e-9 && (last.y - 200.0).abs() < 1e-9);
        let b = path_bounds(path);
        assert!((b.2 - 800.0).abs() < 1e-9 && (b.3 - 400.0).abs() < 1e-9);

        let ring = parse_str(r#"<DOMOvalObject objectWidth="40" objectHeight="40" innerRadius="50"><fill><SolidColor/></fill></DOMOvalObject>"#).unwrap();
        let s = read_oval(&mut ctx, &ring);
        let moves = s.edges[0]
            .path
            .iter()
            .filter(|c| matches!(c, PathCmd::MoveTo(_)))
            .count();
        assert_eq!(moves, 2, "outer contour and hole");

        let pie = parse_str(r#"<DOMOvalObject objectWidth="40" objectHeight="40" startAngle="0" endAngle="90"><fill><SolidColor/></fill></DOMOvalObject>"#).unwrap();
        let s = read_oval(&mut ctx, &pie);
        let path = &s.edges[0].path;
        assert_eq!(
            path[path.len() - 2],
            PathCmd::LineTo(Pt::new(400.0, 400.0)),
            "to the centre"
        );
        assert_eq!(s.edges[0].fill1, 1);

        let arc_only = parse_str(r#"<DOMOvalObject objectWidth="40" objectHeight="40" endAngle="180" closePath="false"><stroke><SolidStroke/></stroke></DOMOvalObject>"#).unwrap();
        let s = read_oval(&mut ctx, &arc_only);
        assert_eq!((s.edges[0].fill1, s.edges[0].stroke), (0, 1));
        assert!(ctx.report.is_empty());
    }
}
