//! Everything the panels show and change, apart from the dock layout: the
//! open document, the selection, the two players, the Output log and the
//! background work. Panels call the methods here; results of background
//! jobs are applied here, on the UI thread.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui;
use jazari_preview::{Descriptors, LogKind, LogLine, PreviewPlayer, RenderMode};
use jazari_publish::Diagnostic;

use crate::dialogs::{DialogPurpose, Dialogs};
use crate::doc::{DocLocation, OpenDoc, TimelineRef};
use crate::dock::Tab;
use crate::jobs::{JobResult, Jobs, OpenedDoc, PublishOutcome, Purpose};
use crate::library::{LibNode, build_tree};
use crate::output::{Entry, Filter, Level, OutputLog, Source};
use crate::paths::DataDir;
use crate::settings::{RenderPref, Settings};
use crate::shaped::ShapedText;

/// Fastest playback rate honoured, in frames per second.
const MAX_FPS: f64 = 120.0;

/// The renderer in use and what the Settings menu says about it.
pub struct RenderInfo {
    /// The mode eframe was started with.
    pub active: RenderMode,
    /// `JAZARI_FORCE_CPU` is set.
    pub forced: bool,
    /// The adapter, for example `Microsoft Basic Render Driver (Dx12, Cpu)`.
    pub adapter: String,
    /// A note about how the adapter was chosen (a hardware fallback in CPU
    /// mode, or the GPU-to-CPU fallback at startup).
    pub notes: Vec<String>,
    /// Ruffle's device bundle, shared by every player; `None` when players
    /// must not be created.
    pub descriptors: Option<Arc<Descriptors>>,
}

/// The stage view's state.
#[derive(Debug, Clone, PartialEq)]
pub enum StageStatus {
    /// No document.
    Empty,
    /// The stage build is being made.
    Building,
    /// There is no stage build; the reason.
    Failed(String),
    /// The player runs.
    Ready,
}

/// Stage zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zoom {
    /// Fit the stage into the panel.
    Fit,
    /// A fixed scale (1.0 = 100 %).
    Scale(f32),
}

/// The stage view.
pub struct StageView {
    /// The stage-build player.
    pub player: Option<PreviewPlayer>,
    /// What the panel shows.
    pub status: StageStatus,
    /// Zoom.
    pub zoom: Zoom,
}

/// Test Movie's state.
#[derive(Debug, Clone, PartialEq)]
pub enum MovieStatus {
    /// Nothing tested yet.
    Idle,
    /// Publishing the release build.
    Publishing,
    /// No movie; the reason.
    Failed(String),
    /// The player runs.
    Running,
}

/// The Test Movie view.
pub struct MovieView {
    /// The release-build player.
    pub player: Option<PreviewPlayer>,
    /// What the panel shows.
    pub status: MovieStatus,
    /// Where the movie came from, for the panel's header.
    pub source: String,
    /// When the player last advanced.
    pub last_tick: Option<Instant>,
}

/// Enter-playback of the shown timeline.
#[derive(Debug, Clone, Copy)]
pub struct Playback {
    last: Instant,
    carry: f64,
}

/// Developer options from the command line.
#[derive(Debug, Clone, Default)]
pub struct DevOptions {
    /// Test Movie plays this file instead of a publish.
    pub test_swf: Option<PathBuf>,
    /// The stage view plays this file instead of the stage build.
    pub stage_swf: Option<PathBuf>,
    /// Start Test Movie once the startup document is open.
    pub autostart_test: bool,
    /// Open this menu once (`file`, `control`, `settings`, `window`,
    /// `help`), for screenshots.
    pub open_menu: Option<String>,
    /// Publish to this file once the startup document is open.
    pub publish_to: Option<PathBuf>,
}

/// The application state shared by the panels.
pub struct Workspace {
    /// The data folder.
    pub data: DataDir,
    /// Saved settings.
    pub settings: Settings,
    /// The renderer.
    pub render: RenderInfo,
    /// The open document.
    pub doc: Option<OpenDoc>,
    /// The library tree of the open document.
    pub library: Vec<LibNode>,
    /// A document being opened.
    pub opening: Option<PathBuf>,
    /// Counts open requests.
    generation: u64,
    /// The timeline the Timeline panel shows.
    pub view: TimelineRef,
    /// The playhead of the shown timeline (0-based).
    pub frame: u32,
    /// The selected layer of the shown timeline.
    pub selected_layer: Option<usize>,
    /// The selected frame (0-based); the playhead moves with it.
    pub selected_frame: Option<u32>,
    /// The selected library item (full path).
    pub selected_item: Option<String>,
    /// Line of the Actions panel to highlight (1-based).
    pub highlight_line: Option<u32>,
    /// Enter-playback, while it runs.
    pub playback: Option<Playback>,
    /// The stage view.
    pub stage: StageView,
    /// Test Movie.
    pub movie: MovieView,
    /// The Output panel's log.
    pub output: OutputLog,
    /// The Output panel's filter.
    pub output_filter: Filter,
    /// Shaped text for right-to-left content.
    pub shaped: ShapedText,
    /// Background work.
    pub jobs: Jobs,
    /// File dialogs.
    pub dialogs: Dialogs,
    /// A tab to bring to the front after this frame.
    pub focus_request: Option<Tab>,
    /// The About window is open.
    pub show_about: bool,
    /// Developer options.
    pub dev: DevOptions,
    /// A frame (1-based) to show once the next document is open.
    pub pending_frame: Option<u32>,
    /// The layer (1-based index or name) to select with `pending_frame`.
    pub pending_layer: Option<String>,
    stage_log: (Sender<LogLine>, Receiver<LogLine>),
    movie_log: (Sender<LogLine>, Receiver<LogLine>),
    ctx: egui::Context,
}

impl Workspace {
    /// A workspace with no document.
    pub fn new(
        ctx: egui::Context,
        data: DataDir,
        settings: Settings,
        render: RenderInfo,
        dev: DevOptions,
    ) -> Self {
        Self {
            data,
            settings,
            render,
            doc: None,
            library: Vec::new(),
            opening: None,
            generation: 0,
            view: TimelineRef::Scene(0),
            frame: 0,
            selected_layer: None,
            selected_frame: None,
            selected_item: None,
            highlight_line: None,
            playback: None,
            stage: StageView {
                player: None,
                status: StageStatus::Empty,
                zoom: Zoom::Fit,
            },
            movie: MovieView {
                player: None,
                status: MovieStatus::Idle,
                source: String::new(),
                last_tick: None,
            },
            output: OutputLog::default(),
            output_filter: Filter::default(),
            shaped: ShapedText::default(),
            jobs: Jobs::new(ctx.clone()),
            dialogs: Dialogs::new(ctx.clone()),
            focus_request: None,
            show_about: false,
            dev,
            pending_frame: None,
            pending_layer: None,
            stage_log: mpsc::channel(),
            movie_log: mpsc::channel(),
            ctx,
        }
    }

    /// Adds a line to the Output panel.
    pub fn log(&mut self, level: Level, source: Source, text: impl Into<String>) {
        self.output.push(Entry::new(level, source, text));
    }

    /// Whether background work or a dialog is pending.
    pub fn busy(&self) -> bool {
        self.jobs.busy() || self.dialogs.busy() || self.opening.is_some()
    }

    // ----- Documents ---------------------------------------------------

    /// Starts opening a document in the background.
    pub fn open(&mut self, path: PathBuf) {
        self.generation += 1;
        self.log(
            Level::Info,
            Source::App,
            tr!("output.opening", path = path.display()),
        );
        self.jobs.open(path.clone(), self.generation);
        if self.doc.is_none() {
            self.stage.status = StageStatus::Building;
        }
        self.opening = Some(path);
    }

    /// Closes the document and both players.
    pub fn close(&mut self) {
        self.generation += 1;
        self.doc = None;
        self.library.clear();
        self.view = TimelineRef::Scene(0);
        self.frame = 0;
        self.selected_layer = None;
        self.selected_frame = None;
        self.selected_item = None;
        self.highlight_line = None;
        self.playback = None;
        self.stage.player = None;
        self.stage.status = StageStatus::Empty;
        self.movie.player = None;
        self.movie.status = MovieStatus::Idle;
    }

    /// A folder to start file dialogs in: the open document's.
    pub fn dialog_dir(&self) -> Option<PathBuf> {
        self.doc
            .as_ref()
            .map(|d| d.doc.source.base_dir.clone())
            .or_else(|| {
                self.settings
                    .recent
                    .first()
                    .and_then(|p| p.parent().map(Path::to_path_buf))
            })
    }

    /// Saves the settings, reporting a failure in the Output panel.
    pub fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.data.settings_file()) {
            self.log(Level::Warning, Source::App, e.to_string());
        }
    }

    /// Records the user's renderer choice (applied at the next start).
    pub fn set_render_pref(&mut self, pref: RenderPref) {
        if self.settings.rendering != pref {
            self.settings.rendering = pref;
            self.save_settings();
            self.log(Level::Info, Source::App, tr!("settings.restart_needed"));
        }
    }

    // ----- Background results ------------------------------------------

    /// Applies finished jobs, closed dialogs and player output.
    pub fn poll(&mut self) {
        for result in self.jobs.poll() {
            self.apply_job(result);
        }
        if let Some((purpose, path)) = self.dialogs.poll()
            && let Some(path) = path
        {
            match purpose {
                DialogPurpose::OpenFile | DialogPurpose::OpenFolder => self.open(path),
                DialogPurpose::PublishTo => self.publish_to(path),
            }
        }
        self.drain_player_logs();
    }

    fn apply_job(&mut self, result: JobResult) {
        match result {
            JobResult::Opened {
                path,
                outcome,
                elapsed,
                generation,
            } => {
                if generation != self.generation {
                    return;
                }
                self.opening = None;
                match outcome {
                    Ok(opened) => self.install(path, *opened, elapsed),
                    Err(message) => {
                        let text =
                            tr!("output.open_failed", path = path.display(), error = message);
                        self.log(Level::Error, Source::App, text.clone());
                        if self.doc.is_none() {
                            self.stage.status = StageStatus::Failed(text);
                        }
                        self.focus_request = Some(Tab::Output);
                    }
                }
            }
            JobResult::Published {
                generation,
                purpose,
                outcome,
                written,
                elapsed,
            } => {
                if generation != self.generation {
                    return;
                }
                self.published(purpose, outcome, written, elapsed);
            }
        }
    }

    /// Makes a freshly read document the open one.
    fn install(&mut self, path: PathBuf, opened: OpenedDoc, elapsed: Duration) {
        self.close();
        let OpenedDoc {
            doc,
            report,
            fonts,
            compiler,
            stage,
        } = opened;
        let mut open = OpenDoc {
            path: path.clone(),
            doc,
            report,
            fonts,
            compiler,
            frame_map: Vec::new(),
            generation: self.generation,
        };
        self.log(
            Level::Info,
            Source::App,
            tr!(
                "output.opened",
                path = path.display(),
                ms = elapsed.as_millis(),
                count = open.report.len()
            ),
        );
        let warnings: Vec<Entry> = open
            .report
            .iter()
            .map(|w| {
                Entry::new(Level::Warning, Source::Import, w.to_string())
                    .at(open.locate_import(&w.location))
            })
            .collect();
        for w in warnings {
            self.output.push(w);
        }
        if let PublishOutcome::Finished(out) = &stage {
            open.set_stage_output(out);
        }
        self.library = build_tree(&open.doc);
        // An absolute path with the platform's separators, so the same
        // document is not listed twice under two spellings.
        let absolute = std::path::absolute(&path).unwrap_or_else(|_| path.clone());
        self.settings.add_recent(&absolute);
        self.save_settings();
        self.doc = Some(open);
        self.stage_built(stage);

        if let Some(frame) = self.pending_frame.take() {
            self.go_to_startup_frame(frame);
        }
        if std::mem::take(&mut self.dev.autostart_test) {
            self.test_movie();
        }
        if let Some(path) = self.dev.publish_to.take() {
            self.publish_to(path);
        }
    }

    /// Shows a stage build in the stage view, or why there is none.
    fn stage_built(&mut self, outcome: PublishOutcome) {
        self.report_diagnostics(&outcome, tr!("output.stage_build"));
        if let Some(path) = self.dev.stage_swf.clone() {
            match std::fs::read(&path) {
                Ok(swf) => self.start_stage_player(swf),
                Err(e) => {
                    self.stage.status = StageStatus::Failed(tr!(
                        "output.read_failed",
                        path = path.display(),
                        error = e
                    ));
                }
            }
            return;
        }
        match (outcome.swf(), outcome.failure()) {
            (Some(swf), _) => self.start_stage_player(swf.to_vec()),
            (None, reason) => {
                let reason = reason.unwrap_or_default();
                self.log(
                    Level::Error,
                    Source::Publish,
                    tr!("output.stage_unavailable", reason = reason),
                );
                self.stage.status = StageStatus::Failed(reason);
            }
        }
    }

    fn start_stage_player(&mut self, swf: Vec<u8>) {
        let tx = self.stage_log.0.clone();
        match self.make_player(swf, tx) {
            Ok(mut player) => {
                player.set_playing(false);
                self.stage.player = Some(player);
                self.stage.status = StageStatus::Ready;
                self.sync_stage();
            }
            Err(message) => self.stage.status = StageStatus::Failed(message),
        }
    }

    fn make_player(&self, swf: Vec<u8>, log: Sender<LogLine>) -> Result<PreviewPlayer, String> {
        let Some(desc) = self.render.descriptors.clone() else {
            return Err(tr!("render.no_players").to_owned());
        };
        let size = self.doc.as_ref().map_or((550, 400), |d| {
            (
                d.doc.width.clamp(1.0, 4096.0) as u32,
                d.doc.height.clamp(1.0, 4096.0) as u32,
            )
        });
        PreviewPlayer::new(desc, swf, size, self.ctx.pixels_per_point(), log)
            .map_err(|e| tr!("player.failed", error = e))
    }

    /// Publish results: diagnostics to Output, then the movie or the file.
    fn published(
        &mut self,
        purpose: Purpose,
        outcome: PublishOutcome,
        written: Option<Result<usize, String>>,
        elapsed: Duration,
    ) {
        match purpose {
            Purpose::Stage => {
                if let (Some(doc), PublishOutcome::Finished(out)) = (self.doc.as_mut(), &outcome) {
                    doc.set_stage_output(out);
                }
                self.stage.player = None;
                self.stage_built(outcome);
            }
            Purpose::TestMovie => {
                self.report_diagnostics(&outcome, tr!("output.test_build"));
                match (outcome.swf(), outcome.failure()) {
                    (Some(swf), _) => {
                        let source = tr!("movie.source_published", ms = elapsed.as_millis());
                        self.start_movie(swf.to_vec(), source);
                    }
                    (None, reason) => {
                        let reason = reason.unwrap_or_default();
                        self.log(
                            Level::Error,
                            Source::Publish,
                            tr!("output.test_unavailable", reason = reason),
                        );
                        self.movie.status = MovieStatus::Failed(reason);
                    }
                }
            }
            Purpose::File(path) => {
                self.report_diagnostics(&outcome, tr!("output.publish_build"));
                match (written, outcome.failure()) {
                    (Some(Ok(bytes)), _) => self.log(
                        Level::Info,
                        Source::Publish,
                        tr!("output.published", path = path.display(), bytes = bytes),
                    ),
                    (Some(Err(e)), _) => self.log(
                        Level::Error,
                        Source::Publish,
                        tr!("output.write_failed", path = path.display(), error = e),
                    ),
                    (None, reason) => self.log(
                        Level::Error,
                        Source::Publish,
                        tr!("output.publish_failed", reason = reason.unwrap_or_default()),
                    ),
                }
            }
        }
    }

    /// Adds a build's diagnostics to Output, each with its location.
    fn report_diagnostics(&mut self, outcome: &PublishOutcome, build: &str) {
        let PublishOutcome::Finished(out) = outcome else {
            return;
        };
        let entries: Vec<Entry> = out
            .diagnostics
            .iter()
            .map(|d| self.diagnostic_entry(d, build))
            .collect();
        for e in entries {
            self.output.push(e);
        }
    }

    fn diagnostic_entry(&self, d: &Diagnostic, build: &str) -> Entry {
        let level = match d.severity {
            jazari_publish::Severity::Info => Level::Info,
            jazari_publish::Severity::Warning => Level::Warning,
            jazari_publish::Severity::Error => Level::Error,
        };
        let location = self
            .doc
            .as_ref()
            .and_then(|doc| doc.locate_origin(&d.origin, d.line, d.column));
        Entry::new(level, Source::Publish, format!("{build}: {d}")).at(location)
    }

    /// Moves player output into the Output panel.
    fn drain_player_logs(&mut self) {
        let lines: Vec<(Source, LogLine)> = self
            .stage_log
            .1
            .try_iter()
            .map(|l| (Source::Stage, l))
            .chain(self.movie_log.1.try_iter().map(|l| (Source::TestMovie, l)))
            .collect();
        for (source, line) in lines {
            let level = match line.kind {
                LogKind::Trace => Level::Trace,
                LogKind::AvmWarning | LogKind::Ruffle => Level::Warning,
                LogKind::Preview => Level::Info,
            };
            self.log(level, source, line.text);
        }
    }

    // ----- Test Movie and Publish --------------------------------------

    /// Control › Test Movie: publishes the release build (or loads the
    /// developer SWF) and plays it in the Test Movie panel.
    pub fn test_movie(&mut self) {
        self.focus_request = Some(Tab::TestMovie);
        if let Some(path) = self.dev.test_swf.clone() {
            match std::fs::read(&path) {
                Ok(swf) => {
                    let source = tr!("movie.source_file", path = path.display());
                    self.start_movie(swf, source);
                }
                Err(e) => {
                    let message = tr!("output.read_failed", path = path.display(), error = e);
                    self.log(Level::Error, Source::App, message.clone());
                    self.movie.status = MovieStatus::Failed(message);
                }
            }
            return;
        }
        let Some(doc) = &self.doc else {
            return;
        };
        self.movie.player = None;
        self.movie.status = MovieStatus::Publishing;
        let (d, f, c, g) = (
            Arc::clone(&doc.doc),
            Arc::clone(&doc.fonts),
            Arc::clone(&doc.compiler),
            doc.generation,
        );
        self.jobs.publish(g, d, f, c, Purpose::TestMovie);
    }

    fn start_movie(&mut self, swf: Vec<u8>, source: String) {
        self.movie.player = None;
        let tx = self.movie_log.0.clone();
        match self.make_player(swf, tx) {
            Ok(player) => {
                self.movie.player = Some(player);
                self.movie.last_tick = None;
                self.movie.status = MovieStatus::Running;
                self.movie.source = source;
                self.log(Level::Info, Source::TestMovie, tr!("output.movie_started"));
            }
            Err(message) => {
                self.log(Level::Error, Source::TestMovie, message.clone());
                self.movie.status = MovieStatus::Failed(message);
            }
        }
        self.focus_request = Some(Tab::TestMovie);
    }

    /// Makes the stage build again.
    pub fn rebuild_stage(&mut self) {
        let Some(doc) = &self.doc else {
            return;
        };
        let (d, f, c, g) = (
            Arc::clone(&doc.doc),
            Arc::clone(&doc.fonts),
            Arc::clone(&doc.compiler),
            doc.generation,
        );
        self.stage.player = None;
        self.stage.status = StageStatus::Building;
        self.jobs.publish(g, d, f, c, Purpose::Stage);
    }

    /// Publishes the release build to `path`.
    pub fn publish_to(&mut self, path: PathBuf) {
        let Some(doc) = &self.doc else {
            return;
        };
        let (d, f, c, g) = (
            Arc::clone(&doc.doc),
            Arc::clone(&doc.fonts),
            Arc::clone(&doc.compiler),
            doc.generation,
        );
        self.log(
            Level::Info,
            Source::Publish,
            tr!("output.publishing", path = path.display()),
        );
        self.jobs.publish(g, d, f, c, Purpose::File(path));
    }

    /// The file name File › Publish… suggests.
    pub fn publish_name(&self) -> String {
        let stem = self.doc.as_ref().and_then(|d| {
            let p = &d.path;
            let name = p.file_stem()?.to_string_lossy().into_owned();
            // A folder's own name is a better guess than "DOMDocument" or
            // "document" (the .xfl stub).
            if name.eq_ignore_ascii_case("domdocument") || p.extension().is_some_and(|e| e == "xfl")
            {
                d.doc
                    .source
                    .base_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            } else {
                Some(name)
            }
        });
        format!("{}.swf", stem.unwrap_or_else(|| "movie".to_owned()))
    }

    // ----- Timeline, frames and selection ------------------------------

    /// The stage size in pixels (550 × 400 without a document).
    pub fn stage_size(&self) -> (f32, f32) {
        let sane = |v: f64, default: f32| {
            if v.is_finite() && v >= 1.0 {
                v.min(8192.0) as f32
            } else {
                default
            }
        };
        self.doc.as_ref().map_or((550.0, 400.0), |d| {
            (sane(d.doc.width, 550.0), sane(d.doc.height, 400.0))
        })
    }

    /// Frames per second for playback.
    pub fn fps(&self) -> f64 {
        self.doc
            .as_ref()
            .map(|d| d.doc.frame_rate)
            .filter(|f| f.is_finite() && *f > 0.0)
            .map_or(12.0, |f| f.min(MAX_FPS))
    }

    /// Number of frames of the shown timeline (at least 1).
    pub fn frame_count(&self) -> u32 {
        self.doc
            .as_ref()
            .and_then(|d| d.timeline(&self.view))
            .map_or(1, |t| t.frame_count().max(1))
    }

    /// Moves the playhead of the shown timeline; on a scene, the stage view
    /// follows.
    pub fn set_frame(&mut self, frame: u32) {
        self.frame = frame.min(self.frame_count() - 1);
        self.sync_stage();
    }

    /// Selects a frame cell (and moves the playhead there).
    pub fn select_cell(&mut self, layer: usize, frame: u32) {
        self.selected_layer = Some(layer);
        self.highlight_line = None;
        self.set_frame(frame);
        // The cell follows the playhead, which stops at the last frame.
        self.selected_frame = Some(self.frame);
    }

    /// Shows the stage at the playhead (scene timelines only).
    pub fn sync_stage(&mut self) {
        let TimelineRef::Scene(scene) = self.view else {
            return;
        };
        let Some(root) = self
            .doc
            .as_ref()
            .and_then(|d| d.root_frame(scene, self.frame))
        else {
            return;
        };
        if let Some(player) = self.stage.player.as_mut() {
            player.goto_and_stop(root);
        }
    }

    /// Shows another timeline in the Timeline panel.
    pub fn show_timeline(&mut self, which: TimelineRef) {
        if self.view == which {
            return;
        }
        let exists = self
            .doc
            .as_ref()
            .is_some_and(|d| d.timeline(&which).is_some());
        if !exists {
            return;
        }
        self.view = which;
        self.playback = None;
        self.frame = 0;
        self.selected_layer = None;
        self.selected_frame = None;
        self.highlight_line = None;
        self.sync_stage();
    }

    /// Selects what a message points at.
    pub fn select_location(&mut self, loc: &DocLocation) {
        if let Some(t) = &loc.timeline {
            self.show_timeline(t.clone());
        }
        if let Some(layer) = loc.layer {
            self.selected_layer = Some(layer);
        }
        if let Some(frame) = loc.frame {
            self.selected_frame = Some(frame);
            self.set_frame(frame);
        }
        if loc.line > 0 {
            self.highlight_line = Some(loc.line);
            self.focus_request = Some(Tab::Actions);
        } else {
            self.highlight_line = None;
            self.focus_request = Some(Tab::Timeline);
        }
    }

    /// `--frame N`: shows root frame N of the movie (in the scene that holds
    /// it) and selects a layer:
    /// the one `--select-layer` names, else the first with a script at that
    /// frame, else the top one, so the panels have something to show.
    fn go_to_startup_frame(&mut self, frame_1_based: u32) {
        // A root frame of the movie: scenes follow each other, so a frame
        // past the first scene lands in the scene that holds it.
        let root = frame_1_based.saturating_sub(1);
        let (scene, frame) = self
            .doc
            .as_ref()
            .and_then(|d| d.frame_map.get(root as usize).copied())
            .unwrap_or((0, root));
        self.show_timeline(TimelineRef::Scene(scene));
        let wanted = self.pending_layer.take();
        let layer = self.doc.as_ref().and_then(|d| {
            let tl = d.timeline(&TimelineRef::Scene(scene))?;
            if let Some(w) = &wanted {
                let by_index = w
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| n.checked_sub(1))
                    .filter(|&i| i < tl.layers.len());
                return by_index.or_else(|| tl.layers.iter().position(|l| l.name == *w));
            }
            let with_script = tl
                .layers
                .iter()
                .position(|l| l.frame_at(frame).is_some_and(|f| f.script.is_some()));
            with_script.or((!tl.layers.is_empty()).then_some(0))
        });
        match layer {
            Some(l) => self.select_cell(l, frame),
            None => self.set_frame(frame),
        }
    }

    // ----- Playback ------------------------------------------------------

    /// Enter: plays the shown timeline from the playhead, or pauses. At the
    /// last frame it starts again from the first.
    pub fn toggle_play(&mut self) {
        if self.playback.take().is_some() {
            return;
        }
        if self.doc.is_none() {
            return;
        }
        if self.frame + 1 >= self.frame_count() {
            self.set_frame(0);
        }
        self.playback = Some(Playback {
            last: Instant::now(),
            carry: 0.0,
        });
    }

    /// Control › Rewind.
    pub fn rewind(&mut self) {
        self.playback = None;
        self.set_frame(0);
    }

    /// Steps the playhead by `delta` frames.
    pub fn step(&mut self, delta: i64) {
        self.playback = None;
        let target = (i64::from(self.frame) + delta).clamp(0, i64::from(u32::MAX)) as u32;
        self.set_frame(target);
    }

    /// Advances Enter-playback; returns when it next needs to run.
    pub fn advance_playback(&mut self) -> Option<Duration> {
        let mut pb = self.playback?;
        let fps = self.fps();
        let now = Instant::now();
        let elapsed = now.duration_since(pb.last).as_secs_f64() + pb.carry;
        pb.last = now;
        let steps = (elapsed * fps).floor();
        pb.carry = elapsed - steps / fps;
        if steps >= 1.0 {
            let last = self.frame_count() - 1;
            let target = (f64::from(self.frame) + steps).min(f64::from(last)) as u32;
            self.set_frame(target);
            if target >= last {
                self.playback = None;
                return None;
            }
        }
        self.playback = Some(pb);
        let until_next = ((1.0 - pb.carry * fps) / fps).max(0.001);
        Some(Duration::from_secs_f64(until_next))
    }
}

#[cfg(test)]
mod tests {
    //! The workspace without a window: egui's context runs headless, and with
    //! no device bundle no player is ever created.

    use super::*;
    use crate::doc::tests::sample_doc;
    use crate::paths::{DataDirKind, tests::scratch};
    use jazari_as2::{Compiler, CompilerOptions, Origin, Severity};
    use jazari_publish::PublishOutput;
    use jazari_text::FontLibrary;
    use jazari_xfl::ImportReport;

    fn workspace(name: &str) -> Workspace {
        let data = DataDir {
            dir: scratch(name),
            kind: DataDirKind::Env,
        };
        let render = RenderInfo {
            active: RenderMode::Cpu,
            forced: true,
            adapter: "test".into(),
            notes: Vec::new(),
            descriptors: None,
        };
        Workspace::new(
            egui::Context::default(),
            data,
            Settings::default(),
            render,
            DevOptions::default(),
        )
    }

    fn opened(stage: PublishOutcome) -> OpenedDoc {
        OpenedDoc {
            doc: Arc::new(sample_doc()),
            report: ImportReport::new(),
            fonts: Arc::new(FontLibrary::embedded_only()),
            compiler: Arc::new(Compiler::new(CompilerOptions::default())),
            stage,
        }
    }

    #[test]
    fn a_failed_stage_build_leaves_the_rest_usable() {
        let mut ws = workspace("ws-install");
        let path = PathBuf::from("/docs/sample");
        ws.install(
            path.clone(),
            opened(PublishOutcome::Panicked("not yet implemented".into())),
            Duration::ZERO,
        );
        assert!(ws.doc.is_some());
        assert!(
            matches!(&ws.stage.status, StageStatus::Failed(m) if m.contains("not yet implemented")),
            "{:?}",
            ws.stage.status
        );
        assert!(ws.stage.player.is_none());
        assert_eq!(ws.library.len(), 1, "one folder holding the symbol");
        assert_eq!(ws.view, TimelineRef::Scene(0));
        assert!(
            ws.settings
                .recent
                .first()
                .is_some_and(|p| p.ends_with("sample"))
        );
        assert!(ws.data.settings_file().is_file(), "recent list saved");
        assert!(ws.output.iter().any(|e| e.level == Level::Error));
        // Test Movie fails the same way without a player.
        ws.movie.status = MovieStatus::Idle;
        ws.published(
            Purpose::TestMovie,
            PublishOutcome::Finished(PublishOutput::default()),
            None,
            Duration::ZERO,
        );
        assert!(matches!(ws.movie.status, MovieStatus::Failed(_)));
        let _ = std::fs::remove_dir_all(&ws.data.dir);
    }

    #[test]
    fn diagnostics_are_located_and_selecting_them_moves_the_views() {
        let mut ws = workspace("ws-locate");
        let diag = jazari_publish::Diagnostic {
            severity: Severity::Error,
            message: "unexpected token".into(),
            origin: Origin {
                timeline: Some("Intro".into()),
                layer: Some("طبقة".into()),
                frame: Some(3),
                ..Origin::default()
            },
            line: 2,
            column: 5,
        };
        let out = PublishOutput {
            diagnostics: vec![diag],
            ..PublishOutput::default()
        };
        ws.install(
            PathBuf::from("/docs/sample"),
            opened(PublishOutcome::Finished(out)),
            Duration::ZERO,
        );
        let located = ws
            .output
            .iter()
            .find(|e| e.text.contains("unexpected token"))
            .and_then(|e| e.location.clone())
            .expect("located diagnostic");
        ws.select_location(&located);
        assert_eq!(ws.view, TimelineRef::Scene(1));
        assert_eq!(ws.selected_layer, Some(0));
        assert_eq!((ws.frame, ws.selected_frame), (3, Some(3)));
        assert_eq!(ws.highlight_line, Some(2));
        assert_eq!(ws.focus_request, Some(Tab::Actions));
        let _ = std::fs::remove_dir_all(&ws.data.dir);
    }

    #[test]
    fn frames_playback_and_timelines() {
        let mut ws = workspace("ws-frames");
        ws.install(
            PathBuf::from("/docs/sample"),
            opened(PublishOutcome::Panicked("x".into())),
            Duration::ZERO,
        );
        assert_eq!(ws.frame_count(), 10);
        ws.set_frame(500);
        assert_eq!(ws.frame, 9, "clamped to the last frame");
        ws.step(-3);
        assert_eq!(ws.frame, 6);
        ws.step(-100);
        assert_eq!(ws.frame, 0);

        // Enter at the last frame starts again from the first.
        ws.set_frame(9);
        ws.toggle_play();
        assert_eq!(ws.frame, 0);
        // Half a second at 24 fps runs off the end: playback stops there.
        ws.playback = Some(Playback {
            last: Instant::now() - Duration::from_millis(500),
            carry: 0.0,
        });
        assert_eq!(ws.advance_playback(), None);
        assert_eq!(ws.frame, 9);
        assert!(ws.playback.is_none());
        // A short step advances one frame and keeps playing.
        ws.set_frame(0);
        ws.playback = Some(Playback {
            last: Instant::now() - Duration::from_millis(50),
            carry: 0.0,
        });
        assert!(ws.advance_playback().is_some());
        assert_eq!(ws.frame, 1);
        ws.toggle_play();
        assert!(ws.playback.is_none());

        ws.show_timeline(TimelineRef::Symbol("ui/Button".into()));
        assert_eq!(ws.view, TimelineRef::Symbol("ui/Button".into()));
        assert_eq!(ws.frame_count(), 4);
        ws.show_timeline(TimelineRef::Symbol("missing".into()));
        assert_eq!(ws.view, TimelineRef::Symbol("ui/Button".into()));
        ws.rewind();
        assert_eq!(ws.frame, 0);
        let _ = std::fs::remove_dir_all(&ws.data.dir);
    }

    #[test]
    fn startup_frame_and_layer() {
        let mut ws = workspace("ws-startup");
        ws.pending_frame = Some(4);
        ws.pending_layer = Some("art".into());
        ws.install(
            PathBuf::from("/docs/sample"),
            opened(PublishOutcome::Panicked("x".into())),
            Duration::ZERO,
        );
        assert_eq!(ws.selected_layer, Some(1));
        assert_eq!((ws.frame, ws.selected_frame), (3, Some(3)));
        assert!(ws.pending_frame.is_none() && ws.pending_layer.is_none());
        assert_eq!(ws.publish_name(), "sample.swf");
        ws.close();
        assert!(ws.doc.is_none());
        assert_eq!(ws.stage.status, StageStatus::Empty);
        let _ = std::fs::remove_dir_all(&ws.data.dir);
    }
}
