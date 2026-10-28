//! Splitting a paragraph into items: runs of one font, bidi level, script
//! and shaping style, each shaped in one harfrust call.

use std::collections::HashMap;
use std::ops::Range;

use unicode_bidi::Level;
use unicode_script::{Script as UScript, UnicodeScript};

use crate::library::{FaceChain, FontId};
use crate::shaping::{FaceView, ShapeStyle};

/// The faces prepared for one layout, by id.
pub(crate) type Views<'l> = HashMap<FontId, FaceView<'l>>;

/// A maximal range of a paragraph shaped in one call.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Item {
    /// Bytes of the paragraph.
    pub range: Range<usize>,
    pub font: FontId,
    /// Bidi embedding level before line reordering (rule L1).
    pub level: Level,
    pub script: harfrust::Script,
    /// Whether the item is in a cursive script (Arabic), where letter spacing
    /// would break the joins and is therefore not applied.
    pub cursive: bool,
    pub style: ShapeStyle,
}

/// What itemizing needs to know about the run under each character.
pub(crate) trait RunLookup {
    /// The run index at an absolute byte offset of the block text.
    fn run_at(&self, abs: usize) -> usize;
    /// The fallback chain of a run's face.
    fn chain(&self, run: usize) -> &FaceChain;
    /// The shaping style of a run.
    fn style(&self, run: usize) -> ShapeStyle;
}

/// Itemizes one paragraph.
///
/// - `para`: the paragraph text; `offset`: its byte offset in the block text;
/// - `levels`: unicode-bidi's per-byte levels for the paragraph.
pub(crate) fn itemize(
    para: &str,
    offset: usize,
    levels: &[Level],
    runs: &impl RunLookup,
    views: &Views<'_>,
) -> Vec<Item> {
    let scripts = resolve_scripts(para);
    let mut items: Vec<Item> = Vec::new();
    let mut prev: Option<(usize, FontId)> = None;
    for ((i, c), script) in para.char_indices().zip(scripts) {
        let run = runs.run_at(offset + i);
        let raw = c.script();
        let is_mark = raw == UScript::Inherited;
        // Characters inherit the previous character's face only within runs
        // of the same face name (a colour or size change must not change the
        // glyphs); a mark keeps its base's face even across runs.
        let prev_font = match prev {
            Some((r, f)) if r == run || is_mark || runs.chain(r) == runs.chain(run) => Some(f),
            _ => None,
        };
        let font = choose_font(c, raw, prev_font, runs.chain(run), views);
        prev = Some((run, font));
        let level = levels.get(i).copied().unwrap_or_else(Level::ltr);
        let hb_script = to_harfrust(script);
        // A mark is shaped with its base, whatever its own run says: a
        // separate shaping call would lose its attachment (GPOS mark-to-base).
        let style = match items.last() {
            Some(it) if is_mark && it.font == font => it.style,
            _ => runs.style(run),
        };
        let end = i + c.len_utf8();
        match items.last_mut() {
            Some(it)
                if it.font == font
                    && it.level == level
                    && it.script == hb_script
                    && it.style == style =>
            {
                it.range.end = end;
            }
            _ => items.push(Item {
                range: i..end,
                font,
                level,
                script: hb_script,
                cursive: is_cursive(script),
                style,
            }),
        }
    }
    items
}

/// Picks the face for one character.
///
/// - Invisible characters (controls, joiners, bidi marks) stay with the
///   previous character.
/// - Marks stay with their base when its face has them.
/// - Arabic letters use the resolved face if it has the letter and can shape
///   Arabic (GSUB `arab`), else the Arabic fallback that suits the face's
///   style, else the other one.
/// - Common characters (spaces, digits, punctuation) prefer the previous
///   character's face, so `مرحبا، عالم` stays in one font. The Arabic Noto
///   fonts lack ASCII brackets, quotes, `?`, `;` and `/`: those go to the
///   resolved face or Noto Sans.
/// - Everything else uses the resolved face, else Noto Sans, else an Arabic
///   Noto font.
fn choose_font(
    c: char,
    script: UScript,
    prev: Option<FontId>,
    chain: &FaceChain,
    views: &Views<'_>,
) -> FontId {
    let covers = |id: FontId| views.get(&id).is_some_and(|v| v.covers(c));
    if is_invisible(c) {
        return prev.unwrap_or_else(|| chain.first());
    }
    let neutral = matches!(
        script,
        UScript::Common | UScript::Inherited | UScript::Unknown
    );
    if neutral && let Some(p) = prev.filter(|&p| covers(p)) {
        return p;
    }
    let arabic_fallback = || chain.arabic.iter().copied().find(|&f| covers(f));
    let latin_fallback = || covers(chain.latin).then_some(chain.latin);
    if script == UScript::Arabic {
        let can_join = |id: FontId| views.get(&id).is_some_and(|v| v.face.arabic_shaping());
        return chain
            .primary
            .filter(|&p| covers(p) && can_join(p))
            .or_else(arabic_fallback)
            .or_else(latin_fallback)
            .unwrap_or(chain.arabic[0]);
    }
    chain
        .primary
        .filter(|&p| covers(p))
        .or_else(latin_fallback)
        .or_else(arabic_fallback)
        .unwrap_or_else(|| chain.first())
}

/// Characters that draw nothing and must not split items: controls, line
/// separators and default-ignorable format characters (ZWJ, ZWNJ, bidi
/// marks and embeddings, variation selectors, BOM).
fn is_invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{180B}'..='\u{180F}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
        )
}

/// The script of each character, with Common and Inherited characters
/// taking the script of the text before them (or after them, at the start
/// of the paragraph). A paragraph without any script stays Common.
fn resolve_scripts(para: &str) -> Vec<UScript> {
    let raw: Vec<UScript> = para.chars().map(|c| c.script()).collect();
    let is_real =
        |s: UScript| !matches!(s, UScript::Common | UScript::Inherited | UScript::Unknown);
    let first = raw
        .iter()
        .copied()
        .find(|&s| is_real(s))
        .unwrap_or(UScript::Common);
    let mut last = first;
    raw.into_iter()
        .map(|s| {
            if is_real(s) {
                last = s;
            }
            last
        })
        .collect()
}

/// Scripts whose letters join, so letter spacing would break the words.
fn is_cursive(script: UScript) -> bool {
    matches!(
        script,
        UScript::Arabic
            | UScript::Syriac
            | UScript::Nko
            | UScript::Mongolian
            | UScript::Mandaic
            | UScript::Adlam
            | UScript::Hanifi_Rohingya
    )
}

/// unicode-script's script as harfrust's (both use ISO 15924 tags).
fn to_harfrust(script: UScript) -> harfrust::Script {
    script
        .short_name()
        .parse()
        .unwrap_or(harfrust::script::COMMON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_characters_take_the_surrounding_script() {
        let s = resolve_scripts("(مرحبا) 2026 abc");
        assert_eq!(s[0], UScript::Arabic); // leading bracket looks ahead
        assert_eq!(s[6], UScript::Arabic); // closing bracket
        assert_eq!(s[8], UScript::Arabic); // digits follow Arabic
        assert_eq!(s[13], UScript::Latin);
        assert_eq!(resolve_scripts("123"), vec![UScript::Common; 3]);
        assert_eq!(to_harfrust(UScript::Arabic), harfrust::script::ARABIC);
        assert_eq!(to_harfrust(UScript::Common), harfrust::script::COMMON);
    }
}
