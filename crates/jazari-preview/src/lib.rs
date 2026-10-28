//! Embedded Ruffle player for Al-Jazari.
//!
//! - [`PreviewPlayer`] runs a published SWF on eframe's own wgpu device and
//!   renders into a texture that egui displays (the stage view and Test
//!   Movie). Ruffle calls are isolated with `catch_unwind`; a crashed player
//!   is rebuilt from the same SWF.
//! - [`OffscreenRenderer`] and [`render_offscreen`] render frames to images on
//!   a private software device (WARP on Windows, lavapipe or llvmpipe on
//!   Linux), for the CLI and golden tests. [`run_headless`] runs a movie with
//!   no renderer at all, to collect its `trace()` output.
//! - [`wgpu_setup`] and [`descriptors_from`] create eframe's device the way
//!   Ruffle needs it and share it with every player.
//!
//! The GPU is never used for rendering when `JAZARI_FORCE_CPU` is set: see
//! [`RenderMode::resolve`].
//!
//! See `docs/ARCHITECTURE.md`, section 9, and `docs/research/ruffle-embed.md`.

#![warn(missing_docs)]

mod device;
mod error;
mod fonts;
mod input;
mod offscreen;
mod output;
mod player;
mod target;

pub use device::{
    AdapterChoice, adapter_summary, descriptors_from, is_software_adapter, last_adapter_choice,
    wgpu_setup,
};
pub use error::PreviewError;
pub use fonts::ARABIC_DEVICE_FONT;
pub use offscreen::{OffscreenOutput, OffscreenRenderer, render_offscreen, run_headless};
pub use player::{GOTO_CALLBACK, PlayerSettings, PlayerStatus, PreviewPlayer};

/// Ruffle's bundle of wgpu objects (instance, adapter, device, queue and
/// pipeline caches). Build it once per device and share it between players.
pub use ruffle_render_wgpu::descriptors::Descriptors;

/// Ruffle's stage quality (anti-aliasing level), used in [`PlayerSettings`].
pub use ruffle_render::quality::StageQuality;

/// The environment variable that forces the software adapter.
pub const FORCE_CPU_ENV: &str = "JAZARI_FORCE_CPU";

/// Which wgpu adapter renders the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderMode {
    /// A hardware adapter.
    Gpu,
    /// The software adapter: WARP on Windows, lavapipe or llvmpipe on Linux.
    Cpu,
}

impl RenderMode {
    /// Whether `JAZARI_FORCE_CPU` asks for CPU mode: set to anything other
    /// than an empty string or `0`.
    pub fn cpu_forced_by_env() -> bool {
        std::env::var_os(FORCE_CPU_ENV).is_some_and(|v| !v.is_empty() && v != "0")
    }

    /// The mode to use for a saved `setting`: [`RenderMode::Cpu`] whenever
    /// `JAZARI_FORCE_CPU` is set, otherwise the setting.
    pub fn resolve(setting: RenderMode) -> RenderMode {
        Self::resolve_with(setting, Self::cpu_forced_by_env())
    }

    fn resolve_with(setting: RenderMode, forced: bool) -> RenderMode {
        if forced { RenderMode::Cpu } else { setting }
    }
}

/// The kind of a line sent to the Output panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogKind {
    /// AS2 `trace()` output.
    Trace,
    /// An AVM warning from Ruffle's log backend (for example an uncaught
    /// exception).
    AvmWarning,
    /// A WARN-or-worse `tracing` event from Ruffle (script timeouts, halts,
    /// invalid bytecode, stubs), which never reaches the log backend.
    Ruffle,
    /// A message from the preview itself: a crash and restart, an AVM halt,
    /// a missing stage-build hook.
    Preview,
}

/// One line for the Output panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Where it comes from.
    pub kind: LogKind,
    /// The text. A `trace()` of a string with line breaks stays one entry.
    pub text: String,
}

impl LogLine {
    /// A line of the given kind.
    pub fn new(kind: LogKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_cpu_wins_over_the_setting() {
        assert_eq!(
            RenderMode::resolve_with(RenderMode::Gpu, true),
            RenderMode::Cpu
        );
        assert_eq!(
            RenderMode::resolve_with(RenderMode::Cpu, true),
            RenderMode::Cpu
        );
        assert_eq!(
            RenderMode::resolve_with(RenderMode::Gpu, false),
            RenderMode::Gpu
        );
    }
}
