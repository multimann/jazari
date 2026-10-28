//! Elements placed on a keyframe: shapes, groups, instances, bitmaps, text.

use super::{Filter, Matrix, Pt, Shape, SymbolKind, TextElement};

/// One element of a keyframe or of a group.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    /// A raw shape or drawing object (`DOMShape`).
    Shape(Shape),
    /// A group (`DOMGroup`).
    Group(Group),
    /// A symbol instance (`DOMSymbolInstance`).
    Instance(SymbolInstance),
    /// A bitmap instance (`DOMBitmapInstance`).
    Bitmap(BitmapInstance),
    /// Static, dynamic or input text.
    Text(TextElement),
    /// An element the reader does not support (for example `DOMTLFText`
    /// without a fallback, video or a component). The reader adds a warning.
    Unsupported {
        /// The XML element's local name.
        tag: String,
    },
}

impl Element {
    /// The element's own matrix, if it has one. Groups without a `<matrix>`
    /// and unsupported elements return `None`.
    pub fn matrix(&self) -> Option<&Matrix> {
        match self {
            Element::Shape(s) => Some(&s.matrix),
            Element::Group(g) => g.matrix.as_ref(),
            Element::Instance(i) => Some(&i.matrix),
            Element::Bitmap(b) => Some(&b.matrix),
            Element::Text(t) => Some(&t.matrix),
            Element::Unsupported { .. } => None,
        }
    }
}

/// A group of elements (`DOMGroup`). Groups nest.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    /// The group's `<matrix>`, if present. Members with their own matrix are
    /// believed to store it in the group's parent space (unverified; see
    /// `docs/research/xfl-format.md`).
    pub matrix: Option<Matrix>,
    /// Members, back to front.
    pub members: Vec<Element>,
}

/// An instance of a library symbol (`DOMSymbolInstance`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SymbolInstance {
    /// `libraryItemName`: key into [`super::Library::symbols`].
    pub library_item: String,
    /// `symbolType` of the instance (absent means movie clip).
    pub kind: SymbolKind,
    /// Instance name (movie clips and buttons).
    pub name: Option<String>,
    /// Placement matrix.
    pub matrix: Matrix,
    /// `transformationPoint` in the instance's local pixels.
    pub transformation_point: Pt,
    /// Colour effect, normalised from `<color><Color/></color>`.
    pub color: ColorEffect,
    /// Filters, in order. Disabled filters are kept but not published.
    pub filters: Vec<Filter>,
    /// `blendMode`.
    pub blend_mode: BlendMode,
    /// `cacheAsBitmap`.
    pub cache_as_bitmap: bool,
    /// Playback of a graphic instance; `None` for movie clips and buttons.
    pub graphic: Option<GraphicPlayback>,
    /// `trackAsMenu` (buttons).
    pub track_as_menu: bool,
    /// The instance's `on()` / `onClipEvent()` source (CR and CRLF
    /// normalised to LF); `jazari-as2` splits it into handlers.
    pub script: Option<String>,
}

/// A colour transform: `out = in × mult + add`, per channel, in R, G, B, A
/// order.
///
/// Brightness and tint shorthands are converted to this form by the reader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorEffect {
    /// Multipliers (1 = unchanged; may exceed 1).
    pub mult: [f64; 4],
    /// Offsets in 0..255 colour units (usually −255..255).
    pub add: [f64; 4],
}

impl ColorEffect {
    /// The identity transform.
    pub const IDENTITY: ColorEffect = ColorEffect {
        mult: [1.0; 4],
        add: [0.0; 4],
    };

    /// Whether this is the identity transform.
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }
}

impl Default for ColorEffect {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// `blendMode` of an instance (SWF 8 blend modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    /// `normal` (SWF 0).
    #[default]
    Normal,
    /// `layer` (SWF 2).
    Layer,
    /// `multiply` (SWF 3).
    Multiply,
    /// `screen` (SWF 4).
    Screen,
    /// `lighten` (SWF 5).
    Lighten,
    /// `darken` (SWF 6).
    Darken,
    /// `difference` (SWF 7).
    Difference,
    /// `add` (SWF 8).
    Add,
    /// `subtract` (SWF 9).
    Subtract,
    /// `invert` (SWF 10).
    Invert,
    /// `alpha` (SWF 11).
    Alpha,
    /// `erase` (SWF 12).
    Erase,
    /// `overlay` (SWF 13).
    Overlay,
    /// `hardlight` (SWF 14).
    HardLight,
}

/// How a graphic instance plays its symbol's timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GraphicPlayback {
    /// `loop`.
    pub mode: LoopMode,
    /// `firstFrame`, 0-based.
    pub first_frame: u32,
}

/// `loop` attribute of a graphic instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LoopMode {
    /// Loop the symbol's timeline (the default).
    #[default]
    Loop,
    /// Play once and hold the last frame.
    PlayOnce,
    /// Show `first_frame` only.
    SingleFrame,
}

/// An instance of a library bitmap (`DOMBitmapInstance`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BitmapInstance {
    /// `libraryItemName`: key into [`super::Library::bitmaps`].
    pub library_item: String,
    /// Placement matrix.
    pub matrix: Matrix,
}
