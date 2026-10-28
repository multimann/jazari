//! Device fonts registered with every player.
//!
//! Ruffle's `default_font` feature already registers a Latin/Greek/Cyrillic
//! Noto Sans subset for `_sans`, `_serif` and `_typewriter`. It has no Arabic,
//! so Noto Sans Arabic is registered under its own name: text fields that
//! name it get Arabic glyphs. Ruffle does not shape or reorder device text,
//! so Arabic there still shows as isolated letters in logical order; static
//! text, shaped at publish time, is unaffected (see
//! `docs/research/ruffle-embed.md`, F18).
//!
//! The bytes duplicate the copy embedded by `jazari-text` (this crate depends
//! on Ruffle only).

use ruffle_core::Player;
use ruffle_core::backend::ui::FontDefinition;
use ruffle_core::font::FontFileData;

/// The device-font name under which Noto Sans Arabic is registered.
pub const ARABIC_DEVICE_FONT: &str = "Noto Sans Arabic";

/// Noto Sans Arabic 2.013, OFL (see `assets/fonts/README.md`).
static NOTO_SANS_ARABIC: &[u8] = include_bytes!("../../../assets/fonts/NotoSansArabic-Regular.ttf");

/// Registers Noto Sans Arabic as the device font [`ARABIC_DEVICE_FONT`].
pub(crate) fn register_arabic(player: &mut Player) {
    player.register_device_font(FontDefinition::FontFile {
        name: ARABIC_DEVICE_FONT.to_owned(),
        is_bold: false,
        is_italic: false,
        data: FontFileData::new(NOTO_SANS_ARABIC),
        index: 0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_font_is_a_truetype_file() {
        assert_eq!(&NOTO_SANS_ARABIC[..4], &[0, 1, 0, 0]);
        assert!(NOTO_SANS_ARABIC.len() > 100_000);
    }
}
