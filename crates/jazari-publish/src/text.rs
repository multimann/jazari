//! Text: static text as DefineFont3 + DefineText/DefineText2, dynamic and
//! input text as DefineEditText with a device font.
//!
//! # Static text
//!
//! Static text is laid out by `jazari-text` (shaping, bidi, wrapping) and
//! published as glyph runs, so Arabic is joined and right to left in every
//! player. The recipe was calibrated on the Flash 8 `text` fixture, whose
//! Latin records ours reproduce (font codes and order, advances, record
//! positions, colours and heights):
//!
//! - One DefineFont3 per face ([`FontId`](jazari_text::FontId)) holding the
//!   union of the glyphs every reachable static text uses (`scan`), in
//!   20480 units per em, codes ascending. A face whose glyphs would make the
//!   tag larger than about 60 KB is split into several fonts (`font`).
//! - One DefineText per text element (DefineText2 when a colour has alpha),
//!   with `matrix = translate(left, top)`; the timeline places it with the
//!   element's matrix. Records follow Flash: the first sets font, height,
//!   colour and baseline (and x only when it is not 0); every line starts a
//!   record with x and y; a change of font, size, colour or baseline (marks
//!   with a y offset) mid-line starts a record that continues the pen
//!   (`records`).
//! - Each DefineText is followed by CSMTextSettings (advanced anti-aliasing,
//!   sub-pixel grid fit), as Flash 8 writes it.
//!
//! # Dynamic and input text
//!
//! DefineEditText with a device font: a glyph-less DefineFont3 named after
//! the face's family, the initial text as HTML (Flash 8 sets the HTML flag
//! and wraps the text in `<p><font>` even when "render as HTML" is off),
//! the variable name and the field's layout, followed by CSMTextSettings
//! (`edit`, `html`). Calibrated on the fixtures and on a probe of Flash 8
//! with styled runs, links, password, multiline and input fields.
//!
//! Flash 8 also turns *static* text into DefineEditText when it is
//! selectable, has links or uses device fonts (the probe). We keep such text
//! as shaped glyphs, so Arabic stays correct, and report what is lost.
//!
//! # Diagnostics
//!
//! Every problem is reported at the text element: faces replaced by a
//! fallback, characters drawn by another font or missing from every font
//! (`.notdef`), faces whose licence forbids embedding, Arabic in dynamic or
//! input text, vertical orientation, links on static text, and values that
//! do not fit the SWF fields (a text is then left out with an error).
//!
//! # Seam with the timeline
//!
//! The timeline calls [`define_text`] once per text element of a keyframe
//! and places the returned [`TextChar`] at the element's depth, with the
//! instance name of dynamic and input text. Identical texts share one
//! character, so a text that stays the same across keyframes keeps its
//! instance.

mod edit;
mod font;
mod html;
mod records;
mod scan;
mod statics;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::rc::Rc;

use jazari_as2::Origin;
use jazari_text::TextLayout;
use jazari_xfl::model::{Matrix, TextElement, TextKind};
use swf::CharacterId;

use crate::context::Ctx;

/// Per-publish state of the text module.
#[derive(Debug, Default)]
pub struct TextState {
    /// Whether the document has been scanned for static text glyphs.
    scanned: bool,
    /// Layouts by text block (see `scan::block_key`).
    layouts: HashMap<String, Rc<TextLayout>>,
    /// Embedded fonts, per face.
    fonts: font::Fonts,
    /// Device fonts (glyph-less DefineFont3) by name, bold and italic.
    device_fonts: HashMap<(String, bool, bool), Option<CharacterId>>,
    /// Text characters by their encoded tags with id 0, so identical texts
    /// share one character.
    defined: HashMap<Vec<u8>, CharacterId>,
}

/// A defined text character and the matrix to place it with.
#[derive(Debug, Clone, PartialEq)]
pub struct TextChar {
    /// The DefineText/DefineText2/DefineEditText character.
    pub id: CharacterId,
    /// The placement matrix (pixels), in the element's parent space.
    pub matrix: Matrix,
}

/// Defines the character for one text element, or reports why not and
/// returns `None`. Empty static text defines nothing and reports nothing.
pub fn define_text(ctx: &mut Ctx<'_>, text: &TextElement, origin: &Origin) -> Option<TextChar> {
    if text.orientation != "horizontal" {
        ctx.warn(
            origin.clone(),
            format!(
                "'{}' text is published horizontally (vertical text is not supported)",
                text.orientation
            ),
        );
    }
    let id = match text.kind {
        TextKind::Static => statics::define(ctx, text, origin)?,
        TextKind::Dynamic | TextKind::Input => edit::define(ctx, text, origin)?,
    };
    Some(TextChar {
        id,
        matrix: text.matrix,
    })
}

/// Registers encoded definition tags (with character id 0) as a character,
/// or returns the character already defined with the same tags. The ids of
/// the tags are patched when they are defined.
fn define_shared(
    ctx: &mut Ctx<'_>,
    kind: crate::context::CharKind,
    tags: Vec<crate::tags::RawTag>,
    deps: Vec<CharacterId>,
    origin: &Origin,
) -> Option<CharacterId> {
    let mut key = Vec::new();
    for t in &tags {
        key.extend_from_slice(&t.code.to_le_bytes());
        key.extend_from_slice(&(t.body.len() as u64).to_le_bytes());
        key.extend_from_slice(&t.body);
    }
    if let Some(&id) = ctx.text.defined.get(&key) {
        return Some(id);
    }
    let id = ctx.define(kind, deps, origin, |id| {
        Ok(tags
            .into_iter()
            .map(|mut t| {
                t.set_leading_id(id);
                t
            })
            .collect())
    })?;
    ctx.text.defined.insert(key, id);
    Some(id)
}

/// Whether a character belongs to the Arabic script blocks (letters,
/// marks, digits and presentation forms).
fn is_arabic(c: char) -> bool {
    matches!(
        u32::from(c),
        0x0600..=0x06FF
            | 0x0750..=0x077F
            | 0x0870..=0x089F
            | 0x08A0..=0x08FF
            | 0xFB50..=0xFDFF
            | 0xFE70..=0xFEFF
    )
}

/// A short, quoted excerpt of some text for messages.
fn excerpt(s: &str) -> String {
    format!("\"{}\"", clip(s, 24))
}

/// A name from the document (a face, say) as it may appear in a message:
/// control characters become spaces and long names are cut.
fn clip(s: &str, max: usize) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}
