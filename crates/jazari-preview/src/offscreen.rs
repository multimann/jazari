//! Offscreen rendering on a private software device, for the CLI and golden
//! tests, and headless runs with no renderer at all for `trace()` capture.

use std::any::Any;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc;

use ruffle_core::backend::locale::DeterministicLocaleBackend;
use ruffle_core::limits::ExecutionLimit;
use ruffle_core::tag_utils::SwfMovieData;
use ruffle_core::{LoadBehavior, Player, PlayerBuilder};
use ruffle_render_wgpu::backend::WgpuRenderBackend;
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::target::TextureTarget;

use crate::device::create_software_descriptors;
use crate::error::panic_message;
use crate::output::{ChannelLog, RuffleCapture};
use crate::player::MOVIE_URL;
use crate::{LogLine, PlayerSettings, PreviewError, fonts};

/// What an offscreen run produced.
#[derive(Debug, Clone)]
pub struct OffscreenOutput {
    /// One image per requested frame, in the order the frames were given.
    /// Straight (not premultiplied) alpha.
    pub images: Vec<image::RgbaImage>,
    /// Everything the movie and Ruffle logged, in order: `trace()` output,
    /// AVM warnings and Ruffle's WARN-or-worse messages.
    pub log: Vec<LogLine>,
    /// Whether the AVM halted (script timeout or invalid bytecode).
    pub halted: bool,
}

/// Renders SWF frames on its own software device: WARP on Windows, lavapipe
/// or llvmpipe on Linux. It never uses a hardware adapter. Create it once
/// and reuse it: the device and Ruffle's pipeline caches are shared by every
/// render.
pub struct OffscreenRenderer {
    descriptors: Arc<Descriptors>,
}

impl OffscreenRenderer {
    /// Creates the software device. Fails when no software adapter exists.
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            descriptors: Arc::new(create_software_descriptors()?),
        })
    }

    /// The software adapter in use.
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.descriptors.adapter.get_info()
    }

    /// The device, for running a [`PreviewPlayer`](crate::PreviewPlayer)
    /// headless (tests).
    pub fn descriptors(&self) -> Arc<Descriptors> {
        Arc::clone(&self.descriptors)
    }

    /// Renders the 1-based `frames` of `swf` at `scale` (1 = the stage size
    /// in pixels) with the default [`PlayerSettings`].
    ///
    /// Frame N is what the movie shows after running N frames from the
    /// start (so scripts that jump around are followed, and a movie that
    /// stops keeps showing its last frame).
    pub fn render(
        &self,
        swf: &[u8],
        frames: &[u32],
        scale: f32,
    ) -> anyhow::Result<OffscreenOutput> {
        self.render_with(swf, frames, scale, &PlayerSettings::default())
    }

    /// Like [`render`](OffscreenRenderer::render), with explicit settings.
    pub fn render_with(
        &self,
        swf: &[u8],
        frames: &[u32],
        scale: f32,
        settings: &PlayerSettings,
    ) -> anyhow::Result<OffscreenOutput> {
        if frames.contains(&0) {
            return Err(PreviewError::FrameZero.into());
        }
        let movie = SwfMovieData::from_data(swf, MOVIE_URL.to_owned(), None, None)
            .map_err(|e| PreviewError::InvalidMovie(e.to_string()))?;
        let max = self.descriptors.limits.max_texture_dimension_2d.max(1);
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let size = |twips_px: f64| ((twips_px * f64::from(scale)).round() as u32).clamp(1, max);
        let (width, height) = (
            size(movie.width().to_pixels()),
            size(movie.height().to_pixels()),
        );

        let (sink, lines) = mpsc::channel();
        let capture = RuffleCapture::new(sink.clone());
        let descriptors = &self.descriptors;
        let run = || -> Result<Vec<image::RgbaImage>, PreviewError> {
            let target = TextureTarget::new(&descriptors.device, (width, height))
                .map_err(|e| PreviewError::Renderer(e.to_string()))?;
            let renderer = WgpuRenderBackend::new(Arc::clone(descriptors), target)
                .map_err(|e| PreviewError::Renderer(e.to_string()))?;
            let player = PlayerBuilder::new()
                .with_renderer(renderer)
                .with_log(ChannelLog::new(sink.clone()))
                .with_locale(DeterministicLocaleBackend::default())
                .with_movie(movie)
                .with_viewport_dimensions(width, height, f64::from(scale))
                .with_quality(settings.quality)
                .with_player_version(settings.player_version)
                .with_load_behavior(LoadBehavior::Blocking)
                .with_max_execution_duration(settings.max_execution_duration)
                .build();
            let mut player = player.lock().map_err(|_| {
                PreviewError::Renderer("the player's mutex was poisoned".to_owned())
            })?;
            if settings.arabic_device_font {
                fonts::register_arabic(&mut player);
            }
            player.preload(&mut ExecutionLimit::none());
            capture_frames(&mut player, frames)
        };
        let images = capture
            .scope(|| catch_unwind(AssertUnwindSafe(run)))
            .unwrap_or_else(|payload| {
                Err(PreviewError::Panicked {
                    stage: "rendering offscreen",
                    message: panic_message(&*payload),
                })
            })?;
        drop(sink);
        Ok(OffscreenOutput {
            images,
            log: lines.try_iter().collect(),
            halted: capture.halted(),
        })
    }
}

/// Runs the frames up to the last one asked for, rendering and reading back
/// the requested ones.
fn capture_frames(
    player: &mut Player,
    frames: &[u32],
) -> Result<Vec<image::RgbaImage>, PreviewError> {
    let wanted: BTreeMap<u32, ()> = frames.iter().map(|f| (*f, ())).collect();
    let last = wanted.keys().next_back().copied().unwrap_or(0);
    let mut captured = BTreeMap::new();
    for frame in 1..=last {
        player.run_frame();
        if wanted.contains_key(&frame) {
            player.render();
            let renderer: &dyn Any = player.renderer();
            let image = renderer
                .downcast_ref::<WgpuRenderBackend<TextureTarget>>()
                .and_then(WgpuRenderBackend::capture_frame)
                .ok_or(PreviewError::Capture { frame })?;
            captured.insert(frame, image);
        }
    }
    frames
        .iter()
        .map(|f| {
            captured
                .get(f)
                .cloned()
                .ok_or(PreviewError::Capture { frame: *f })
        })
        .collect()
}

/// Renders the 1-based `frames` of `swf` at `scale` on a new software device
/// (see [`OffscreenRenderer::render`]). Prefer an [`OffscreenRenderer`] when
/// rendering more than one movie.
pub fn render_offscreen(
    swf: &[u8],
    frames: &[u32],
    scale: f32,
) -> anyhow::Result<Vec<image::RgbaImage>> {
    Ok(OffscreenRenderer::new()?.render(swf, frames, scale)?.images)
}

/// Runs `frames` frames of `swf` with Ruffle's null renderer (no wgpu device
/// at all) and returns what was logged: `trace()` output, AVM warnings and
/// Ruffle's WARN-or-worse messages.
pub fn run_headless(swf: &[u8], frames: u32) -> anyhow::Result<Vec<LogLine>> {
    let settings = PlayerSettings::default();
    let movie = SwfMovieData::from_data(swf, MOVIE_URL.to_owned(), None, None)
        .map_err(|e| PreviewError::InvalidMovie(e.to_string()))?;
    let (sink, lines) = mpsc::channel();
    let capture = RuffleCapture::new(sink.clone());
    let run = || -> Result<(), PreviewError> {
        let player = PlayerBuilder::new()
            .with_log(ChannelLog::new(sink.clone()))
            .with_locale(DeterministicLocaleBackend::default())
            .with_movie(movie)
            .with_player_version(settings.player_version)
            .with_load_behavior(LoadBehavior::Blocking)
            .with_max_execution_duration(settings.max_execution_duration)
            .build();
        let mut player = player
            .lock()
            .map_err(|_| PreviewError::Renderer("the player's mutex was poisoned".to_owned()))?;
        player.preload(&mut ExecutionLimit::none());
        for _ in 0..frames {
            player.run_frame();
        }
        Ok(())
    };
    capture
        .scope(|| catch_unwind(AssertUnwindSafe(run)))
        .unwrap_or_else(|payload| {
            Err(PreviewError::Panicked {
                stage: "running headless",
                message: panic_message(&*payload),
            })
        })?;
    drop(sink);
    Ok(lines.try_iter().collect())
}
