//! `--screenshot`: once the window has been idle for a while (no document
//! being opened, nothing published, no dialog), capture it with eframe's
//! screenshot command, save a PNG and optionally close the app.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::png;

/// Longest wait for the window to settle before capturing anyway.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(180);
/// Longest wait for eframe to deliver the screenshot.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(20);

enum State {
    Waiting,
    Requested(Instant),
    Done,
}

/// The pending screenshot.
pub struct Screenshot {
    path: PathBuf,
    quit: bool,
    settle: Duration,
    started: Instant,
    idle_since: Option<Instant>,
    state: State,
}

impl Screenshot {
    /// A screenshot to `path` after `settle` of idle time.
    pub fn new(path: PathBuf, quit: bool, settle: Duration) -> Self {
        Self {
            path,
            quit,
            settle,
            started: Instant::now(),
            idle_since: None,
            state: State::Waiting,
        }
    }

    /// Advances; call once per frame. Returns a message for the Output
    /// panel when the screenshot was saved or failed.
    pub fn update(&mut self, ctx: &egui::Context, busy: bool) -> Option<Result<String, String>> {
        let now = Instant::now();
        match self.state {
            State::Done => return None,
            State::Waiting => {
                let idle = if busy {
                    self.idle_since = None;
                    false
                } else {
                    now.duration_since(*self.idle_since.get_or_insert(now)) >= self.settle
                };
                if idle || now.duration_since(self.started) >= SETTLE_TIMEOUT {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                    self.state = State::Requested(now);
                }
            }
            State::Requested(at) => {
                let image = ctx.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(image) = image {
                    self.state = State::Done;
                    let result = self.save(&image);
                    self.finish(ctx);
                    return Some(result);
                }
                if now.duration_since(at) >= DELIVERY_TIMEOUT {
                    self.state = State::Done;
                    self.finish(ctx);
                    return Some(Err(tr!("screenshot.timeout").to_owned()));
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        None
    }

    fn save(&self, image: &egui::ColorImage) -> Result<String, String> {
        let [w, h] = image.size;
        let rgba: Vec<u8> = image
            .pixels
            .iter()
            .flat_map(|c| c.to_srgba_unmultiplied())
            .collect();
        let bytes = png::encode_rgba(w, h, &rgba).map_err(|e| e.to_string())?;
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.path, bytes).map_err(|e| {
            tr!(
                "screenshot.write_failed",
                path = self.path.display(),
                error = e
            )
        })?;
        Ok(tr!(
            "screenshot.saved",
            path = self.path.display(),
            width = w,
            height = h
        ))
    }

    fn finish(&self, ctx: &egui::Context) {
        if self.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
