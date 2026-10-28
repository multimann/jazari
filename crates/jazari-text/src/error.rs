//! Errors of the font library.

use std::path::PathBuf;

/// An error raised while adding fonts to a [`FontLibrary`](crate::FontLibrary).
///
/// Layout itself never fails: missing glyphs become `.notdef`, and missing
/// faces fall back to the embedded fonts.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A font file could not be read.
    #[error("cannot read font file {path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The data holds no face that could be parsed.
    #[error("no usable font face in {what}")]
    NoFaces {
        /// What was loaded (a path or a description).
        what: String,
    },
}
