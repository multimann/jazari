//! Shapes: edges, fills and strokes.

use super::{Matrix, Pt, Rgba};

/// A shape (`DOMShape`): edges with fill and stroke styles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shape {
    /// The shape's `<matrix>` (identity when absent). Edge coordinates are in
    /// the shape's local space.
    pub matrix: Matrix,
    /// Fill styles. [`Edge::fill0`] and [`Edge::fill1`] index this list.
    pub fills: Vec<FillStyle>,
    /// Stroke styles. [`Edge::stroke`] indexes this list.
    pub strokes: Vec<StrokeStyle>,
    /// Edges, in document order.
    pub edges: Vec<Edge>,
    /// `isDrawingObject`.
    pub is_drawing_object: bool,
}

/// One `Edge` element: one or more subpaths sharing the same styles.
///
/// Style selectors are **1-based positions** in [`Shape::fills`] and
/// [`Shape::strokes`] (0 = none). The reader remaps XFL's `FillStyle@index`
/// and `StrokeStyle@index` to positions, so the publisher can copy them to
/// SWF `fill_style_0`, `fill_style_1` and `line_style` unchanged.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Edge {
    /// `fillStyle0`: the fill on the left of the direction of travel
    /// (y-down space).
    pub fill0: u32,
    /// `fillStyle1`: the fill on the right of the direction of travel.
    pub fill1: u32,
    /// `strokeStyle`.
    pub stroke: u32,
    /// Path commands in absolute **twips** (1/20 px), possibly fractional.
    pub path: Vec<PathCmd>,
}

/// A path command. Coordinates are absolute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCmd {
    /// Start a subpath.
    MoveTo(Pt),
    /// Straight line to a point.
    LineTo(Pt),
    /// Quadratic Bézier: control point, then end point.
    QuadTo(Pt, Pt),
}

/// A fill style.
#[derive(Debug, Clone, PartialEq)]
pub enum FillStyle {
    /// A solid colour.
    Solid(Rgba),
    /// A linear gradient.
    Linear(Gradient),
    /// A radial gradient (focal when [`Gradient::focal_point`] is non-zero).
    Radial(Gradient),
    /// A bitmap fill.
    Bitmap(BitmapFill),
}

impl Default for FillStyle {
    /// Opaque black, the XFL `SolidColor` default.
    fn default() -> Self {
        FillStyle::Solid(Rgba::BLACK)
    }
}

/// A linear or radial gradient.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gradient {
    /// Maps the gradient square (±819.2 px, i.e. ±16384 twips) to shape
    /// pixels. `a..d` equal the SWF Fixed16 values; `tx`/`ty` are pixels.
    pub matrix: Matrix,
    /// Colour stops in ratio order. SWF 8 allows at most 15.
    pub stops: Vec<GradientStop>,
    /// `spreadMethod`.
    pub spread: Spread,
    /// `interpolationMethod`.
    pub interpolation: Interp,
    /// `focalPointRatio`, −1..1 (radial gradients only).
    pub focal_point: f64,
}

/// A gradient colour stop.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradientStop {
    /// Position along the gradient, 0..1 (SWF stores `round(ratio × 255)`).
    pub ratio: f64,
    /// Colour at this position.
    pub color: Rgba,
}

/// `spreadMethod` of a gradient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Spread {
    /// Extend the end colours.
    #[default]
    Pad,
    /// Mirror the gradient.
    Reflect,
    /// Repeat the gradient.
    Repeat,
}

/// `interpolationMethod` of a gradient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interp {
    /// Interpolate in sRGB.
    #[default]
    Rgb,
    /// Interpolate in linear RGB.
    LinearRgb,
}

/// A bitmap fill.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BitmapFill {
    /// `bitmapPath`: key into [`super::Library::bitmaps`].
    pub library_item: String,
    /// Bitmap-to-shape matrix; `a = d = 20` means one bitmap pixel per pixel.
    pub matrix: Matrix,
    /// `bitmapIsClipped` (absent is read as false: the bitmap repeats).
    pub clipped: bool,
}

/// A stroke style.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    /// `weight` in pixels.
    pub weight: f64,
    /// The stroke's fill (non-solid fills need SWF 8 LineStyle2).
    pub fill: FillStyle,
    /// `caps`.
    pub caps: Caps,
    /// `joints`.
    pub joints: Joints,
    /// `miterLimit`.
    pub miter_limit: f64,
    /// `scaleMode`.
    pub scale_mode: ScaleMode,
    /// `pixelHinting`.
    pub pixel_hinting: bool,
    /// `solidStyle="hairline"`: a one-twip line at any scale.
    pub hairline: bool,
    /// The stroke element's name: `SolidStroke`, or another kind (dashed,
    /// dotted, ...) that is published as solid with a warning.
    pub kind: String,
}

impl Default for StrokeStyle {
    /// A 1 px solid black stroke with the XFL defaults.
    fn default() -> Self {
        Self {
            weight: 1.0,
            fill: FillStyle::default(),
            caps: Caps::Round,
            joints: Joints::Round,
            miter_limit: 3.0,
            scale_mode: ScaleMode::Normal,
            pixel_hinting: false,
            hairline: false,
            kind: "SolidStroke".to_owned(),
        }
    }
}

/// Line cap style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Caps {
    /// No cap.
    None,
    /// Round caps (the default).
    #[default]
    Round,
    /// Square caps.
    Square,
}

/// Line join style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Joints {
    /// Mitred joins, limited by [`StrokeStyle::miter_limit`].
    Miter,
    /// Round joins (the default).
    #[default]
    Round,
    /// Bevelled joins.
    Bevel,
}

/// `scaleMode` of a stroke: which instance scaling changes its width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ScaleMode {
    /// Scale in both directions (read as the default when absent).
    #[default]
    Normal,
    /// `horizontal`: SWF `NO_V_SCALE` (mapping unverified).
    Horizontal,
    /// `vertical`: SWF `NO_H_SCALE` (mapping unverified).
    Vertical,
    /// `none`: never scale.
    None,
}
