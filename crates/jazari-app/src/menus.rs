//! The menu bar and the keyboard shortcuts.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText, Ui};
use egui_dock::DockState;

use crate::dock::{self, Tab};
use crate::settings::RenderPref;
use crate::workspace::Workspace;

/// Ctrl+O: File › Open…
pub const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
/// Shift+F12: File › Publish… (Flash's shortcut).
pub const PUBLISH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::F12);
/// Ctrl+Q: File › Exit.
pub const EXIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);
/// Ctrl+Enter: Control › Test Movie.
pub const TEST_MOVIE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);
/// Enter: Control › Play.
pub const PLAY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Enter);
/// `.`: next frame (Flash's shortcut).
pub const STEP_FORWARD: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Period);
/// `,`: previous frame.
pub const STEP_BACK: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Comma);

/// Draws the menu bar.
pub fn menu_bar(ui: &mut Ui, ws: &mut Workspace, dock: &mut DockState<Tab>, frame: &eframe::Frame) {
    egui::MenuBar::new().ui(ui, |ui| {
        let file = ui.menu_button(tr!("menu.file"), |ui| file_menu(ui, ws, frame));
        let control = ui.menu_button(tr!("menu.control"), |ui| control_menu(ui, ws));
        let settings = ui.menu_button(tr!("menu.settings"), |ui| settings_menu(ui, ws));
        let window = ui.menu_button(tr!("menu.window"), |ui| window_menu(ui, dock));
        let help = ui.menu_button(tr!("menu.help"), |ui| {
            if ui.button(tr!("menu.about")).clicked() {
                ws.show_about = true;
            }
        });
        // Developer option `--open-menu <name>`: open one menu once, so a
        // screenshot can show it.
        if let Some(name) = ws.dev.open_menu.take() {
            let target = match name.as_str() {
                "file" => Some(file.response),
                "control" => Some(control.response),
                "settings" => Some(settings.response),
                "window" => Some(window.response),
                "help" => Some(help.response),
                _ => None,
            };
            if let Some(response) = target {
                egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&response));
            }
        }
    });
}

fn item(ui: &mut Ui, enabled: bool, text: &str, shortcut: Option<&KeyboardShortcut>) -> bool {
    let mut button = egui::Button::new(text);
    if let Some(s) = shortcut {
        button = button.shortcut_text(ui.ctx().format_shortcut(s));
    }
    ui.add_enabled(enabled, button).clicked()
}

fn file_menu(ui: &mut Ui, ws: &mut Workspace, frame: &eframe::Frame) {
    let idle = !ws.dialogs.busy();
    if item(ui, idle, tr!("menu.open"), Some(&OPEN)) {
        let dir = ws.dialog_dir();
        ws.dialogs.open_file(frame, dir.as_deref());
    }
    if item(ui, idle, tr!("menu.open_folder"), None) {
        let dir = ws.dialog_dir();
        ws.dialogs.open_folder(frame, dir.as_deref());
    }
    let recent = ws.settings.recent.clone();
    ui.add_enabled_ui(!recent.is_empty(), |ui| {
        ui.menu_button(tr!("menu.recent"), |ui| {
            for path in &recent {
                let exists = path.exists();
                let text = path.display().to_string();
                if ws
                    .shaped
                    .button(ui, &text, exists)
                    .on_disabled_hover_text(tr!("menu.recent_missing"))
                    .clicked()
                {
                    ws.open(path.clone());
                }
            }
            ui.separator();
            if ui.button(tr!("menu.recent_clear")).clicked() {
                ws.settings.recent.clear();
                ws.save_settings();
            }
        });
    });
    if item(ui, ws.doc.is_some(), tr!("menu.close"), None) {
        ws.close();
    }
    ui.separator();
    if item(
        ui,
        ws.doc.is_some() && idle,
        tr!("menu.publish"),
        Some(&PUBLISH),
    ) {
        let dir = ws.dialog_dir();
        let name = ws.publish_name();
        ws.dialogs.save_swf(frame, dir.as_deref(), &name);
    }
    ui.separator();
    if item(ui, true, tr!("menu.exit"), Some(&EXIT)) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

fn control_menu(ui: &mut Ui, ws: &mut Workspace) {
    let can_test = ws.doc.is_some() || ws.dev.test_swf.is_some();
    if item(ui, can_test, tr!("menu.test_movie"), Some(&TEST_MOVIE)) {
        ws.test_movie();
    }
    ui.separator();
    let has_doc = ws.doc.is_some();
    let play = if ws.playback.is_some() {
        tr!("menu.pause")
    } else {
        tr!("menu.play")
    };
    if item(ui, has_doc, play, Some(&PLAY)) {
        ws.toggle_play();
    }
    if item(ui, has_doc, tr!("menu.rewind"), None) {
        ws.rewind();
    }
    if item(ui, has_doc, tr!("menu.step_forward"), Some(&STEP_FORWARD)) {
        ws.step(1);
    }
    if item(ui, has_doc, tr!("menu.step_back"), Some(&STEP_BACK)) {
        ws.step(-1);
    }
}

fn settings_menu(ui: &mut Ui, ws: &mut Workspace) {
    ui.label(RichText::new(tr!("menu.rendering")).strong());
    let forced = ws.render.forced;
    let mut pref = ws.settings.rendering;
    ui.add_enabled_ui(!forced, |ui| {
        ui.radio_value(&mut pref, RenderPref::Gpu, tr!("menu.render_gpu"))
            .on_disabled_hover_text(tr!("menu.render_gpu_forced"));
    });
    ui.radio_value(&mut pref, RenderPref::Cpu, tr!("menu.render_cpu"));
    if pref != ws.settings.rendering {
        ws.set_render_pref(pref);
    }
    if forced {
        ui.label(RichText::new(tr!("menu.render_forced")).color(ui.visuals().warn_fg_color));
    }
    ui.label(RichText::new(tr!("menu.render_restart")).weak());
    ui.separator();
    let active = match ws.render.active {
        jazari_preview::RenderMode::Gpu => tr!("menu.render_active_gpu"),
        jazari_preview::RenderMode::Cpu => tr!("menu.render_active_cpu"),
    };
    ui.label(tr!("menu.render_active", mode = active));
    ui.label(RichText::new(tr!("menu.adapter", adapter = ws.render.adapter)).weak());
    for note in &ws.render.notes {
        ui.label(RichText::new(note).color(ui.visuals().warn_fg_color));
    }
    if ws.settings.rendering.mode() != ws.render.active && !ws.render.forced {
        ui.label(RichText::new(tr!("menu.render_pending")).weak());
    }
    ui.separator();
    let weak = ui.visuals().weak_text_color();
    let text = tr!("menu.data_dir", path = ws.data.dir.display());
    // Not justified like the rest of the menu: a long path would be
    // stretched across the width.
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ws.shaped.label(ui, &text, Some(weak));
    });
}

fn window_menu(ui: &mut Ui, dock: &mut DockState<Tab>) {
    for tab in Tab::ALL {
        let mut shown = dock.find_tab(&tab).is_some();
        let fixed = matches!(tab, Tab::Stage | Tab::Timeline);
        let response = ui.add_enabled(!fixed, egui::Checkbox::new(&mut shown, tab.title()));
        if response.clicked() {
            dock::toggle(dock, tab);
        }
    }
    ui.separator();
    if ui.button(tr!("menu.reset_layout")).clicked() {
        *dock = dock::default_layout();
    }
}

/// Global shortcuts. Plain Enter, `.` and `,` are left to a focused
/// Test Movie, whose movie may want them.
pub fn shortcuts(ctx: &egui::Context, ws: &mut Workspace, frame: &eframe::Frame) {
    let movie_focused = ws.movie.player.as_ref().is_some_and(|p| p.is_focused());
    let pressed = |s: &KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(s));
    if pressed(&TEST_MOVIE) && (ws.doc.is_some() || ws.dev.test_swf.is_some()) {
        ws.test_movie();
    }
    if pressed(&OPEN) && !ws.dialogs.busy() {
        let dir = ws.dialog_dir();
        ws.dialogs.open_file(frame, dir.as_deref());
    }
    if pressed(&PUBLISH) && ws.doc.is_some() && !ws.dialogs.busy() {
        let dir = ws.dialog_dir();
        let name = ws.publish_name();
        ws.dialogs.save_swf(frame, dir.as_deref(), &name);
    }
    if pressed(&EXIT) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    let text_focus = ctx.egui_wants_keyboard_input();
    if movie_focused || text_focus || ws.doc.is_none() {
        return;
    }
    if pressed(&PLAY) {
        ws.toggle_play();
    }
    if pressed(&STEP_FORWARD) {
        ws.step(1);
    }
    if pressed(&STEP_BACK) {
        ws.step(-1);
    }
}
