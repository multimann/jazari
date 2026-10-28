//! [`PreviewPlayer`]: a Ruffle player on a shared wgpu device.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::{egui, egui_wgpu};
use ruffle_core::backend::ui::MouseCursor;
use ruffle_core::config::Letterbox;
use ruffle_core::context::UpdateContext;
use ruffle_core::external::{ExternalInterfaceProvider, Value as ExternalValue};
use ruffle_core::limits::ExecutionLimit;
use ruffle_core::tag_utils::SwfMovieData;
use ruffle_core::{
    FloatDuration, LoadBehavior, Player, PlayerBuilder, StageScaleMode, ViewportDimensions,
};
use ruffle_render::quality::StageQuality;
use ruffle_render_wgpu::backend::WgpuRenderBackend;
use ruffle_render_wgpu::descriptors::Descriptors;

use crate::error::panic_message;
use crate::input::{InputAction, InputMapper, Viewport, cursor_icon};
use crate::output::{ChannelLog, RuffleCapture};
use crate::target::{PreviewTarget, read_texture};
use crate::{LogKind, LogLine, PreviewError, fonts};

/// The ExternalInterface callback a stage build registers on root frame 1
/// (`docs/ARCHITECTURE.md`, section 6); [`PreviewPlayer::goto_and_stop`]
/// calls it.
pub const GOTO_CALLBACK: &str = "jz_goto";

/// The URL the movie is loaded from. A `file://` URL gives the local
/// sandbox, as Flash's Test Movie does.
pub(crate) const MOVIE_URL: &str = "file:///jazari/preview.swf";

/// How a player is configured. Kept across rebuilds.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSettings {
    /// Anti-aliasing. `High` (4x MSAA) is Flash's default.
    pub quality: StageQuality,
    /// The Flash Player version Ruffle emulates; `Some(8)` gives Flash
    /// Player 8's AVM1 quirks and `System.capabilities.version`.
    pub player_version: Option<u8>,
    /// How long one script may run before Ruffle stops it and halts the
    /// AVM. Flash Player uses 15 s; the preview runs on the UI thread, so it
    /// uses less by default.
    pub max_execution_duration: Duration,
    /// Register Noto Sans Arabic as the device font
    /// [`ARABIC_DEVICE_FONT`](crate::ARABIC_DEVICE_FONT).
    pub arabic_device_font: bool,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            quality: StageQuality::High,
            player_version: Some(8),
            max_execution_duration: Duration::from_secs(5),
            arabic_device_font: true,
        }
    }
}

/// Whether the player runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerStatus {
    /// Running normally (possibly paused).
    Running,
    /// The AVM halted after a script timeout or invalid bytecode: the
    /// timeline still plays but no script runs again. Offer a restart.
    Halted,
    /// Ruffle panicked and the automatic rebuild failed or was used up. The
    /// texture keeps the last frame. Offer a restart.
    Crashed,
}

/// The texture registered with egui, freed when the player is dropped.
struct EguiTexture {
    id: egui::TextureId,
    generation: u64,
    renderer: Arc<egui::mutex::RwLock<egui_wgpu::Renderer>>,
}

/// A Ruffle player sharing eframe's device, rendering into its own texture.
///
/// The player is `!Send`: create and use it on the UI thread. Every Ruffle
/// call runs inside `catch_unwind` and inside a scoped `tracing` dispatcher
/// that sends Ruffle's warnings to the log channel. After a panic the player
/// is rebuilt from the same SWF once; after an AVM halt the host should
/// offer [`restart`](PreviewPlayer::restart).
///
/// Typical use from `eframe::App::ui` (the player must render before eframe
/// submits egui's frame, which is guaranteed inside `ui`):
///
/// ```ignore
/// player.resize(w_px, h_px, ppp);
/// player.tick(dt);
/// player.render();
/// let id = player.egui_texture(render_state);
/// let response = ui.add(egui::Image::from_texture((id, size)).sense(egui::Sense::click_and_drag()));
/// player.ui_input(ui, &response);
/// if let Some(after) = player.repaint_after() { ui.ctx().request_repaint_after(after); }
/// ```
pub struct PreviewPlayer {
    descriptors: Arc<Descriptors>,
    swf: Arc<[u8]>,
    settings: PlayerSettings,
    log: Sender<LogLine>,
    capture: RuffleCapture,
    player: Option<Arc<Mutex<Player>>>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    generation: u64,
    size_px: (u32, u32),
    ppp: f32,
    playing: bool,
    dirty: bool,
    status: PlayerStatus,
    auto_rebuilds_left: u8,
    warned_no_goto: bool,
    input: InputMapper,
    cursor: MouseCursor,
    egui_texture: Option<EguiTexture>,
}

impl PreviewPlayer {
    /// Loads `swf` into a new player rendering at `size_px` physical pixels
    /// with `ppp` pixels per point, with the default [`PlayerSettings`].
    /// Trace output goes to `log`. The player starts playing; frame 1 has
    /// already run.
    pub fn new(
        desc: Arc<Descriptors>,
        swf: Vec<u8>,
        size_px: (u32, u32),
        ppp: f32,
        log: Sender<LogLine>,
    ) -> anyhow::Result<Self> {
        Self::with_settings(desc, swf, size_px, ppp, log, PlayerSettings::default())
    }

    /// Like [`new`](PreviewPlayer::new), with explicit settings.
    pub fn with_settings(
        desc: Arc<Descriptors>,
        swf: Vec<u8>,
        size_px: (u32, u32),
        ppp: f32,
        log: Sender<LogLine>,
        settings: PlayerSettings,
    ) -> anyhow::Result<Self> {
        let size_px = clamp_size(&desc, size_px);
        let ppp = sane_ppp(ppp);
        let capture = RuffleCapture::new(log.clone());
        let swf: Arc<[u8]> = swf.into();
        let built = build_guarded(&desc, &swf, size_px, ppp, &settings, &log, &capture, true)?;
        let mut this = Self {
            descriptors: desc,
            swf,
            settings,
            log,
            capture,
            player: Some(built.player),
            texture: built.texture,
            view: built.view,
            generation: built.generation,
            size_px,
            ppp,
            playing: true,
            dirty: true,
            status: PlayerStatus::Running,
            auto_rebuilds_left: 1,
            warned_no_goto: false,
            input: InputMapper::default(),
            cursor: MouseCursor::Arrow,
            egui_texture: None,
        };
        this.check_halt();
        Ok(this)
    }

    /// Advances playback by `dt` (does nothing while paused).
    pub fn tick(&mut self, dt: Duration) {
        if self.playing {
            self.call("running a frame", |p| p.tick(FloatDuration::from_std(dt)));
        }
    }

    /// Renders the current frame into the texture if anything changed.
    ///
    /// On eframe's shared device, call it inside `App::ui`, so Ruffle's
    /// commands reach the queue before egui's.
    pub fn render(&mut self) {
        let dirty = std::mem::take(&mut self.dirty);
        self.call("rendering", |p| {
            if dirty || p.needs_render() {
                p.render();
            }
        });
    }

    /// Runs exactly one frame, even while paused, and marks it for
    /// rendering.
    pub fn step_frame(&mut self) {
        self.call("running a frame", |p| p.run_frame());
        self.dirty = true;
    }

    /// The texture view to register with egui. It changes after
    /// [`resize`](PreviewPlayer::resize) and after a rebuild; see
    /// [`texture_generation`](PreviewPlayer::texture_generation) and
    /// [`egui_texture`](PreviewPlayer::egui_texture).
    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Changes whenever [`texture_view`](PreviewPlayer::texture_view) points
    /// at a new texture.
    pub fn texture_generation(&self) -> u64 {
        self.generation
    }

    /// The texture size in physical pixels (after clamping to the device's
    /// maximum texture size).
    pub fn size_px(&self) -> (u32, u32) {
        self.size_px
    }

    /// Registers the texture with egui on the first call, and re-points the
    /// same id after a resize or rebuild. The id is freed when the player is
    /// dropped.
    pub fn egui_texture(&mut self, rs: &egui_wgpu::RenderState) -> egui::TextureId {
        let filter = wgpu::FilterMode::Linear;
        match &mut self.egui_texture {
            Some(tex) if tex.generation == self.generation => tex.id,
            Some(tex) => {
                tex.renderer
                    .write()
                    .update_egui_texture_from_wgpu_texture(&rs.device, &self.view, filter, tex.id);
                tex.generation = self.generation;
                tex.id
            }
            None => {
                let id = rs
                    .renderer
                    .write()
                    .register_native_texture(&rs.device, &self.view, filter);
                self.egui_texture = Some(EguiTexture {
                    id,
                    generation: self.generation,
                    renderer: Arc::clone(&rs.renderer),
                });
                id
            }
        }
    }

    /// Resizes the render target (physical pixels) and the stage viewport.
    /// The size is clamped to `1..=max_texture_dimension_2d`; does nothing
    /// when neither the size nor `ppp` changed (every resize recreates the
    /// texture).
    pub fn resize(&mut self, w: u32, h: u32, ppp: f32) {
        let size = clamp_size(&self.descriptors, (w, h));
        let ppp = sane_ppp(ppp);
        if size == self.size_px && ppp == self.ppp {
            return;
        }
        self.size_px = size;
        self.ppp = ppp;
        let target = self.call("resizing", |p| {
            p.set_viewport_dimensions(ViewportDimensions {
                width: size.0,
                height: size.1,
                scale_factor: f64::from(ppp),
            });
            target_of(p)
        });
        if let Some(Some(target)) = target {
            self.set_target(target);
        }
        self.dirty = true;
    }

    /// Stage build only: shows `frame` (1-based root frame) through the
    /// [`GOTO_CALLBACK`] ExternalInterface callback, then pauses. Logs a
    /// [`LogKind::Preview`] line once if the movie has no such callback.
    /// Frame 0 (a 0-based model frame passed by mistake) is refused with a
    /// [`LogKind::Preview`] line instead of being handed to the movie.
    pub fn goto_and_stop(&mut self, frame: u32) {
        if frame == 0 {
            self.send("goto_and_stop(0) ignored: frame numbers are 1-based");
            return;
        }
        self.playing = false;
        let result = self.call("going to a frame", |p| {
            p.set_is_playing(false);
            let result =
                p.call_internal_interface(GOTO_CALLBACK, [ExternalValue::Number(f64::from(frame))]);
            // Runs the actions the goto queued and refreshes the mouse state.
            p.update(|_| {});
            result
        });
        self.dirty = true;
        if matches!(result, Some(ExternalValue::Null)) && !self.warned_no_goto {
            self.warned_no_goto = true;
            self.send(format!(
                "goto_and_stop({frame}): the movie has no {GOTO_CALLBACK} callback (it is not a stage build)"
            ));
        }
    }

    /// Plays or pauses.
    pub fn set_playing(&mut self, on: bool) {
        self.playing = on;
        self.call("pausing or resuming", |p| p.set_is_playing(on));
    }

    /// Whether the movie is playing.
    pub fn is_playing(&self) -> bool {
        self.playing && self.player.is_some()
    }

    /// The current frame of the root timeline (1-based), if known.
    pub fn current_frame(&self) -> Option<u16> {
        let player = self.player.as_ref()?;
        player.try_lock().ok()?.current_frame()
    }

    /// When the host should repaint: `Some(ZERO)` when a render is pending,
    /// the time until the next frame while playing, and `None` when nothing
    /// will change by itself (paused, halted with nothing to draw, crashed).
    pub fn repaint_after(&self) -> Option<Duration> {
        if self.dirty {
            return Some(Duration::ZERO);
        }
        let player = self.player.as_ref()?.try_lock().ok()?;
        if player.needs_render() {
            Some(Duration::ZERO)
        } else if self.playing {
            Some(player.time_til_next_frame())
        } else {
            None
        }
    }

    /// Forwards an egui input event that happened over `rect` (the image's
    /// screen rectangle) to the player. Key and text events are forwarded
    /// only while the preview is focused (see
    /// [`set_focused`](PreviewPlayer::set_focused)).
    pub fn handle_event(&mut self, ev: &egui::Event, rect: egui::Rect) {
        self.handle_events(std::slice::from_ref(ev), rect);
    }

    /// Forwards several egui events at once (one Ruffle call).
    pub fn handle_events(&mut self, events: &[egui::Event], rect: egui::Rect) {
        self.map_events(events, rect, false);
    }

    /// [`handle_events`](PreviewPlayer::handle_events), with positions
    /// inside `rect` counting as outside when `pointer_blocked`.
    fn map_events(&mut self, events: &[egui::Event], rect: egui::Rect, pointer_blocked: bool) {
        let viewport = Viewport {
            rect,
            size_px: self.size_px,
            ppp: self.ppp,
            pointer_blocked,
        };
        let mut actions = Vec::new();
        for ev in events {
            self.input.map(ev, viewport, &mut actions);
        }
        self.forward(actions);
    }

    /// Gives or takes keyboard focus; sends `FocusGained` / `FocusLost` on a
    /// change and releases held keys when focus is lost.
    pub fn set_focused(&mut self, focused: bool) {
        let mut actions = Vec::new();
        self.input.set_focused(focused, &mut actions);
        self.forward(actions);
    }

    /// Whether the preview has keyboard focus.
    pub fn is_focused(&self) -> bool {
        self.input.focused()
    }

    /// Everything a host needs for input on the image's `response`: takes
    /// focus when a button is pressed over the image (on press, not on
    /// release, so the first keys are not lost), keeps Tab and the arrows
    /// from moving egui's focus, forwards this frame's events and sets the
    /// cursor Ruffle asks for.
    ///
    /// Pointer positions over a part of the image that another egui layer
    /// covers (a menu, a popup) or that is clipped away (a zoomed stage in a
    /// scroll area) count as outside the movie. The image may sit in a
    /// transformed layer (`egui::Scene`): its rectangle is mapped to screen
    /// coordinates, where egui reports pointer events.
    pub fn ui_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let pressed_here = response.hovered() && ui.input(|i| i.pointer.any_pressed());
        if pressed_here {
            response.request_focus();
        }
        let focused = pressed_here || response.has_focus();
        if focused {
            ui.ctx().memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                );
            });
        }
        self.set_focused(focused);
        let events = ui.input(|i| i.events.clone());
        let rect = match ui.ctx().layer_transform_to_global(response.layer_id) {
            Some(to_global) => to_global.mul_rect(response.rect),
            None => response.rect,
        };
        self.map_events(&events, rect, !response.contains_pointer());
        if response.hovered() || self.input.dragging() {
            ui.ctx().set_cursor_icon(self.cursor_icon());
        }
    }

    /// The cursor Ruffle wants over the movie (a hand over buttons).
    pub fn cursor_icon(&self) -> egui::CursorIcon {
        cursor_icon(self.cursor)
    }

    /// Whether the AVM halted or the player crashed; the host should offer
    /// to restart the preview.
    pub fn is_halted(&self) -> bool {
        self.status != PlayerStatus::Running
    }

    /// The player's status.
    pub fn status(&self) -> PlayerStatus {
        self.status
    }

    /// Rebuilds the player from the same SWF, at the current size, and
    /// re-arms the automatic rebuild after a panic.
    pub fn restart(&mut self) -> anyhow::Result<()> {
        self.auto_rebuilds_left = 1;
        self.rebuild()?;
        Ok(())
    }

    /// The adapter the player renders on.
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.descriptors.adapter.get_info()
    }

    /// Reads the texture back into an image (straight alpha). Blocks until
    /// the device is idle, so use it for tests and occasional captures only.
    pub fn capture_image(&self) -> anyhow::Result<image::RgbaImage> {
        let descriptors = &self.descriptors;
        let texture = &self.texture;
        catch_unwind(AssertUnwindSafe(|| read_texture(descriptors, texture))).map_err(|payload| {
            PreviewError::Panicked {
                stage: "reading the frame back",
                message: panic_message(&*payload),
            }
            .into()
        })
    }

    /// Runs `f` on the player inside the capture scope and `catch_unwind`.
    /// A panic (or a poisoned mutex) drops the player and triggers the
    /// rebuild path; `None` is returned then, or when there is no player.
    fn call<R>(&mut self, stage: &'static str, f: impl FnOnce(&mut Player) -> R) -> Option<R> {
        let player = Arc::clone(self.player.as_ref()?);
        let outcome = self.capture.scope(|| {
            catch_unwind(AssertUnwindSafe(|| match player.lock() {
                Ok(mut guard) => Ok(f(&mut guard)),
                Err(_) => Err("the player's mutex was poisoned".to_owned()),
            }))
        });
        let outcome = match outcome {
            Ok(result) => result,
            Err(payload) => Err(panic_message(&*payload)),
        };
        match outcome {
            Ok(value) => {
                self.check_halt();
                Some(value)
            }
            Err(message) => {
                self.crashed(stage, &message);
                None
            }
        }
    }

    /// Applies Ruffle's cursor and the input actions in one call.
    fn forward(&mut self, actions: Vec<InputAction>) {
        if actions.is_empty() {
            return;
        }
        let edits_text = actions.iter().any(|a| {
            matches!(
                a,
                InputAction::Event(
                    ruffle_core::PlayerEvent::KeyDown { .. }
                        | ruffle_core::PlayerEvent::TextInput { .. }
                        | ruffle_core::PlayerEvent::TextControl { .. }
                        | ruffle_core::PlayerEvent::Ime(_)
                )
            )
        });
        let cursor = self.call("handling input", |p| {
            for action in actions {
                match action {
                    InputAction::Event(event) => {
                        p.handle_event(event);
                    }
                    InputAction::MouseInStage(inside) => p.set_mouse_in_stage(inside),
                }
            }
            p.mouse_cursor()
        });
        if let Some(cursor) = cursor {
            self.cursor = cursor;
        }
        // A paused player does not re-render after typing on its own.
        if edits_text {
            self.dirty = true;
        }
    }

    fn check_halt(&mut self) {
        if self.status == PlayerStatus::Running && self.capture.halted() {
            self.status = PlayerStatus::Halted;
            self.send("The movie's scripts were stopped (AVM halted); restart the preview to run them again.");
        }
    }

    fn crashed(&mut self, stage: &'static str, message: &str) {
        self.player = None;
        self.send(format!("Ruffle crashed while {stage}: {message}"));
        if self.auto_rebuilds_left > 0 {
            self.auto_rebuilds_left -= 1;
            match self.rebuild() {
                Ok(()) => self.send("The preview was restarted from frame 1."),
                Err(e) => self.send(format!("The preview could not be restarted: {e}")),
            }
        } else {
            self.status = PlayerStatus::Crashed;
            self.send("The preview stopped; restart it to try again.");
        }
    }

    /// Builds a fresh player from the stored SWF and settings.
    fn rebuild(&mut self) -> Result<(), PreviewError> {
        self.player = None;
        self.capture.reset();
        let built = build_guarded(
            &self.descriptors,
            &self.swf,
            self.size_px,
            self.ppp,
            &self.settings,
            &self.log,
            &self.capture,
            self.playing,
        );
        match built {
            Ok(built) => {
                self.player = Some(built.player);
                self.set_target((built.texture, built.view, built.generation));
                self.status = PlayerStatus::Running;
                self.warned_no_goto = false;
                self.input = InputMapper::default();
                self.cursor = MouseCursor::Arrow;
                self.dirty = true;
                self.check_halt();
                Ok(())
            }
            Err(e) => {
                self.status = PlayerStatus::Crashed;
                Err(e)
            }
        }
    }

    fn set_target(&mut self, (texture, view, generation): (wgpu::Texture, wgpu::TextureView, u64)) {
        self.texture = texture;
        self.view = view;
        self.generation = generation;
    }

    fn send(&self, text: impl Into<String>) {
        let _ = self.log.send(LogLine::new(LogKind::Preview, text));
    }
}

impl Drop for PreviewPlayer {
    fn drop(&mut self) {
        if let Some(tex) = self.egui_texture.take() {
            tex.renderer.write().free_texture(&tex.id);
        }
    }
}

/// Answers the movie's `ExternalInterface.call`s with `undefined`. Having a
/// provider at all is what makes `ExternalInterface.available` true and
/// `addCallback` succeed.
struct HostInterface {
    log: Sender<LogLine>,
}

impl ExternalInterfaceProvider for HostInterface {
    fn call_method(
        &self,
        _context: &mut UpdateContext<'_>,
        name: &str,
        _args: &[ExternalValue],
    ) -> ExternalValue {
        let _ = self.log.send(LogLine::new(
            LogKind::Preview,
            format!("ExternalInterface.call(\"{name}\") has no host in the preview; it returns undefined"),
        ));
        ExternalValue::Undefined
    }

    fn on_callback_available(&self, _name: &str) {}

    fn get_id(&self) -> Option<String> {
        None
    }
}

/// A freshly built player and its target texture.
struct Built {
    player: Arc<Mutex<Player>>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    generation: u64,
}

/// [`build`] inside the capture scope and `catch_unwind`.
#[allow(clippy::too_many_arguments)]
fn build_guarded(
    desc: &Arc<Descriptors>,
    swf: &[u8],
    size_px: (u32, u32),
    ppp: f32,
    settings: &PlayerSettings,
    log: &Sender<LogLine>,
    capture: &RuffleCapture,
    playing: bool,
) -> Result<Built, PreviewError> {
    capture
        .scope(|| {
            catch_unwind(AssertUnwindSafe(|| {
                build(desc, swf, size_px, ppp, settings, log, playing)
            }))
        })
        .unwrap_or_else(|payload| {
            Err(PreviewError::Panicked {
                stage: "loading the movie",
                message: panic_message(&*payload),
            })
        })
}

/// Builds a player: our target, log backend and ExternalInterface provider,
/// show-all scaling with letterboxing, blocking load, an explicit script
/// timeout, autoplay; then registers the Arabic device font and runs frame 1.
fn build(
    desc: &Arc<Descriptors>,
    swf: &[u8],
    (width, height): (u32, u32),
    ppp: f32,
    settings: &PlayerSettings,
    log: &Sender<LogLine>,
    playing: bool,
) -> Result<Built, PreviewError> {
    let movie = SwfMovieData::from_data(swf, MOVIE_URL.to_owned(), None, None)
        .map_err(|e| PreviewError::InvalidMovie(e.to_string()))?;
    let target = PreviewTarget::new(&desc.device, width, height);
    let renderer = WgpuRenderBackend::new(Arc::clone(desc), target)
        .map_err(|e| PreviewError::Renderer(e.to_string()))?;
    let player = PlayerBuilder::new()
        .with_renderer(renderer)
        .with_log(ChannelLog::new(log.clone()))
        .with_external_interface(Box::new(HostInterface { log: log.clone() }))
        .with_movie(movie)
        .with_viewport_dimensions(width, height, f64::from(ppp))
        .with_scale_mode(StageScaleMode::ShowAll, false)
        .with_letterbox(Letterbox::On)
        .with_quality(settings.quality)
        .with_player_version(settings.player_version)
        .with_load_behavior(LoadBehavior::Blocking)
        .with_max_execution_duration(settings.max_execution_duration)
        .with_autoplay(playing)
        .build();
    let target = {
        let mut p = player.lock().map_err(|_| {
            PreviewError::Renderer("the new player's mutex was poisoned".to_owned())
        })?;
        if settings.arabic_device_font {
            fonts::register_arabic(&mut p);
        }
        p.preload(&mut ExecutionLimit::none());
        p.run_frame();
        target_of(&p)
    };
    let (texture, view, generation) = target
        .ok_or_else(|| PreviewError::Renderer("the player lost its preview target".to_owned()))?;
    Ok(Built {
        player,
        texture,
        view,
        generation,
    })
}

/// The current texture of the player's [`PreviewTarget`].
fn target_of(player: &Player) -> Option<(wgpu::Texture, wgpu::TextureView, u64)> {
    let renderer: &dyn Any = player.renderer();
    let backend = renderer.downcast_ref::<WgpuRenderBackend<PreviewTarget>>()?;
    let target = backend.target();
    Some((
        target.texture().clone(),
        target.view().clone(),
        target.generation(),
    ))
}

/// `ppp` when it is a usable scale factor, otherwise 1. A NaN would also
/// never compare equal to itself and recreate the texture on every resize.
fn sane_ppp(ppp: f32) -> f32 {
    if ppp.is_finite() && ppp > 0.0 {
        ppp
    } else {
        1.0
    }
}

/// Clamps a texture size to `1..=max_texture_dimension_2d`, as Ruffle would
/// do silently, so the host's idea of the size stays right.
fn clamp_size(desc: &Descriptors, (w, h): (u32, u32)) -> (u32, u32) {
    let max = desc.limits.max_texture_dimension_2d.max(1);
    (w.clamp(1, max), h.clamp(1, max))
}

#[cfg(test)]
mod tests {
    //! The crash path, which needs a panic inside a Ruffle call. Runs on the
    //! software adapter.

    use super::*;
    use crate::OffscreenRenderer;
    use ruffle_core::swf::{Compression, Header, Rectangle, Tag, Twips};
    use std::sync::mpsc;

    /// A one-frame, 100x100 SWF 8 that does nothing.
    fn empty_movie() -> Vec<u8> {
        let header = Header {
            compression: Compression::None,
            version: 8,
            stage_size: Rectangle {
                x_min: Twips::ZERO,
                x_max: Twips::from_pixels(100.0),
                y_min: Twips::ZERO,
                y_max: Twips::from_pixels(100.0),
            },
            frame_rate: ruffle_core::swf::Fixed8::from_f32(12.0),
            num_frames: 1,
        };
        let mut out = Vec::new();
        ruffle_core::swf::write_swf(&header, &[Tag::ShowFrame], &mut out).expect("SWF");
        out
    }

    #[test]
    fn unusable_pixels_per_point_fall_back_to_one() {
        assert_eq!(sane_ppp(1.5), 1.5);
        assert_eq!(sane_ppp(0.0), 1.0);
        assert_eq!(sane_ppp(-2.0), 1.0);
        assert_eq!(sane_ppp(f32::NAN), 1.0);
        assert_eq!(sane_ppp(f32::INFINITY), 1.0);
    }

    #[test]
    fn a_panic_rebuilds_once_then_stops_until_restart() {
        let renderer = OffscreenRenderer::new().expect("a software adapter (WARP) is required");
        let (tx, rx) = mpsc::channel();
        let mut player =
            PreviewPlayer::new(renderer.descriptors(), empty_movie(), (100, 100), 1.0, tx)
                .expect("player");
        let generation = player.texture_generation();

        // Silence the default hook's stderr output for the deliberate panics.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let first = player.call::<()>("testing", |_| panic!("boom"));
        let after_first = (player.status(), player.texture_generation());
        let second = player.call::<()>("testing", |_| panic!("boom again"));
        std::panic::set_hook(hook);

        assert!(first.is_none());
        assert_eq!(
            after_first.0,
            PlayerStatus::Running,
            "rebuilt after the first panic"
        );
        assert_ne!(after_first.1, generation, "the rebuild made a new texture");
        assert!(second.is_none());
        assert_eq!(player.status(), PlayerStatus::Crashed);
        assert!(player.is_halted());
        // Without a player, calls are no-ops and nothing needs repainting.
        player.tick(Duration::from_secs(1));
        player.render();
        assert_eq!(player.repaint_after(), None);

        let log: Vec<_> = rx.try_iter().map(|l| l.text).collect();
        assert!(
            log.iter()
                .any(|t| t.contains("crashed while testing: boom")),
            "{log:?}"
        );
        assert!(log.iter().any(|t| t.contains("restarted")), "{log:?}");

        player.restart().expect("restart");
        assert_eq!(player.status(), PlayerStatus::Running);
        player.render();
        assert_eq!(
            player.capture_image().expect("readback").dimensions(),
            (100, 100)
        );
    }
}
