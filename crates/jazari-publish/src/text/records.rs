//! DefineText records from positioned glyphs.
//!
//! Positions are absolute twips in the DefineText's own space (y down). The
//! player draws each glyph at the pen, then moves the pen by the glyph's
//! signed advance; a record may set the pen's x and the baseline y, and a
//! record without them continues where the previous one stopped. So:
//!
//! - glyph `i`'s advance is `round(x[i+1]) - round(x[i])` when the next
//!   glyph is on the same line (shaper offsets are absorbed, research F14),
//!   else its own rounded advance (as Flash writes the last glyph of a
//!   line);
//! - a record starts at every line, and wherever the font, size, colour or
//!   baseline changes (a mark with a y offset gets its own record), or after
//!   255 glyphs;
//! - a record sets only what changed. The first one always sets font,
//!   height, colour and y, and sets x only when it is not 0, as Flash 8
//!   does; a mid-line record continues the pen and sets no x (the probe of
//!   Flash 8 with colour and size changes inside a line wrote exactly that).
//!
//! Only the offsets a record writes must fit 16 bits (±1638 px from the
//! text's origin); a long line reaches further through its advances.

use swf::{CharacterId, Color, GlyphEntry, TextRecord, Twips};

use crate::units::round_clamped;

/// Most glyphs in one record (the count is a byte).
pub const MAX_GLYPHS_PER_RECORD: usize = 255;

/// A glyph to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    /// The font character.
    pub font: CharacterId,
    /// Glyph index in that font.
    pub index: u16,
    /// Drawing x in twips.
    pub x: f64,
    /// Baseline y in twips (minus the glyph's y offset).
    pub y: f64,
    /// Pen advance in twips.
    pub advance: f64,
    /// Em size in twips (the record height).
    pub height: f64,
    /// Colour.
    pub color: Color,
    /// Line number (a new line always starts a record).
    pub line: usize,
}

/// A value that does not fit its DefineText field.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordError {
    /// A line or a mark that starts beyond ±32767 twips (about ±1638 px)
    /// from the text's origin, where a record must set its position.
    Offset(f64),
    /// A font size beyond 65535 twips (3276 px), or negative.
    Height(f64),
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offset(v) => write!(
                f,
                "a line or mark starts {:.0} px from the text's origin; DefineText allows at most 1638 px",
                v / 20.0
            ),
            Self::Height(v) => write!(
                f,
                "font size {:.0} px is outside 0..3276 px, which DefineText allows",
                v / 20.0
            ),
        }
    }
}

/// The record state the player carries from record to record.
#[derive(Debug, Clone, Copy)]
struct State {
    font: CharacterId,
    height: i32,
    color: Color,
    y: i64,
    line: usize,
}

/// Largest position we handle, in twips. Only the offsets a record
/// actually writes must fit 16 bits; glyphs further along a line are
/// reached through advances.
const MAX_POSITION: f64 = 1_000_000_000.0;

/// Builds the records of one text. Glyphs are in drawing order (lines top
/// to bottom, each left to right).
pub fn build(glyphs: &[Placed]) -> Result<Vec<TextRecord>, RecordError> {
    let mut recs: Vec<TextRecord> = Vec::new();
    let mut state: Option<State> = None;
    // The player's pen x after the last glyph written (it starts at 0).
    let mut pen: i64 = 0;
    let mut prev: Option<(i64, usize)> = None; // (x, line) of the last glyph
    for (i, g) in glyphs.iter().enumerate() {
        let x = position(g.x)?;
        let y = position(g.y)?;
        let height = height(g.height)?;
        // Finish the previous glyph's advance now that we know this one.
        if let Some((px, pline)) = prev {
            let advance = if pline == g.line {
                x - px
            } else {
                i64::from(own_advance(&glyphs[i - 1]))
            };
            if let Some(e) = recs.last_mut().and_then(|r| r.glyphs.last_mut()) {
                e.advance = i32::try_from(advance).map_err(|_| RecordError::Offset(g.x))?;
            }
            pen = px + advance;
        }
        let continues = state.is_some_and(|s| {
            s.font == g.font
                && s.height == height
                && s.color == g.color
                && s.y == y
                && s.line == g.line
        }) && recs
            .last()
            .is_some_and(|r| r.glyphs.len() < MAX_GLYPHS_PER_RECORD);
        if !continues {
            let first = state.is_none();
            let (font_changed, color_changed, new_line, y_changed) = match state {
                None => (true, true, false, true),
                Some(s) => (
                    s.font != g.font || s.height != height,
                    s.color != g.color,
                    s.line != g.line,
                    s.y != y,
                ),
            };
            let x_offset = if new_line || pen != x {
                Some(offset(x, g.x)?)
            } else {
                None
            };
            let y_offset = if first || y_changed {
                Some(offset(y, g.y)?)
            } else {
                None
            };
            recs.push(TextRecord {
                font_id: font_changed.then_some(g.font),
                height: font_changed.then_some(Twips::new(height)),
                color: color_changed.then_some(g.color),
                x_offset,
                y_offset,
                glyphs: Vec::new(),
            });
        }
        state = Some(State {
            font: g.font,
            height,
            color: g.color,
            y,
            line: g.line,
        });
        if let Some(r) = recs.last_mut() {
            r.glyphs.push(GlyphEntry {
                index: u32::from(g.index),
                advance: 0,
            });
        }
        prev = Some((x, g.line));
    }
    if let (Some(last), Some(e)) = (
        glyphs.last(),
        recs.last_mut().and_then(|r| r.glyphs.last_mut()),
    ) {
        e.advance = own_advance(last);
    }
    Ok(recs)
}

/// A glyph's own advance, rounded (for the last glyph of a line).
fn own_advance(g: &Placed) -> i32 {
    round_clamped(g.advance, f64::from(i16::MAX))
}

/// A glyph position, rounded to whole twips.
fn position(v: f64) -> Result<i64, RecordError> {
    let r = v.round();
    if r.is_finite() && r.abs() <= MAX_POSITION {
        Ok(r as i64)
    } else {
        Err(RecordError::Offset(v))
    }
}

/// A position written as a record offset (i16 twips).
fn offset(v: i64, original: f64) -> Result<Twips, RecordError> {
    i16::try_from(v)
        .map(|v| Twips::new(i32::from(v)))
        .map_err(|_| RecordError::Offset(original))
}

/// A record height (u16 twips).
fn height(v: f64) -> Result<i32, RecordError> {
    let r = v.round();
    if r.is_finite() && (0.0..=f64::from(u16::MAX)).contains(&r) {
        Ok(r as i32)
    } else {
        Err(RecordError::Height(v))
    }
}
