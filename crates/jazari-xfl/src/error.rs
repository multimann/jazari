//! Errors that stop a document from being read at all.
//!
//! Anything less serious (unknown elements, bad attribute values, missing
//! media) is not an error: it goes into the [`crate::ImportReport`].

use std::path::PathBuf;

/// A fatal error while opening a document.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A file or folder could not be read.
    #[error("cannot read {}: {source}", .path.display())]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },

    /// A pre-CS5 binary FLA (OLE2 magic `D0 CF 11 E0`).
    #[error(
        "{} is a Flash 8 (or older) binary FLA, which Al-Jazari cannot read; \
         convert it to XFL with JPEXS FLA Convert first",
        .path.display()
    )]
    LegacyFla {
        /// The file.
        path: PathBuf,
    },

    /// Neither an XFL folder, a `.xfl` stub nor a `.fla` ZIP.
    #[error(
        "{} is not an XFL document (expected a folder with DOMDocument.xml, \
         a .xfl file or a CS5+ .fla)",
        .path.display()
    )]
    NotXfl {
        /// The path.
        path: PathBuf,
    },

    /// The `.fla` ZIP archive is damaged.
    #[error("cannot read the archive {}: {source}", .path.display())]
    Zip {
        /// The archive.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: zip::result::ZipError,
    },

    /// The container has no `DOMDocument.xml`.
    #[error("{} has no DOMDocument.xml", .path.display())]
    MissingDocument {
        /// The folder or archive.
        path: PathBuf,
    },

    /// `DOMDocument.xml` is not well-formed XML. (A malformed library file
    /// is only a warning: that symbol is skipped.)
    #[error("malformed XML in {file} at byte {position}: {message}")]
    Xml {
        /// The file inside the document, with `/` separators.
        file: String,
        /// Byte offset of the error.
        position: u64,
        /// The parser's message.
        message: String,
    },
}

/// Result type of this crate.
pub type Result<T> = std::result::Result<T, Error>;
