//! The in-memory authoring document (`docs/ARCHITECTURE.md`, section 5).
//!
//! Units: coordinates are **pixels as `f64`**, except shape edges
//! ([`Edge::path`]), which keep **twips as `f64`** because XFL edge numbers are
//! twips and may be fractional. Layer order follows XFL: index 0 of
//! [`Timeline::layers`] is the top layer; index 0 of [`Frame::elements`] is the
//! bottom of a layer's stack.
//!
//! The model is plain data. The reader fills it in and never aborts on unknown
//! content: unknown elements become [`Element::Unsupported`], unknown layer
//! types [`LayerKind::Other`], unknown tweens [`Tween::Other`], each with a
//! warning in the [`crate::ImportReport`].

mod document;
mod element;
mod filter;
mod geom;
mod shape;
mod text;
mod timeline;

pub use document::{
    BitmapData, BitmapItem, ContainerKind, Document, FontItem, Library, Linkage, PublishSettings,
    SourceInfo, Symbol, SymbolKind,
};
pub use element::{
    BitmapInstance, BlendMode, ColorEffect, Element, GraphicPlayback, Group, LoopMode,
    SymbolInstance,
};
pub use filter::{
    AdjustColorFilter, BevelFilter, BevelPlacement, BlurFilter, ColorMatrixFilter,
    ConvolutionFilter, DropShadowFilter, Filter, FilterKind, GlowFilter, GradientFilter,
};
pub use geom::{Decomposed, Matrix, Pt, Rect, Rgba};
pub use shape::{
    BitmapFill, Caps, Edge, FillStyle, Gradient, GradientStop, Interp, Joints, PathCmd, ScaleMode,
    Shape, Spread, StrokeStyle,
};
pub use text::{Align, LineType, TextAttrs, TextElement, TextKind, TextRun};
pub use timeline::{
    Frame, FrameLabel, LabelKind, Layer, LayerKind, MotionTween, RotateMode, ShapeTween, Timeline,
    Tween,
};
