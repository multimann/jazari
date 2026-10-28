//! Fonts compiled into the executable (`assets/fonts`, SIL OFL 1.1).
//!
//! Release bundles must ship [`OFL_TEXT`] with the program. Sources, versions
//! and checksums are listed in `assets/fonts/README.md`.

/// Noto Sans Arabic Regular 2.013: fallback for sans-serif Arabic faces.
pub const NOTO_SANS_ARABIC: &[u8] =
    include_bytes!("../../../assets/fonts/NotoSansArabic-Regular.ttf");

/// Noto Naskh Arabic Regular 2.021: fallback for Naskh (serif) Arabic faces.
pub const NOTO_NASKH_ARABIC: &[u8] =
    include_bytes!("../../../assets/fonts/NotoNaskhArabic-Regular.ttf");

/// Noto Sans Regular 2.015: Latin, Greek, Cyrillic and common punctuation,
/// which the Arabic Noto fonts lack.
pub const NOTO_SANS: &[u8] = include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf");

/// The SIL Open Font License text with the fonts' copyright notices.
pub const OFL_TEXT: &str = include_str!("../../../assets/fonts/OFL.txt");

/// Every embedded font with its file name.
pub const ALL: [(&str, &[u8]); 3] = [
    ("NotoSansArabic-Regular.ttf", NOTO_SANS_ARABIC),
    ("NotoNaskhArabic-Regular.ttf", NOTO_NASKH_ARABIC),
    ("NotoSans-Regular.ttf", NOTO_SANS),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_fonts_are_truetype() {
        for (name, data) in ALL {
            assert_eq!(
                data.get(..4),
                Some(&[0, 1, 0, 0][..]),
                "{name} lacks the TrueType magic"
            );
        }
        assert!(OFL_TEXT.contains("SIL OPEN FONT LICENSE Version 1.1"));
    }
}
