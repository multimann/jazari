//! Native file dialogs that do not block the UI.
//!
//! rfd's `AsyncFileDialog` runs the dialog on its own thread and returns a
//! future. The future is polled once per frame with a waker that asks egui
//! to repaint, so the previews keep running while a dialog is open
//! (`docs/research/app-shell.md`, missed point on blocking dialogs).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use eframe::egui;

/// What a dialog was opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogPurpose {
    /// File › Open… (a `.fla`, `.xfl` or `DOMDocument.xml`).
    OpenFile,
    /// File › Open Folder… (an XFL folder).
    OpenFolder,
    /// File › Publish… (where to write the SWF).
    PublishTo,
}

type DialogFuture = Pin<Box<dyn Future<Output = Option<rfd::FileHandle>>>>;

/// Wakes egui when the dialog thread finishes.
struct RepaintWaker(egui::Context);

impl Wake for RepaintWaker {
    fn wake(self: Arc<Self>) {
        self.0.request_repaint();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.request_repaint();
    }
}

/// At most one open dialog.
pub struct Dialogs {
    pending: Option<(DialogPurpose, DialogFuture)>,
    waker: Waker,
    /// A result that arrived on the first poll.
    ready: Option<(DialogPurpose, Option<PathBuf>)>,
}

impl Dialogs {
    /// Dialogs that wake `ctx` when they close.
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            pending: None,
            waker: Waker::from(Arc::new(RepaintWaker(ctx))),
            ready: None,
        }
    }

    /// Whether a dialog is open.
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Asks for a document file.
    pub fn open_file(&mut self, parent: &eframe::Frame, dir: Option<&Path>) {
        let dialog = base(parent, dir, tr!("dialog.open_title"))
            .add_filter(tr!("dialog.filter_documents"), &["fla", "xfl", "xml"])
            .add_filter(tr!("dialog.filter_all"), &["*"]);
        self.start(DialogPurpose::OpenFile, Box::pin(dialog.pick_file()));
    }

    /// Asks for an XFL folder.
    pub fn open_folder(&mut self, parent: &eframe::Frame, dir: Option<&Path>) {
        let dialog = base(parent, dir, tr!("dialog.open_folder_title"));
        self.start(DialogPurpose::OpenFolder, Box::pin(dialog.pick_folder()));
    }

    /// Asks where to publish the SWF.
    pub fn save_swf(&mut self, parent: &eframe::Frame, dir: Option<&Path>, file_name: &str) {
        let dialog = base(parent, dir, tr!("dialog.publish_title"))
            .add_filter(tr!("dialog.filter_swf"), &["swf"])
            .set_file_name(file_name);
        self.start(DialogPurpose::PublishTo, Box::pin(dialog.save_file()));
    }

    /// A dialog that closed since the last call: its purpose and the chosen
    /// path (`None` when cancelled).
    pub fn poll(&mut self) -> Option<(DialogPurpose, Option<PathBuf>)> {
        if let Some(done) = self.ready.take() {
            return Some(done);
        }
        let (purpose, future) = self.pending.as_mut()?;
        let purpose = *purpose;
        let mut cx = Context::from_waker(&self.waker);
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(handle) => {
                self.pending = None;
                Some((purpose, handle.map(|h| h.path().to_path_buf())))
            }
            Poll::Pending => None,
        }
    }

    fn start(&mut self, purpose: DialogPurpose, future: DialogFuture) {
        if self.pending.is_some() {
            return;
        }
        self.pending = Some((purpose, future));
        // The dialog thread starts on the first poll.
        if let Some(done) = self.poll() {
            self.ready = Some(done);
        }
    }
}

fn base(parent: &eframe::Frame, dir: Option<&Path>, title: &str) -> rfd::AsyncFileDialog {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_parent(parent);
    match dir.filter(|d| d.is_dir()) {
        Some(d) => dialog.set_directory(d),
        None => dialog,
    }
}
