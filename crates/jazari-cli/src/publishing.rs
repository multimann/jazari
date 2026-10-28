//! Opening and publishing documents the way the app does: system fonts plus
//! the document's `fonts/` folder, and the document folder plus its
//! `PackagePaths` as AS2 class path.

use std::path::Path;

use jazari_publish::{BuildMode, Diagnostic, PublishOptions, PublishOutput, Severity, publish};
use jazari_text::FontLibrary;
use jazari_xfl::{Document, ImportReport};

use crate::error::{CliError, Result};

/// Opens a document (XFL folder, `.xfl` stub or CS5+ `.fla`).
pub fn open(path: &Path) -> Result<(Document, ImportReport)> {
    jazari_xfl::open(path).map_err(|source| CliError::Open {
        path: path.to_path_buf(),
        source,
    })
}

/// The font library for a document: the installed fonts, the embedded
/// fallbacks and the document's own `fonts/` folder. Scanning the system
/// fonts takes a fraction of a second, so share one library per process
/// and add each document's fonts to it.
pub fn system_fonts() -> FontLibrary {
    FontLibrary::new()
}

/// An AS2 compiler for `doc`: class paths are the document folder and its
/// `PackagePaths`; `#include` is relative to the document folder.
pub fn compiler_for(doc: &Document) -> jazari_as2::Compiler {
    let base = doc.source.base_dir.clone();
    jazari_as2::Compiler::new(jazari_as2::CompilerOptions {
        class_paths: std::iter::once(base.clone())
            .chain(doc.publish.package_paths.iter().map(|p| base.join(p)))
            .collect(),
        include_base: Some(base),
        ..Default::default()
    })
}

/// Publishes `doc` with `fonts` (to which the document's `fonts/` folder
/// is added) and a fresh compiler.
pub fn publish_doc(
    doc: &Document,
    fonts: &mut FontLibrary,
    mode: BuildMode,
    compress: bool,
) -> PublishOutput {
    fonts.add_document_fonts(&doc.source.base_dir);
    let as2 = compiler_for(doc);
    publish(doc, &PublishOptions { mode, compress }, fonts, &as2)
}

/// Counts of diagnostics by severity: (errors, warnings, infos).
pub fn severity_counts(diagnostics: &[Diagnostic]) -> (usize, usize, usize) {
    diagnostics
        .iter()
        .fold((0, 0, 0), |(e, w, i), d| match d.severity {
            Severity::Error => (e + 1, w, i),
            Severity::Warning => (e, w + 1, i),
            Severity::Info => (e, w, i + 1),
        })
}
