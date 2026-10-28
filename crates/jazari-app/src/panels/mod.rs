//! The dock panels. Each module has a `show` function drawing one panel
//! from the [`Workspace`](crate::workspace::Workspace).

pub mod actions;
pub mod library;
pub mod output;
pub mod properties;
pub mod stage;
pub mod test_movie;
pub mod timeline;

use eframe::egui::{self, Align, Color32, Layout, RichText, Ui};

/// A message in the middle of an empty panel.
pub fn centered_message(ui: &mut Ui, text: &str, detail: Option<&str>) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.35).max(0.0));
        ui.label(RichText::new(text).weak().size(15.0));
        if let Some(detail) = detail {
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(detail).weak()).wrap());
        }
    });
}

/// A boxed error or notice with an optional detail, wrapped to the panel.
pub fn notice(ui: &mut Ui, title: &str, detail: &str, error: bool) {
    let color = if error {
        Color32::from_rgb(230, 90, 80)
    } else {
        ui.visuals().warn_fg_color
    };
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.0, color))
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.label(RichText::new(title).strong().color(color));
                if !detail.is_empty() {
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(detail).wrap());
                }
            });
        });
}

/// A spinner with a label.
pub fn busy(ui: &mut Ui, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.35).max(0.0));
        ui.add(egui::Spinner::new());
        ui.label(RichText::new(text).weak());
    });
}
