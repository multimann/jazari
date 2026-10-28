//! Opens a small eframe window that plays a SWF through `PreviewPlayer` on
//! eframe's own device, takes a screenshot of the window after a few
//! seconds, saves it and closes. A reference for wiring the preview into the
//! app, and a check that `wgpu_setup`, `descriptors_from` and the egui
//! texture registration work together.
//!
//! Always runs in CPU mode (WARP / lavapipe / llvmpipe):
//!
//! ```text
//! cargo run -p jazari-preview --example eframe_smoke -- [file.swf] [seconds] [screenshot.png]
//! ```

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use eframe::{egui, egui_wgpu};
use jazari_preview::{
    LogLine, PreviewPlayer, RenderMode, adapter_summary, descriptors_from, is_software_adapter,
    last_adapter_choice, wgpu_setup,
};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let swf_path = args.next().map_or_else(
        || PathBuf::from(r"C:\dev\samples\f8\smoke.swf"),
        PathBuf::from,
    );
    let seconds: f32 = args
        .next()
        .map_or(Ok(3.0), |s| s.parse())
        .context("seconds")?;
    let shot = args.next().map_or_else(
        || PathBuf::from(r"C:\dev\jazari\target\preview-check\eframe-smoke.png"),
        PathBuf::from,
    );
    let swf =
        std::fs::read(&swf_path).with_context(|| format!("reading {}", swf_path.display()))?;

    // Keep eframe's other defaults (LOW_LATENCY surface, wgpu renderer); only
    // the device setup changes.
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("jazari-preview smoke test")
            .with_inner_size([600.0, 480.0]),
        persist_window: false,
        ..Default::default()
    };
    options.wgpu_options.wgpu_setup = wgpu_setup(RenderMode::Cpu);
    if let Some(dir) = shot.parent() {
        std::fs::create_dir_all(dir)?;
        options.persistence_path = Some(dir.join("eframe-smoke.ron"));
    }

    eframe::run_native(
        "jazari-preview-smoke",
        options,
        Box::new(move |cc| {
            let app = Smoke::new(cc, swf, Duration::from_secs_f32(seconds), shot)?;
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow!("eframe: {e}"))
}

struct Smoke {
    player: PreviewPlayer,
    log: Receiver<LogLine>,
    started: Instant,
    last_tick: Instant,
    run_for: Duration,
    shot: PathBuf,
    requested: bool,
    frames: u32,
}

impl Smoke {
    fn new(
        cc: &eframe::CreationContext<'_>,
        swf: Vec<u8>,
        run_for: Duration,
        shot: PathBuf,
    ) -> anyhow::Result<Self> {
        let rs: &egui_wgpu::RenderState = cc
            .wgpu_render_state
            .as_ref()
            .context("eframe did not create a wgpu renderer")?;
        let info = rs.adapter.get_info();
        println!("eframe adapter: {}", adapter_summary(&info));
        println!("selector choice: {:?}", last_adapter_choice());
        if RenderMode::cpu_forced_by_env() && !is_software_adapter(&info) {
            return Err(anyhow!(
                "JAZARI_FORCE_CPU is set but eframe got a hardware adapter"
            ));
        }
        let (tx, log) = mpsc::channel();
        let ppp = cc.egui_ctx.pixels_per_point();
        let player = PreviewPlayer::new(descriptors_from(rs), swf, (550, 400), ppp, tx)?;
        let now = Instant::now();
        Ok(Self {
            player,
            log,
            started: now,
            last_tick: now,
            run_for,
            shot,
            requested: false,
            frames: 0,
        })
    }
}

impl eframe::App for Smoke {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        for line in self.log.try_iter() {
            println!("log {:?}: {}", line.kind, line.text);
        }
        let Some(rs) = frame.wgpu_render_state() else {
            return;
        };
        self.frames += 1;

        egui::CentralPanel::default().show(ui, |ui| {
            let size = ui.available_size();
            let ppp = ui.ctx().pixels_per_point();
            self.player.resize(
                (size.x * ppp).round() as u32,
                (size.y * ppp).round() as u32,
                ppp,
            );
            let now = Instant::now();
            self.player.tick(now - self.last_tick);
            self.last_tick = now;
            // Ruffle submits to the shared queue before eframe submits egui.
            self.player.render();
            let id = self.player.egui_texture(rs);
            let response = ui.add(
                egui::Image::from_texture(egui::load::SizedTexture::new(id, size))
                    .sense(egui::Sense::click_and_drag()),
            );
            self.player.ui_input(ui, &response);
        });

        let screenshot = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = screenshot {
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                Some(png) => match png.save(&self.shot) {
                    Ok(()) => println!("screenshot {}x{} -> {}", w, h, self.shot.display()),
                    Err(e) => println!("screenshot not saved: {e}"),
                },
                None => println!("screenshot had an unexpected size"),
            }
            println!(
                "ran {} egui frames in {:?}; preview frame {:?}, status {:?}, texture {:?}",
                self.frames,
                self.started.elapsed(),
                self.player.current_frame(),
                self.player.status(),
                self.player.size_px()
            );
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else if !self.requested && self.started.elapsed() >= self.run_for {
            self.requested = true;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        } else if self.started.elapsed() >= self.run_for + Duration::from_secs(10) {
            println!("no screenshot arrived; closing");
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let after = self
            .player
            .repaint_after()
            .unwrap_or(Duration::from_millis(100));
        ui.ctx()
            .request_repaint_after(after.min(Duration::from_millis(100)));
    }
}
