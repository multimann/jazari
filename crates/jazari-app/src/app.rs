//! The eframe application: the dock, the menu and status bars, and the
//! order of work in each frame.

use std::path::PathBuf;
use std::time::Duration;

use eframe::egui;
use egui_dock::{DockArea, DockState, Style};
use jazari_preview::{
    AdapterChoice, RenderMode, adapter_summary, descriptors_from, is_software_adapter,
    last_adapter_choice,
};

use crate::cli::Args;
use crate::dock::{self, Tab, Viewer};
use crate::menus;
use crate::output::{Level, Source};
use crate::paths::DataDir;
use crate::screenshot::Screenshot;
use crate::settings::Settings;
use crate::workspace::{DevOptions, RenderInfo, Workspace};
use crate::{about, fonts};

/// eframe storage key of the dock layout.
const DOCK_KEY: &str = "jazari-dock";
/// Default idle time before `--screenshot` captures the window.
const SCREENSHOT_SETTLE: Duration = Duration::from_secs(3);

/// What `main` hands to the app.
pub struct Startup {
    /// The data folder.
    pub data: DataDir,
    /// The settings as loaded.
    pub settings: Settings,
    /// A problem met while loading them.
    pub settings_problem: Option<String>,
    /// The command line.
    pub args: Args,
    /// The mode eframe was started with.
    pub mode: RenderMode,
    /// `JAZARI_FORCE_CPU` is set.
    pub forced: bool,
    /// Notices from startup (the GPU-to-CPU fallback).
    pub notices: Vec<String>,
}

/// The application.
pub struct JazariApp {
    dock: DockState<Tab>,
    ws: Workspace,
    screenshot: Option<Screenshot>,
    /// `--dump-output`: print the Output panel at exit.
    dump_output: bool,
    /// No frame has been shown yet.
    first_frame: bool,
}

impl JazariApp {
    /// Builds the app inside eframe's creation callback.
    pub fn new(cc: &eframe::CreationContext<'_>, startup: Startup) -> Result<Self, String> {
        fonts::install(&cc.egui_ctx);
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .ok_or_else(|| tr!("render.no_wgpu").to_owned())?;
        let info = rs.adapter.get_info();
        let adapter = adapter_summary(&info);
        let mut notes = startup.notices.clone();
        if let Some(AdapterChoice::HardwareFallback { reason, .. }) = last_adapter_choice() {
            notes.push(tr!("render.hardware_fallback", reason = reason));
        }
        // Belt and braces: with JAZARI_FORCE_CPU set, never render on a
        // hardware adapter even if one was handed to us.
        let descriptors = if startup.forced && !is_software_adapter(&info) {
            notes.push(tr!("render.refused", adapter = adapter));
            None
        } else {
            Some(descriptors_from(rs))
        };
        let render = RenderInfo {
            active: startup.mode,
            forced: startup.forced,
            adapter,
            notes,
            descriptors,
        };

        let saved = cc
            .storage
            .and_then(|s| eframe::get_value::<DockState<Tab>>(s, DOCK_KEY));
        let dock = if startup.args.screenshot.is_some() {
            // Automatic screenshots always use the default layout.
            dock::default_layout()
        } else {
            dock::restore(saved)
        };

        let args = startup.args;
        let dev = DevOptions {
            test_swf: args.test_swf.clone(),
            stage_swf: args.stage_swf.clone(),
            autostart_test: args.test_swf.is_some() || args.test_movie,
            open_menu: args.open_menu.clone(),
            publish_to: args.publish_to.clone(),
        };
        let mut ws = Workspace::new(
            cc.egui_ctx.clone(),
            startup.data,
            startup.settings,
            render,
            dev,
        );
        ws.log(
            Level::Info,
            Source::App,
            tr!(
                "app.started",
                version = env!("CARGO_PKG_VERSION"),
                adapter = ws.render.adapter,
                dir = ws.data.dir.display()
            ),
        );
        for note in ws.render.notes.clone() {
            ws.log(Level::Warning, Source::App, note);
        }
        if let Some(problem) = startup.settings_problem {
            ws.log(Level::Warning, Source::App, problem);
        }
        ws.pending_frame = args.frame;
        ws.pending_layer = args.select_layer.clone();
        ws.show_about = args.show_about;
        if let Some(path) = args.open.clone() {
            ws.open(path);
        }
        let screenshot = args.screenshot.clone().map(|path| {
            let settle = args
                .screenshot_delay
                .map_or(SCREENSHOT_SETTLE, Duration::from_secs_f32);
            Screenshot::new(path, args.quit_after_screenshot, settle)
        });
        Ok(Self {
            dock,
            ws,
            screenshot,
            dump_output: args.dump_output,
            first_frame: true,
        })
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let ws = &mut self.ws;
            if let Some(path) = &ws.opening {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(tr!("status.opening", path = path.display()));
            } else if let Some(doc) = &ws.doc {
                let text = doc.path.display().to_string();
                ws.shaped.label(ui, &text, None);
            } else {
                ui.label(egui::RichText::new(tr!("status.no_document")).weak());
            }
            if ws.jobs.busy() && ws.opening.is_none() {
                ui.separator();
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(tr!("status.working"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mode = match ws.render.active {
                    RenderMode::Gpu => tr!("status.gpu"),
                    RenderMode::Cpu => tr!("status.cpu"),
                };
                ui.label(egui::RichText::new(format!("{mode} · {}", ws.render.adapter)).weak())
                    .on_hover_text(tr!("status.render_hover"));
            });
        });
    }
}

impl eframe::App for JazariApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if std::mem::take(&mut self.first_frame) {
            // egui saves open popups with its memory; a menu left open when
            // the app closed must not reopen at the next start.
            egui::Popup::close_all(&ctx);
        }
        self.ws.poll();

        let dropped: Option<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            self.ws.open(path);
        }
        if self.ws.dev.autostart_test && self.ws.opening.is_none() && self.ws.doc.is_none() {
            self.ws.dev.autostart_test = false;
            self.ws.test_movie();
        }

        menus::shortcuts(&ctx, &mut self.ws, frame);
        let next_step = self.ws.advance_playback();

        egui::Panel::top("jazari-menu").show(ui, |ui| {
            menus::menu_bar(ui, &mut self.ws, &mut self.dock, frame);
        });
        egui::Panel::bottom("jazari-status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let mut viewer = Viewer {
                    ws: &mut self.ws,
                    rs: frame.wgpu_render_state(),
                };
                DockArea::new(&mut self.dock)
                    .id(egui::Id::new("jazari-dock-area"))
                    .style(Style::from_egui(ui.style().as_ref()))
                    .show_inside(ui, &mut viewer);
            });
        if let Some(tab) = self.ws.focus_request.take() {
            dock::focus(&mut self.dock, tab);
        }
        about::window(&ctx, &mut self.ws.show_about);

        if let Some(shot) = self.screenshot.as_mut() {
            let busy = self.ws.busy();
            match shot.update(&ctx, busy) {
                Some(Ok(message)) => {
                    println!("{message}");
                    self.ws.log(Level::Info, Source::App, message);
                }
                Some(Err(message)) => {
                    eprintln!("{message}");
                    self.ws.log(Level::Error, Source::App, message);
                }
                None => {}
            }
        }
        if let Some(after) = next_step {
            ctx.request_repaint_after(after);
        }
        self.ws.shaped.end_frame();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, DOCK_KEY, &self.dock);
    }

    fn on_exit(&mut self) {
        if self.dump_output {
            for e in self.ws.output.iter() {
                let location = e
                    .location
                    .as_ref()
                    .map_or(String::new(), |l| format!(" @{l:?}"));
                let repeats = if e.repeats > 1 {
                    format!(" (x{})", e.repeats)
                } else {
                    String::new()
                };
                println!(
                    "[{:?}/{:?}] {}{repeats}{location}",
                    e.level, e.source, e.text
                );
            }
        }
    }
}
