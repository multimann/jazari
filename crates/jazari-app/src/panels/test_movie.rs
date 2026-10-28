//! The Test Movie panel: the release build playing in its own player, with
//! mouse and keyboard input. Ruffle letterboxes the movie in the panel.

use std::time::{Duration, Instant};

use eframe::egui::{self, RichText, Sense, Ui};
use eframe::egui_wgpu;
use jazari_preview::PlayerStatus;

use crate::panels::{busy, centered_message, notice};
use crate::workspace::{MovieStatus, Workspace};

/// Longest time step given to the player at once, so a panel that was
/// hidden for a while does not race through frames.
const MAX_STEP: Duration = Duration::from_millis(100);

/// Draws the panel.
pub fn show(ui: &mut Ui, ws: &mut Workspace, rs: Option<&egui_wgpu::RenderState>) {
    toolbar(ui, ws);
    ui.separator();
    match ws.movie.status.clone() {
        MovieStatus::Idle => centered_message(
            ui,
            tr!("movie.idle"),
            Some(if ws.doc.is_some() || ws.dev.test_swf.is_some() {
                tr!("movie.idle_hint")
            } else {
                tr!("movie.idle_no_document")
            }),
        ),
        MovieStatus::Publishing => busy(ui, tr!("movie.publishing")),
        MovieStatus::Failed(reason) => {
            ui.add_space(8.0);
            notice(ui, tr!("movie.unavailable"), &reason, true);
        }
        MovieStatus::Running => player_view(ui, ws, rs),
    }
}

fn toolbar(ui: &mut Ui, ws: &mut Workspace) {
    ui.horizontal(|ui| {
        let can_test = ws.doc.is_some() || ws.dev.test_swf.is_some();
        if ui
            .add_enabled(can_test, egui::Button::new(tr!("movie.run")))
            .on_hover_text(tr!("movie.run_hover"))
            .clicked()
        {
            ws.test_movie();
        }
        let Some(player) = ws.movie.player.as_mut() else {
            return;
        };
        let playing = player.is_playing();
        if ui
            .button(if playing {
                tr!("movie.pause")
            } else {
                tr!("movie.play")
            })
            .clicked()
        {
            player.set_playing(!playing);
        }
        if ui.button(tr!("movie.step")).clicked() {
            player.set_playing(false);
            player.step_frame();
        }
        if ui.button(tr!("player.restart")).clicked() {
            let _ = player.restart();
        }
        ui.separator();
        let status = match player.status() {
            PlayerStatus::Running if playing => tr!("movie.status_playing"),
            PlayerStatus::Running => tr!("movie.status_paused"),
            PlayerStatus::Halted => tr!("movie.status_halted"),
            PlayerStatus::Crashed => tr!("movie.status_crashed"),
        };
        ui.label(status);
        if let Some(frame) = player.current_frame() {
            ui.label(RichText::new(tr!("movie.frame", frame = frame)).weak());
        }
        ui.label(RichText::new(&ws.movie.source).weak());
    });
}

fn player_view(ui: &mut Ui, ws: &mut Workspace, rs: Option<&egui_wgpu::RenderState>) {
    let Some(rs) = rs else {
        centered_message(ui, tr!("render.no_wgpu"), None);
        return;
    };
    let now = Instant::now();
    let dt = ws
        .movie
        .last_tick
        .map_or(Duration::ZERO, |t| now.duration_since(t).min(MAX_STEP));
    ws.movie.last_tick = Some(now);
    let Some(player) = ws.movie.player.as_mut() else {
        return;
    };
    if player.is_halted() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("movie.halted_banner")).color(ui.visuals().warn_fg_color));
        });
    }
    let size = ui.available_size().max(egui::vec2(1.0, 1.0));
    let ppp = ui.ctx().pixels_per_point();
    player.resize(
        (size.x * ppp).round() as u32,
        (size.y * ppp).round() as u32,
        ppp,
    );
    player.tick(dt);
    player.render();
    let texture = player.egui_texture(rs);
    let response = ui.add(
        egui::Image::from_texture(egui::load::SizedTexture::new(texture, size))
            .sense(Sense::click_and_drag()),
    );
    player.ui_input(ui, &response);
    if let Some(after) = player.repaint_after() {
        ui.ctx().request_repaint_after(after);
    }
}
