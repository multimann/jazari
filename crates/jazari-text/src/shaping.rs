//! Shaping one segment of a paragraph with harfrust.

use std::ops::Range;

use harfrust::{BufferFlags, Direction, Feature, ShapeOptions, Tag, UnicodeBuffer};
use skrifa::FontRef;
use skrifa::charmap::Charmap;

use crate::library::{FontId, FontLibrary, LoadedFace};

/// A face prepared for one layout: parsed font, charmap and shaper.
pub(crate) struct FaceView<'l> {
    pub face: &'l LoadedFace,
    charmap: Charmap<'l>,
    metrics: skrifa::metrics::GlyphMetrics<'l>,
    shaper: harfrust::Shaper<'l>,
    /// Font units per em.
    pub upem: f64,
}

impl<'l> FaceView<'l> {
    /// Prepares a face, or `None` if it cannot be loaded.
    pub fn new(lib: &'l FontLibrary, id: FontId) -> Option<Self> {
        use skrifa::MetadataProvider;
        let face = lib.face(id)?;
        let font: FontRef<'l> = face.font()?;
        let charmap = font.charmap();
        let metrics = font.glyph_metrics(
            skrifa::instance::Size::unscaled(),
            skrifa::instance::LocationRef::default(),
        );
        let shaper = face.shaper_data().shaper(&font).build();
        let upem = f64::from(face.metrics().units_per_em.max(1));
        Some(Self {
            face,
            charmap,
            metrics,
            shaper,
            upem,
        })
    }

    /// The nominal (`hmtx`) advance of a glyph in font units.
    pub fn nominal_advance(&self, glyph: u16) -> Option<f64> {
        self.metrics
            .advance_width(skrifa::GlyphId::new(u32::from(glyph)))
            .map(f64::from)
    }

    /// Whether the face has a real glyph (not `.notdef`) for a character.
    pub fn covers(&self, c: char) -> bool {
        self.charmap.map(c).is_some_and(|g| g.to_u32() != 0)
    }
}

/// Shaping options that split items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ShapeStyle {
    /// XFL `autoKern`.
    pub kern: bool,
    /// Letter spacing is non-zero: optional ligatures are turned off, as CSS
    /// does, so spacing applies between the letters a user typed.
    pub spaced: bool,
}

/// One shaped glyph, in font units (y up), before scaling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RawGlyph {
    pub glyph: u16,
    /// Byte offset of the cluster's first character in the paragraph.
    pub cluster: usize,
    pub x_advance: f64,
    pub x_offset: f64,
    pub y_offset: f64,
}

/// What to shape.
pub(crate) struct Segment<'a> {
    /// The whole paragraph, for context.
    pub para: &'a str,
    /// The bytes to shape.
    pub range: Range<usize>,
    pub rtl: bool,
    pub script: harfrust::Script,
    pub style: ShapeStyle,
}

/// Shapes a segment. Glyphs come out in visual order (left to right), even
/// for right-to-left segments, with clusters as paragraph byte offsets.
///
/// The text around the segment is passed as pre- and post-context, so Arabic
/// letters keep their joining forms across font, direction and line cuts.
pub(crate) fn shape(view: &FaceView<'_>, seg: &Segment<'_>) -> Vec<RawGlyph> {
    let Some(text) = seg.para.get(seg.range.clone()) else {
        return Vec::new();
    };
    let mut buf = UnicodeBuffer::new();
    for (i, c) in text.char_indices() {
        let Some(c) = shapeable(c) else { continue };
        let cluster = u32::try_from(seg.range.start + i).unwrap_or(u32::MAX);
        buf.add(c, cluster);
    }
    if buf.is_empty() {
        return Vec::new();
    }
    // Context must be set after the characters: `add` clears the post-context.
    buf.set_pre_context(seg.para.get(..seg.range.start).unwrap_or(""));
    buf.set_post_context(seg.para.get(seg.range.end..).unwrap_or(""));
    buf.set_direction(if seg.rtl {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    });
    buf.set_script(seg.script);
    let mut flags = BufferFlags::REMOVE_DEFAULT_IGNORABLES;
    if seg.range.start == 0 {
        flags |= BufferFlags::BEGINNING_OF_TEXT;
    }
    if seg.range.end == seg.para.len() {
        flags |= BufferFlags::END_OF_TEXT;
    }
    buf.set_flags(flags);

    let mut features = Vec::with_capacity(3);
    if !seg.style.kern {
        features.push(Feature::new(Tag::new(b"kern"), 0, ..));
    }
    if seg.style.spaced {
        features.push(Feature::new(Tag::new(b"liga"), 0, ..));
        features.push(Feature::new(Tag::new(b"clig"), 0, ..));
    }
    // No scale: positions in font units.
    let out = view
        .shaper
        .shape(buf, ShapeOptions::new().features(&features));
    out.glyph_infos()
        .iter()
        .zip(out.glyph_positions())
        .map(|(info, pos)| RawGlyph {
            glyph: u16::try_from(info.glyph_id).unwrap_or(0),
            cluster: usize::try_from(info.cluster).unwrap_or(0),
            x_advance: f64::from(pos.x_advance),
            x_offset: f64::from(pos.x_offset),
            y_offset: f64::from(pos.y_offset),
        })
        .collect()
}

/// Maps a character to what the shaper should see: tabs become spaces;
/// other controls and line separators produce no glyph.
fn shapeable(c: char) -> Option<char> {
    match c {
        '\t' => Some(' '),
        '\u{2028}' | '\u{2029}' => None,
        c if c.is_control() => None,
        c => Some(c),
    }
}
