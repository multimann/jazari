//! The Output panel: movie traces, player messages, the import report and
//! compiler and publish diagnostics. An entry with a location selects it
//! when clicked (timeline, layer, frame and, for scripts, the line).

use eframe::egui::{self, Color32, CornerRadius, RichText, ScrollArea, Sense, Ui, Vec2, pos2};

use crate::doc::DocLocation;
use crate::output::{Entry, Level, Source};
use crate::shaped::{BASELINE_BELOW_CENTER, needs_shaping};
use crate::workspace::Workspace;

/// Draws the panel.
pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    toolbar(ui, ws);
    ui.separator();
    let filter = ws.output_filter;
    let visible: Vec<usize> = ws
        .output
        .iter()
        .enumerate()
        .filter(|(_, e)| filter.accepts(e))
        .map(|(i, _)| i)
        .collect();
    if visible.is_empty() {
        ui.label(RichText::new(tr!("output.empty")).weak());
        return;
    }
    let font_size = egui::TextStyle::Monospace.resolve(ui.style()).size;
    let row_height = ui
        .text_style_height(&egui::TextStyle::Body)
        .max(font_size * 1.4)
        + 2.0;
    let mut clicked: Option<DocLocation> = None;
    let Workspace { output, shaped, .. } = ws;
    ScrollArea::vertical()
        .id_salt("output-list")
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show_rows(ui, row_height, visible.len(), |ui, range| {
            for &i in &visible[range] {
                let Some(entry) = output.get(i) else {
                    continue;
                };
                let width = ui.available_width();
                let sense = if entry.location.is_some() {
                    Sense::click()
                } else {
                    Sense::hover()
                };
                let (rect, response) = ui.allocate_exact_size(Vec2::new(width, row_height), sense);
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let visuals = ui.visuals();
                if response.hovered() && entry.location.is_some() {
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::same(2),
                        visuals.widgets.hovered.weak_bg_fill,
                    );
                }
                let painter = ui.painter_at(rect);
                let (badge, color) = badge(entry, visuals);
                let mono = egui::FontId::monospace(font_size * 0.9);
                let badge_rect = painter.text(
                    pos2(rect.left() + 2.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    badge,
                    mono,
                    color,
                );
                let mut x = rect.left() + 2.0 + badge_rect.width().max(74.0) + 6.0;
                let text_color = match entry.level {
                    Level::Error => Color32::from_rgb(235, 100, 90),
                    Level::Warning => visuals.warn_fg_color,
                    Level::Trace => visuals.strong_text_color(),
                    Level::Info => visuals.text_color(),
                };
                let first_line = entry.text.lines().next().unwrap_or("");
                let ctx = ui.ctx().clone();
                let baseline = rect.center().y + font_size * BASELINE_BELOW_CENTER;
                if needs_shaping(first_line) {
                    x += shaped.paint(
                        &ctx, &painter, x, baseline, first_line, font_size, text_color,
                    );
                } else {
                    let r = painter.text(
                        pos2(x, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        first_line,
                        egui::FontId::monospace(font_size),
                        text_color,
                    );
                    x = r.right();
                }
                if entry.repeats > 1 {
                    painter.text(
                        pos2(x + 8.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        tr!("output.repeats", count = entry.repeats),
                        egui::FontId::proportional(font_size * 0.9),
                        visuals.weak_text_color(),
                    );
                }
                let response = if entry.location.is_some() {
                    response.on_hover_cursor(egui::CursorIcon::PointingHand)
                } else {
                    response
                };
                let response = if entry.text.contains('\n') || x > rect.right() {
                    response.on_hover_text(&entry.text)
                } else {
                    response
                };
                if response.clicked()
                    && let Some(loc) = &entry.location
                {
                    clicked = Some(loc.clone());
                }
            }
        });
    if let Some(loc) = clicked {
        ws.select_location(&loc);
    }
}

fn toolbar(ui: &mut Ui, ws: &mut Workspace) {
    ui.horizontal(|ui| {
        let f = &mut ws.output_filter;
        let o = &ws.output;
        ui.checkbox(
            &mut f.traces,
            tr!("output.filter_traces", count = o.count(Level::Trace)),
        );
        ui.checkbox(
            &mut f.info,
            tr!("output.filter_info", count = o.count(Level::Info)),
        );
        ui.checkbox(
            &mut f.warnings,
            tr!("output.filter_warnings", count = o.count(Level::Warning)),
        );
        ui.checkbox(
            &mut f.errors,
            tr!("output.filter_errors", count = o.count(Level::Error)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr!("output.clear")).clicked() {
                ws.output.clear();
            }
        });
    });
}

/// The tag in front of an entry and its colour.
fn badge(entry: &Entry, visuals: &egui::Visuals) -> (String, Color32) {
    let source = match entry.source {
        Source::App => tr!("output.src_app"),
        Source::Import => tr!("output.src_import"),
        Source::Publish => tr!("output.src_publish"),
        Source::Stage => tr!("output.src_stage"),
        Source::TestMovie => tr!("output.src_movie"),
    };
    let color = match entry.level {
        Level::Trace => Color32::from_rgb(90, 170, 230),
        Level::Info => visuals.weak_text_color(),
        Level::Warning => visuals.warn_fg_color,
        Level::Error => Color32::from_rgb(235, 100, 90),
    };
    (source.to_owned(), color)
}
