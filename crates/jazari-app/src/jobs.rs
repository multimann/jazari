//! Work that can take long runs on background threads: reading a document
//! (with its fonts and the stage build) and publishing. Results come back
//! through a channel and are applied on the UI thread, which owns the
//! players.
//!
//! The publisher and the reader are called inside `catch_unwind`: while
//! `jazari-publish` is unfinished it may stop with `todo!()`, and a bug on
//! hostile input must not take the editor down.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui;
use jazari_as2::{Compiler, CompilerOptions};
use jazari_publish::{BuildMode, PublishOptions, PublishOutput};
use jazari_text::FontLibrary;
use jazari_xfl::ImportReport;
use jazari_xfl::model::Document;

/// How a publish ended.
#[derive(Debug)]
pub enum PublishOutcome {
    /// The publisher returned (with or without a SWF).
    Finished(PublishOutput),
    /// The publisher panicked; the message.
    Panicked(String),
}

impl PublishOutcome {
    /// The SWF, if one was produced.
    pub fn swf(&self) -> Option<&[u8]> {
        match self {
            PublishOutcome::Finished(out) => out.swf.as_deref(),
            PublishOutcome::Panicked(_) => None,
        }
    }

    /// Why there is no SWF, for the stage and Test Movie panels.
    pub fn failure(&self) -> Option<String> {
        match self {
            PublishOutcome::Finished(out) if out.swf.is_some() => None,
            PublishOutcome::Finished(out) => {
                Some(tr!("publish.no_swf", count = out.diagnostics.len()))
            }
            PublishOutcome::Panicked(message) => Some(tr!("publish.panicked", message = message)),
        }
    }
}

/// A document read in the background.
pub struct OpenedDoc {
    /// The model.
    pub doc: Arc<Document>,
    /// The reader's warnings.
    pub report: ImportReport,
    /// Fonts for publishing.
    pub fonts: Arc<FontLibrary>,
    /// The compiler.
    pub compiler: Arc<Compiler>,
    /// The stage build.
    pub stage: PublishOutcome,
}

/// What a publish is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Purpose {
    /// Rebuild the stage view.
    Stage,
    /// Test Movie.
    TestMovie,
    /// File › Publish to this path.
    File(PathBuf),
}

/// A finished job.
pub enum JobResult {
    /// A document was read (or could not be).
    Opened {
        /// What was asked for.
        path: PathBuf,
        /// The document, or why it could not be read.
        outcome: Result<Box<OpenedDoc>, String>,
        /// Time taken.
        elapsed: Duration,
        /// The open request's number.
        generation: u64,
    },
    /// A publish finished.
    Published {
        /// The document generation it was made from.
        generation: u64,
        /// What it was for.
        purpose: Purpose,
        /// The result.
        outcome: PublishOutcome,
        /// For [`Purpose::File`]: bytes written, or the write error.
        written: Option<Result<usize, String>>,
        /// Time taken.
        elapsed: Duration,
    },
}

/// The background job runner.
pub struct Jobs {
    tx: Sender<JobResult>,
    rx: Receiver<JobResult>,
    ctx: egui::Context,
    running: usize,
}

impl Jobs {
    /// A runner that wakes `ctx` whenever a job finishes.
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            ctx,
            running: 0,
        }
    }

    /// Whether any job is still running.
    pub fn busy(&self) -> bool {
        self.running > 0
    }

    /// Reads a document, builds its font library and compiler, and makes
    /// the stage build.
    pub fn open(&mut self, path: PathBuf, generation: u64) {
        let fallback_path = path.clone();
        self.spawn(
            "jazari-open",
            move || {
                let start = Instant::now();
                let outcome = guarded(|| open_document(&path)).unwrap_or_else(Err);
                JobResult::Opened {
                    path,
                    outcome,
                    elapsed: start.elapsed(),
                    generation,
                }
            },
            move |message| JobResult::Opened {
                path: fallback_path.clone(),
                outcome: Err(message),
                elapsed: Duration::ZERO,
                generation,
            },
        );
    }

    /// Publishes `doc` for `purpose` (the release build, except for
    /// [`Purpose::Stage`]); for a file, writes it too.
    pub fn publish(
        &mut self,
        generation: u64,
        doc: Arc<Document>,
        fonts: Arc<FontLibrary>,
        compiler: Arc<Compiler>,
        purpose: Purpose,
    ) {
        let fallback_purpose = purpose.clone();
        self.spawn(
            "jazari-publish",
            move || {
                let start = Instant::now();
                let opts = match purpose {
                    Purpose::Stage => stage_options(),
                    _ => PublishOptions {
                        mode: BuildMode::Release,
                        compress: doc.publish.compress,
                    },
                };
                let outcome = run_publish(&doc, &opts, &fonts, &compiler);
                let written = match (&purpose, outcome.swf()) {
                    (Purpose::File(path), Some(swf)) => Some(
                        write_file(path, swf)
                            .map(|()| swf.len())
                            .map_err(|e| e.to_string()),
                    ),
                    _ => None,
                };
                JobResult::Published {
                    generation,
                    purpose,
                    outcome,
                    written,
                    elapsed: start.elapsed(),
                }
            },
            move |message| JobResult::Published {
                generation,
                purpose: fallback_purpose.clone(),
                outcome: PublishOutcome::Panicked(message),
                written: None,
                elapsed: Duration::ZERO,
            },
        );
    }

    /// The jobs that finished since the last call.
    pub fn poll(&mut self) -> Vec<JobResult> {
        let done: Vec<JobResult> = self.rx.try_iter().collect();
        self.running = self.running.saturating_sub(done.len());
        done
    }

    /// Runs `work` on a named thread. If the thread cannot be started, or
    /// the work panics outside the guarded calls, the job ends with
    /// `on_error`'s result instead, so no job is ever left running.
    fn spawn(
        &mut self,
        name: &str,
        work: impl FnOnce() -> JobResult + Send + 'static,
        on_error: impl Fn(String) -> JobResult + Send + Sync + 'static,
    ) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let on_error = Arc::new(on_error);
        let thread_on_error = Arc::clone(&on_error);
        self.running += 1;
        let spawned = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let result = guarded(work).unwrap_or_else(|message| thread_on_error(message));
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        if let Err(e) = spawned {
            let _ = self.tx.send(on_error(tr!("jobs.no_thread", error = e)));
        }
    }
}

/// Options of the stage build: uncompressed, since it never leaves memory.
pub fn stage_options() -> PublishOptions {
    PublishOptions {
        mode: BuildMode::Stage,
        compress: false,
    }
}

/// Calls the publisher inside `catch_unwind`.
pub fn run_publish(
    doc: &Document,
    opts: &PublishOptions,
    fonts: &FontLibrary,
    compiler: &Compiler,
) -> PublishOutcome {
    match guarded(|| jazari_publish::publish(doc, opts, fonts, compiler)) {
        Ok(out) => PublishOutcome::Finished(out),
        Err(message) => PublishOutcome::Panicked(message),
    }
}

/// Reads the document and prepares everything a publish needs.
fn open_document(path: &std::path::Path) -> Result<Box<OpenedDoc>, String> {
    let (doc, report) = jazari_xfl::open(path).map_err(|e| e.to_string())?;
    let base = doc.source.base_dir.clone();
    let mut fonts = FontLibrary::new();
    fonts.add_document_fonts(&base);
    let compiler = Compiler::new(CompilerOptions {
        class_paths: doc
            .publish
            .package_paths
            .iter()
            .map(|p| base.join(p))
            .collect(),
        include_base: Some(base),
        ..CompilerOptions::default()
    });
    let stage = run_publish(&doc, &stage_options(), &fonts, &compiler);
    Ok(Box::new(OpenedDoc {
        doc: Arc::new(doc),
        report,
        fonts: Arc::new(fonts),
        compiler: Arc::new(compiler),
        stage,
    }))
}

/// Writes through a temporary file next to the target.
fn write_file(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Runs `f`, turning a panic into its message.
pub fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| panic_text(&*payload))
}

/// The message of a panic payload.
pub fn panic_text(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        tr!("jobs.unknown_panic").to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panics_become_messages() {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let a = guarded(|| -> u32 { panic!("boom {}", 1) });
        let b = guarded(|| -> u32 { std::panic::panic_any(7u8) });
        std::panic::set_hook(hook);
        assert_eq!(a, Err("boom 1".to_owned()));
        assert!(b.is_err());
        assert_eq!(guarded(|| 5), Ok(5));
    }

    /// Waits for `n` results.
    fn wait(jobs: &mut Jobs, n: usize) -> Vec<JobResult> {
        let mut done = Vec::new();
        let start = Instant::now();
        while done.len() < n && start.elapsed() < Duration::from_secs(120) {
            done.extend(jobs.poll());
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!jobs.busy());
        done
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
    }

    #[test]
    fn documents_open_and_publish_off_the_ui_thread() {
        let smoke = fixture("smoke").join("xfl");
        if !smoke.is_dir() {
            return; // fixtures not checked out
        }
        let mut jobs = Jobs::new(egui::Context::default());
        jobs.open(smoke.clone(), 7);
        jobs.open(fixture("smoke").join("smoke.fla"), 8);
        assert!(jobs.busy());
        let mut results = wait(&mut jobs, 2);
        results.sort_by_key(|r| match r {
            JobResult::Opened { generation, .. } => *generation,
            JobResult::Published { generation, .. } => *generation,
        });
        let opened = match results.remove(0) {
            JobResult::Opened {
                outcome: Ok(doc),
                generation: 7,
                ..
            } => doc,
            _ => panic!("the XFL folder should open"),
        };
        assert!(!opened.doc.scenes.is_empty());
        match &results[0] {
            JobResult::Opened {
                outcome: Err(message),
                generation: 8,
                ..
            } => assert!(message.contains("JPEXS"), "{message}"),
            _ => panic!("a Flash 8 binary FLA is refused"),
        }

        // File > Publish: whatever the publisher does today, the job ends,
        // and a produced SWF is on disk.
        let dir = crate::paths::tests::scratch("jobs-publish");
        let out = dir.join("smoke.swf");
        jobs.publish(
            7,
            Arc::clone(&opened.doc),
            Arc::clone(&opened.fonts),
            Arc::clone(&opened.compiler),
            Purpose::File(out.clone()),
        );
        match wait(&mut jobs, 1).pop() {
            Some(JobResult::Published {
                purpose: Purpose::File(_),
                outcome,
                written,
                ..
            }) => match (outcome.swf(), written) {
                (Some(swf), Some(Ok(n))) => {
                    assert_eq!(n, swf.len());
                    let bytes = std::fs::read(&out).expect("written");
                    assert!(bytes.starts_with(b"CWS") || bytes.starts_with(b"FWS"));
                }
                (None, None) => assert!(outcome.failure().is_some()),
                (swf, written) => panic!("unexpected: {:?} {written:?}", swf.map(<[u8]>::len)),
            },
            _ => panic!("no publish result"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn outcome_messages() {
        let none = PublishOutcome::Finished(PublishOutput::default());
        assert!(none.swf().is_none());
        assert!(none.failure().is_some());
        let some = PublishOutcome::Finished(PublishOutput {
            swf: Some(vec![1, 2, 3]),
            ..PublishOutput::default()
        });
        assert_eq!(some.swf(), Some(&[1u8, 2, 3][..]));
        assert!(some.failure().is_none());
        let panicked = PublishOutcome::Panicked("not yet implemented".into());
        assert!(
            panicked
                .failure()
                .is_some_and(|m| m.contains("not yet implemented"))
        );
    }
}
