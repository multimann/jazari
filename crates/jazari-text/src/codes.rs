//! Character codes for the DefineFont3 code table.
//!
//! Every embedded glyph needs a code, and the codes of one font must be
//! unique (Ruffle maps codes to glyphs for edit text and kerning; the last
//! duplicate wins). Contextual Arabic forms and ligatures often have no code
//! point of their own, so each glyph gets, in order of preference:
//!
//! 1. the font's own cmap code for it, preferring a base code point over an
//!    Arabic Presentation Form and over a Private Use code;
//! 2. otherwise the Presentation Form the font maps to it (that is already
//!    covered by step 1, which ranks such codes second);
//! 3. otherwise a free Private Use code (U+E000..U+F8FF), assigned in
//!    ascending glyph order, so the table depends only on the font.
//!
//! See `docs/research/text-arabic.md`, F12.

use skrifa::{FontRef, MetadataProvider};

/// Arabic Presentation Forms-A and -B.
fn is_presentation_form(cp: u32) -> bool {
    (0xFB50..=0xFDFF).contains(&cp) || (0xFE70..=0xFEFE).contains(&cp)
}

/// The BMP Private Use Area.
fn is_private_use(cp: u32) -> bool {
    (0xE000..=0xF8FF).contains(&cp)
}

/// Preference rank of a cmap code for a glyph (lower is better).
fn rank(cp: u32) -> u8 {
    if is_private_use(cp) {
        2
    } else if is_presentation_form(cp) {
        1
    } else {
        0
    }
}

/// Builds the code of every glyph of a font, indexed by glyph id.
///
/// Codes outside the BMP cannot be stored (DefineFont3 codes are `u16`),
/// so glyphs reachable only through them get a Private Use code instead.
pub(crate) fn build_code_table(font: &FontRef<'_>, glyph_count: u16) -> Vec<u16> {
    let count = usize::from(glyph_count);
    // Best (rank, code) per glyph from the cmap.
    let mut best: Vec<Option<(u8, u16)>> = vec![None; count];
    let mut used = vec![false; 0x1_0000];
    for (cp, gid) in font.charmap().mappings() {
        let Ok(code) = u16::try_from(cp) else {
            continue;
        };
        if (0xD800..=0xDFFF).contains(&cp) {
            continue;
        }
        let Ok(g) = usize::try_from(gid.to_u32()) else {
            continue;
        };
        let Some(slot) = best.get_mut(g) else {
            continue;
        };
        used[usize::from(code)] = true;
        let candidate = (rank(cp), code);
        if slot.is_none_or(|current| candidate < current) {
            *slot = Some(candidate);
        }
    }

    // Free codes for the rest: the Private Use Area first, then any code the
    // font does not map (skipping controls and surrogates).
    let mut free = (0xE000u32..=0xF8FF)
        .chain(0x0021..0xD800)
        .chain(0xF900..=0xFFFD)
        .filter_map(|cp| u16::try_from(cp).ok());
    let mut table = Vec::with_capacity(count);
    for slot in best {
        let code = match slot {
            Some((_, code)) => code,
            None => loop {
                match free.next() {
                    Some(code) if used[usize::from(code)] => continue,
                    Some(code) => {
                        used[usize::from(code)] = true;
                        break code;
                    }
                    // More unmapped glyphs than free BMP codes: cannot stay
                    // unique. Unreachable for fonts with fewer than ~63k glyphs.
                    None => break 0xFFFD,
                }
            },
        };
        table.push(code);
    }
    table
}
