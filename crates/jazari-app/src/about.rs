//! Help › About: the version, Al-Jazari's licence (public domain, the
//! Unlicense) and the notices of the third-party code and fonts it ships.
//! Licence texts are shown verbatim, untranslated.

use eframe::egui::{self, RichText, ScrollArea, Ui};

/// Al-Jazari's own licence.
const UNLICENSE: &str = include_str!("../../../LICENSE");
/// Rascal's MIT licence (vendored in `vendor/rascal`).
const RASCAL_MIT: &str = include_str!("../../../vendor/rascal/LICENSE-MIT");
/// The Apache License 2.0 text (Rascal's and Ruffle's second option).
const APACHE_2: &str = include_str!("../../../vendor/rascal/LICENSE-APACHE");
/// Ruffle's MIT notice, from its `LICENSE.md` at the pinned revision.
const RUFFLE_MIT: &str =
    "Copyright (c) 2018-2022 Ruffle LLC <ruffle@ruffle.rs> and Ruffle contributors
(https://github.com/ruffle-rs/ruffle/graphs/contributors)

Permission is hereby granted, free of charge, to any person obtaining a copy of this \
software and associated documentation files (the \"Software\"), to deal in the Software \
without restriction, including without limitation the rights to use, copy, modify, merge, \
publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons \
to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or \
substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, \
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR \
PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE \
FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR \
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER \
DEALINGS IN THE SOFTWARE.";

/// Shows the About window while `open` is true.
pub fn window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new(tr!("about.title"))
        .open(open)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .default_height(520.0)
        .show(ctx, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.heading(tr!("app.name"));
                    ui.label(tr!("about.version", version = env!("CARGO_PKG_VERSION")));
                    ui.label(tr!("about.tagline"));
                    ui.separator();
                    ui.label(RichText::new(tr!("about.licence_heading")).strong());
                    ui.label(tr!("about.licence"));
                    licence_text(
                        ui,
                        "about-unlicense",
                        tr!("about.show_unlicense"),
                        UNLICENSE,
                    );
                    ui.separator();
                    ui.label(RichText::new(tr!("about.third_party")).strong());
                    notice(
                        ui,
                        tr!("about.ruffle"),
                        tr!("about.ruffle_detail"),
                        &[
                            ("about-ruffle-mit", RUFFLE_MIT),
                            ("about-ruffle-apache", APACHE_2),
                        ],
                    );
                    notice(
                        ui,
                        tr!("about.rascal"),
                        tr!("about.rascal_detail"),
                        &[
                            ("about-rascal-mit", RASCAL_MIT),
                            ("about-rascal-apache", APACHE_2),
                        ],
                    );
                    notice(
                        ui,
                        tr!("about.noto"),
                        tr!("about.noto_detail"),
                        &[("about-ofl", jazari_text::embedded::OFL_TEXT)],
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new(tr!("about.others")).weak());
                });
        });
}

fn notice(ui: &mut Ui, title: &str, detail: &str, texts: &[(&str, &str)]) {
    ui.add_space(4.0);
    ui.label(RichText::new(title).strong());
    ui.label(detail);
    for (id, text) in texts {
        let first = text
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        let heading = tr!("about.show_text", first = first);
        licence_text(ui, id, &heading, text);
    }
}

fn licence_text(ui: &mut Ui, id: &str, heading: &str, text: &str) {
    egui::CollapsingHeader::new(heading)
        .id_salt(id)
        .default_open(false)
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt(id)
                .max_height(220.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(text).monospace().small()).wrap());
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn licence_texts_are_present() {
        assert!(UNLICENSE.contains("public domain"));
        assert!(RASCAL_MIT.contains("Permission is hereby granted"));
        assert!(APACHE_2.contains("Apache License"));
        assert!(RUFFLE_MIT.contains("Ruffle LLC"));
        assert!(jazari_text::embedded::OFL_TEXT.contains("SIL OPEN FONT LICENSE"));
    }
}
