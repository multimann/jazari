//! Al-Jazari, the authoring application (binary `jazari`).
//!
//! See `docs/ARCHITECTURE.md`, section 10, and `docs/research/app-shell.md`.
//!
//! Startup: read `settings.toml` from the portable data folder
//! ([`paths`]), resolve the render mode (`JAZARI_FORCE_CPU` always wins and
//! is never saved), and run eframe with `jazari_preview::wgpu_setup`. If the
//! GPU mode cannot create its device, the app restarts once in CPU mode with
//! a notice; there is never a fallback the other way.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
mod i18n;

mod about;
mod app;
mod cli;
mod dialogs;
mod doc;
mod dock;
mod fonts;
mod icons;
mod jobs;
mod library;
mod menus;
mod output;
mod panels;
mod paths;
mod png;
mod raster;
mod screenshot;
mod settings;
mod shaped;
mod timeline_geom;
mod workspace;

use std::io::Write as _;
use std::process::ExitCode;

use eframe::egui;
use jazari_preview::RenderMode;

use crate::app::{JazariApp, Startup};
use crate::paths::DataDir;
use crate::settings::Settings;

/// The application id: the Wayland app id and the `.desktop` file name.
const APP_ID: &str = "org.aljazari.Jazari";

fn main() -> ExitCode {
    let args = match cli::parse(std::env::args_os().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}\n\n{}", cli::usage());
            return ExitCode::from(2);
        }
    };
    if args.help {
        println!("{}", cli::usage());
        return ExitCode::SUCCESS;
    }
    if args.version {
        println!("{} {}", tr!("app.name"), env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    init_logging();

    let data = paths::resolve(&paths::Environment::current(), &paths::ensure_writable);
    let (settings, problem) = Settings::load(&data.settings_file());
    let problem = problem.map(|p| p.to_string());
    let forced = RenderMode::cpu_forced_by_env();
    let mut mode = settings::active_mode(settings.rendering, forced);
    let mut notices = Vec::new();
    let interactive = args.screenshot.is_none();
    loop {
        let startup = Startup {
            data: data.clone(),
            settings: settings.clone(),
            settings_problem: problem.clone(),
            args: args.clone(),
            mode,
            forced,
            notices: notices.clone(),
        };
        match run(startup) {
            Ok(()) => return ExitCode::SUCCESS,
            // One fallback, from GPU to CPU only. The saved preference is
            // left alone: the notice tells the user how to change it.
            Err(eframe::Error::Wgpu(e)) if mode == RenderMode::Gpu => {
                let note = tr!("render.gpu_failed", error = e);
                log::warn!("{note}");
                notices.push(note);
                mode = RenderMode::Cpu;
            }
            Err(e) => {
                fatal(&data, &e.to_string(), interactive);
                return ExitCode::FAILURE;
            }
        }
    }
}

/// Runs eframe once in `startup.mode`.
fn run(startup: Startup) -> eframe::Result {
    let automatic = startup.args.screenshot.is_some();
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tr!("app.name"))
            .with_app_id(APP_ID)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_drag_and_drop(true),
        persistence_path: Some(startup.data.ui_state_file()),
        // Automatic screenshots always use the default window size.
        persist_window: !automatic,
        ..Default::default()
    };
    // Only the device setup changes; eframe's other wgpu defaults (the
    // low-latency surface) stay.
    options.wgpu_options.wgpu_setup = jazari_preview::wgpu_setup(startup.mode);
    eframe::run_native(
        tr!("app.name"),
        options,
        Box::new(move |cc| {
            let app = JazariApp::new(cc, startup)?;
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
}

/// Logs to stderr; `RUST_LOG` overrides the default level.
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Reports an error that stops the app: on stderr, in `jazari.log` in the
/// data folder, and (when a person is watching) in a message box, which
/// may silently fail on Linux without zenity.
fn fatal(data: &DataDir, message: &str, interactive: bool) {
    let text = tr!("app.fatal", error = message);
    eprintln!("{text}");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(data.log_file())
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let _ = writeln!(file, "[{secs}] {text}");
    }
    if interactive {
        let hint = if cfg!(windows) {
            tr!("app.fatal_hint_windows")
        } else {
            tr!("app.fatal_hint_linux")
        };
        let _ = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title(tr!("app.name"))
            .set_description(format!("{text}\n\n{hint}"))
            .show();
    }
}
