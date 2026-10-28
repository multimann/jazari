//! The Stage panel: the stage build in a player, letterboxed on a
//! pasteboard, with zoom. The player never plays by itself: the timeline
//! moves it with `goto_and_stop`.

use eframe::egui::{
    self, Color32, CornerRadius, Rect, RichText, ScrollArea, Sense, Ui, pos2, vec2,
};
use eframe::egui_wgpu;

use crate::doc::TimelineRef;
use crate::panels::{busy, centered_message, notice};
use crate::workspace::{StageStatus, Workspace, Zoom};

/// Largest texture side the stage renders at; larger zooms are scaled up.
const MAX_TEXTURE: f32 = 4096.0;
/// Zoom presets of the toolbar.
const PRESETS: [f32; 7] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

/// Draws the panel.
pub fn show(ui: &mut Ui, ws: &mut Workspace, rs: Option<&egui_wgpu::RenderState>) {
    toolbar(ui, ws);
    ui.separator();
    match ws.stage.status.clone() {
        StageStatus::Empty => centered_message(
            ui,
            tr!("stage.no_document"),
            Some(tr!("stage.no_document_hint")),
        ),
        StageStatus::Building => busy(ui, tr!("stage.building")),
        StageStatus::Failed(reason) => {
            ui.add_space(8.0);
            notice(ui, tr!("stage.unavailable"), &reason, true);
            ui.add_space(6.0);
            ui.label(RichText::new(tr!("stage.unavailable_hint")).weak());
        }
        StageStatus::Ready => player_view(ui, ws, rs),
    }
}

fn toolbar(ui: &mut Ui, ws: &mut Workspace) {
    ui.horizontal(|ui| {
        ui.label(tr!("stage.zoom"));
        let current = match ws.stage.zoom {
            Zoom::Fit => tr!("stage.zoom_fit").to_owned(),
            Zoom::Scale(s) => format!("{:.0}%", s * 100.0),
        };
        egui::ComboBox::from_id_salt("stage-zoom")
            .selected_text(current)
            .width(80.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut ws.stage.zoom, Zoom::Fit, tr!("stage.zoom_fit"));
                for p in PRESETS {
                    ui.selectable_value(
                        &mut ws.stage.zoom,
                        Zoom::Scale(p),
                        format!("{:.0}%", p * 100.0),
                    );
                }
            });
        if ws.doc.is_some() {
            ui.separator();
            let (w, h) = ws.stage_size();
            ui.label(RichText::new(tr!("stage.size", width = w, height = h)).weak());
            if let TimelineRef::Symbol(_) = ws.view {
                ui.separator();
                ui.label(RichText::new(tr!("stage.symbol_note")).weak())
                    .on_hover_text(tr!("stage.symbol_note_hover"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(tr!("stage.rebuild"))
                    .on_hover_text(tr!("stage.rebuild_hover"))
                    .clicked()
                {
                    ws.rebuild_stage();
                }
                if let Some(player) = ws.stage.player.as_mut()
                    && player.is_halted()
                    && ui.button(tr!("player.restart")).clicked()
                    && player.restart().is_ok()
                {
                    ws.sync_stage();
                }
            });
        }
    });
}

fn player_view(ui: &mut Ui, ws: &mut Workspace, rs: Option<&egui_wgpu::RenderState>) {
    let Some(rs) = rs else {
        centered_message(ui, tr!("render.no_wgpu"), None);
        return;
    };
    let (sw, sh) = ws.stage_size();
    let avail = ui.available_size();
    let fit = ((avail.x - 24.0) / sw).min((avail.y - 24.0) / sh).max(0.01);
    let scale = match ws.stage.zoom {
        Zoom::Fit => fit,
        Zoom::Scale(s) => s,
    };
    let size = vec2(sw * scale, sh * scale);
    let ppp = ui.ctx().pixels_per_point();
    let mut px = size * ppp;
    let over = (px.x / MAX_TEXTURE).max(px.y / MAX_TEXTURE);
    if over > 1.0 {
        px /= over;
    }
    let Some(player) = ws.stage.player.as_mut() else {
        return;
    };
    player.resize(
        px.x.round().max(1.0) as u32,
        px.y.round().max(1.0) as u32,
        ppp,
    );
    player.render();
    let texture = player.egui_texture(rs);
    if let Some(after) = player.repaint_after() {
        ui.ctx().request_repaint_after(after);
    }

    let output = ScrollArea::both()
        .id_salt("stage-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let total = vec2(size.x.max(avail.x), size.y.max(avail.y));
            let (rect, response) = ui.allocate_exact_size(total, Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, CornerRadius::ZERO, pasteboard(ui));
            let stage = Rect::from_center_size(rect.center(), size);
            painter.rect_filled(
                stage.translate(vec2(3.0, 3.0)),
                CornerRadius::ZERO,
                Color32::from_black_alpha(90),
            );
            painter.image(
                texture,
                stage,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            response
        });

    // Ctrl + wheel (or a pinch) zooms around the current scale.
    if output.inner.hovered() {
        let zoom = ui.input(|i| i.zoom_delta());
        if (zoom - 1.0).abs() > f32::EPSILON {
            ws.stage.zoom = Zoom::Scale((scale * zoom).clamp(0.05, 16.0));
        }
    }
}

/// The pasteboard around the stage.
fn pasteboard(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_gray(48)
    } else {
        Color32::from_gray(200)
    }
}
