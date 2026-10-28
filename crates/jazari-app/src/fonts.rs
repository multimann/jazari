//! egui's fonts: the defaults plus Noto Sans Arabic as a fallback, so
//! Arabic shown through egui's own text (combo boxes, tooltips, file paths)
//! has glyphs. Right-to-left content that must read correctly goes through
//! [`crate::shaped`] instead, because egui does not reorder bidi text.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// Adds the embedded Arabic font after egui's default fonts.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let name = "noto-sans-arabic".to_owned();
    fonts.font_data.insert(
        name.clone(),
        Arc::new(FontData::from_static(
            jazari_text::embedded::NOTO_SANS_ARABIC,
        )),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push(name.clone());
    }
    ctx.set_fonts(fonts);
}
