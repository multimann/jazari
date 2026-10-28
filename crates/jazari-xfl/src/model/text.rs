//! Classic text fields: static, dynamic and input text.

use super::{Matrix, Rgba};

/// A classic text element (`DOMStaticText`, `DOMDynamicText`, `DOMInputText`).
///
/// Geometry is in pixels, in the text's local space under [`TextElement::matrix`].
/// Classic XFL text has no direction attribute: Arabic is stored in logical
/// order and its base direction is inferred when laying it out.
#[derive(Debug, Clone, PartialEq)]
pub struct TextElement {
    /// Static, dynamic or input.
    pub kind: TextKind,
    /// Placement matrix.
    pub matrix: Matrix,
    /// `left`: x of the text box.
    pub left: f64,
    /// `top`: y of the text box.
    pub top: f64,
    /// `width` of the text box.
    pub width: f64,
    /// `height` of the text box.
    pub height: f64,
    /// Runs in logical order. A CR (`\r`) inside a run ends a paragraph.
    pub runs: Vec<TextRun>,
    /// `autoExpand`: the box sizes itself to the text.
    pub auto_expand: bool,
    /// `lineType` (dynamic and input text; static text is treated as
    /// multiline when its box has a width).
    pub line_type: LineType,
    /// `isSelectable`.
    pub selectable: bool,
    /// Instance name (dynamic and input text).
    pub name: Option<String>,
    /// `variableName` (dynamic and input text).
    pub variable: Option<String>,
    /// `maxCharacters` (input text).
    pub max_chars: Option<u32>,
    /// `border`: draw a border and background.
    pub border: bool,
    /// `renderAsHTML` (dynamic and input text).
    pub render_as_html: bool,
    /// `orientation`: `horizontal`, or a vertical writing mode.
    pub orientation: String,
}

impl Default for TextElement {
    /// An empty, selectable, horizontal static text at the origin.
    fn default() -> Self {
        Self {
            kind: TextKind::Static,
            matrix: Matrix::IDENTITY,
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
            runs: Vec::new(),
            auto_expand: false,
            line_type: LineType::SingleLine,
            selectable: true,
            name: None,
            variable: None,
            max_chars: None,
            border: false,
            render_as_html: false,
            orientation: "horizontal".to_owned(),
        }
    }
}

/// The kind of a text element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextKind {
    /// `DOMStaticText`: laid out and shaped by Al-Jazari, published as glyphs.
    #[default]
    Static,
    /// `DOMDynamicText`: published as DefineEditText with a device font.
    Dynamic,
    /// `DOMInputText`: published as editable DefineEditText.
    Input,
}

/// `lineType` of a text field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineType {
    /// `single line` (the default).
    #[default]
    SingleLine,
    /// `multiline`: wrap at the box width.
    Multiline,
    /// `multiline no wrap`: break only at CR.
    MultilineNoWrap,
    /// `password`: single line, masked.
    Password,
}

/// A run of text with uniform attributes (`DOMTextRun`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextRun {
    /// `<characters>`, in logical order.
    pub text: String,
    /// `<textAttrs><DOMTextAttrs/></textAttrs>`.
    pub attrs: TextAttrs,
}

/// Character and paragraph attributes of a run (`DOMTextAttrs`).
///
/// Paragraph attributes (alignment, indent, margins, line spacing) are
/// repeated on every run of the paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct TextAttrs {
    /// `face`: the font name as written (PostScript or family name).
    pub face: String,
    /// `size` in points (= pixels at 1:1). May be fractional.
    pub size: f64,
    /// `fillColor` with `alpha`.
    pub fill: Rgba,
    /// `alignment`.
    pub align: Align,
    /// `indent` in points.
    pub indent: f64,
    /// `leftMargin` in points.
    pub left_margin: f64,
    /// `rightMargin` in points.
    pub right_margin: f64,
    /// `lineSpacing` (leading) in points.
    pub line_spacing: f64,
    /// `letterSpacing` in points.
    pub letter_spacing: f64,
    /// `autoKern`.
    pub auto_kern: bool,
    /// `url` of a link.
    pub url: Option<String>,
    /// `target` of a link.
    pub target: Option<String>,
}

impl Default for TextAttrs {
    /// The XFL defaults: 12 pt black, left-aligned, 2 pt leading, kerned.
    fn default() -> Self {
        Self {
            face: String::new(),
            size: 12.0,
            fill: Rgba::BLACK,
            align: Align::Left,
            indent: 0.0,
            left_margin: 0.0,
            right_margin: 0.0,
            line_spacing: 2.0,
            letter_spacing: 0.0,
            auto_kern: true,
            url: None,
            target: None,
        }
    }
}

/// Paragraph alignment. `Left` and `Right` are absolute, as in XFL; for an
/// RTL paragraph `Right` is the start edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    /// Flush left (the default).
    #[default]
    Left,
    /// Flush right.
    Right,
    /// Centred.
    Center,
    /// Justified.
    Justify,
}
