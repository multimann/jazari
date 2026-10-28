//! Publishes an Al-Jazari [`Document`] as SWF version 8 bytes.
//!
//! Two builds: [`BuildMode::Release`] for Test Movie and File › Publish, and
//! [`BuildMode::Stage`] for the editor's stage view (user scripts omitted,
//! every clip stopped, a `jz_goto` callback on the root). A failed script or
//! element is reported and skipped; the publish continues.
//!
//! See `docs/ARCHITECTURE.md`, section 6, and `docs/research/swf-writer.md`.

#![warn(missing_docs)]
// Skeleton: remove once the bodies are implemented.
#![allow(unused_variables, dead_code)]

use jazari_xfl::Document;

/// Compiler diagnostics and publish warnings share one type.
pub use jazari_as2::{Diagnostic, Origin, Severity};

/// Which SWF to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BuildMode {
    /// The real movie, with all scripts.
    #[default]
    Release,
    /// The stage-view build: no user scripts, `stop()` everywhere, and an
    /// `ExternalInterface` callback `jz_goto(frame)` on the root timeline.
    Stage,
}

/// Options of [`publish`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublishOptions {
    /// Release or stage build.
    pub mode: BuildMode,
    /// zlib-compress the SWF (`CWS`).
    pub compress: bool,
}

impl Default for PublishOptions {
    fn default() -> Self {
        Self {
            mode: BuildMode::Release,
            compress: true,
        }
    }
}

/// The result of [`publish`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PublishOutput {
    /// The SWF bytes, or `None` when publishing failed as a whole (the
    /// reason is in `diagnostics`).
    pub swf: Option<Vec<u8>>,
    /// Compiler errors and publish warnings, with their origins.
    pub diagnostics: Vec<Diagnostic>,
    /// For each root frame of the SWF, in order: the scene index and the
    /// 0-based frame within that scene (scenes are concatenated).
    pub frame_map: Vec<(usize, u32)>,
}

/// Publishes a document. Never panics on document content.
pub fn publish(
    doc: &Document,
    opts: &PublishOptions,
    fonts: &jazari_text::FontLibrary,
    as2: &jazari_as2::Compiler,
) -> PublishOutput {
    todo!("jazari-publish: publish")
}
