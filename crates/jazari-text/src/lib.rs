//! Fonts, shaping, bidi and line layout for Al-Jazari's static text.
//!
//! Text is shaped with harfrust per bidi run and script, reordered with
//! unicode-bidi, wrapped with unicode-linebreak, and returned as positioned
//! glyphs that the publisher turns into DefineFont3 + DefineText and that the
//! stage draws. Outlines come from skrifa. Fonts are found by PostScript
//! name, then family name, with the embedded Noto fonts as fallbacks
//! ([`embedded`]), and characters a face lacks fall back one by one.
//!
//! ```
//! use jazari_text::{FontLibrary, TextBlock, layout};
//! use jazari_xfl::model::{TextAttrs, TextRun};
//!
//! let lib = FontLibrary::embedded_only();
//! let attrs = TextAttrs { face: "Traditional Arabic".into(), size: 24.0, ..TextAttrs::default() };
//! let block = TextBlock {
//!     runs: vec![TextRun { text: "مرحبا بالعالم".into(), attrs }],
//!     ..TextBlock::default()
//! };
//! let text = layout(&lib, &block);
//! assert_eq!(text.lines.len(), 1);
//! assert!(text.lines[0].rtl);
//! ```
//!
//! # Coordinates
//!
//! Everything a [`TextLayout`] holds is in **twips as `f64`** (1/20 px), in
//! the text box's own space: the origin is the box's top-left corner (the
//! point XFL calls `left`, `top`, inside Flash's 2 px gutter), x grows to the
//! right and y grows downwards. Flash 8 places a DefineText at the XFL
//! matrix, with record coordinates relative to that corner.
//!
//! A glyph's `(x, y)` is where its outline's origin goes: the pen position
//! plus the shaper's offsets, on the baseline. Outlines from
//! [`FontLibrary::glyph_outline`] are in ems, y down, so outline point
//! `(px, py)` lands at `(x + px × size, y + py × size)`, with
//! [`PositionedGlyph::size`] in twips. For DefineFont3, scale outlines by
//! 20480 (its units per em) and give each record `height = size`.
//!
//! # Layout rules
//!
//! Calibrated on the Flash 8 `text` oracle fixture where possible: line
//! origins, baselines and per-glyph advances of its Arial and Times New Roman
//! texts match Flash's DefineText records exactly (`tests/system_fonts.rs`).
//!
//! - Font sizes are points, and 1 pt = 1 px at 100 %: `size × 20` twips.
//! - Each glyph advance is rounded to whole twips, as Flash writes them, and
//!   line widths (so alignment) are sums of those. Mark offsets stay exact.
//! - The first baseline is `ascent` below the top; each next baseline is the
//!   previous one plus the previous line's descent, the previous paragraph's
//!   `lineSpacing` and this line's ascent. Baselines and line origins are
//!   rounded to whole pixels, as Flash does.
//! - A line's ascent and descent are the largest among the runs on it, taken
//!   from each run's resolved face (not from per-character fallbacks), with
//!   the sources described in [`FontMetrics`]. The font's line gap is not
//!   used.
//! - Alignment is absolute (`Left` is the left edge even for Arabic, as in
//!   XFL). Justified lines stretch their breaking spaces, except the last
//!   line of a paragraph, which sits at the paragraph's start edge.
//! - Trailing spaces do not count for fitting or alignment; on a
//!   right-to-left line they sit to the left of the content.
//! - `leftMargin` and `rightMargin` are absolute; `indent` applies to the
//!   first line of a paragraph at its start edge (the right edge for a
//!   right-to-left paragraph). Not verified against Flash, which had no
//!   right-to-left layout.
//! - `letterSpacing` (points) is added after every cluster, except in
//!   Arabic text, where it would break the joins. Non-zero spacing also
//!   turns off optional ligatures, as CSS does.
//! - `autoKern="false"` turns off the `kern` feature.
//! - Each paragraph's base direction comes from its first strong character
//!   (UAX #9 rules P2 and P3); a paragraph without one takes the direction
//!   of the first paragraph that has one, else left-to-right.
//! - Wrapping is greedy over break opportunities (UAX #14), measured on
//!   logical advances; each line is then shaped again in visual order, with
//!   the paragraph around it as shaping context, so words broken across
//!   lines keep their joining forms. A word wider than the line is broken
//!   between clusters.
//!
//! # Fonts
//!
//! - A face name matches a PostScript name, then a family name (also after
//!   dropping style words such as `Bold` or `-BoldMT`). A document's `fonts/`
//!   folder wins over the executable's, which wins over installed fonts,
//!   which win over the embedded ones. Flash's device names `_sans`,
//!   `_serif` and `_typewriter` resolve to Arial, Times New Roman and
//!   Courier New (or a similar installed family).
//! - Characters the face lacks fall back one by one: Arabic letters to Noto
//!   Naskh Arabic for Naskh-like faces (`Traditional Arabic`, `Simplified
//!   Arabic`, `Arabic Typesetting`, Times-like names) or to Noto Sans Arabic
//!   otherwise; Latin and the punctuation the Arabic Noto fonts lack (ASCII
//!   brackets, quotes, `?`, `;`, `/`) to Noto Sans. Spaces, digits and
//!   punctuation stay in the previous character's face when it has them
//!   and the face name is the same (a colour or size change does not change
//!   glyphs); marks stay in their base's face when it has them, and are
//!   then shaped with their base even across runs. Faces whose GSUB has no `arab` script cannot join Arabic, so Arabic
//!   letters skip them.
//!
//! See `docs/ARCHITECTURE.md`, section 8, and `docs/research/text-arabic.md`.

#![warn(missing_docs)]

mod codes;
pub mod embedded;
mod error;
mod face;
mod itemize;
mod layout;
mod library;
mod names;
mod outline;
mod shaping;
mod wrap;

pub use error::Error;
pub use layout::{LayoutLine, PositionedGlyph, TWIPS_PER_PX, TextBlock, TextLayout, layout};
pub use library::{
    EmbeddedFonts, FaceInfo, FaceMatch, FontId, FontLibrary, FontLibraryOptions, FontMetrics,
    FontOrigin, Resolution,
};
