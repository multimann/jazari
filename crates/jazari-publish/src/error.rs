//! Internal publish errors.
//!
//! Document problems never become errors: they are reported as located
//! [`crate::Diagnostic`]s and the element is skipped. A [`PublishError`] means
//! that one tag could not be written or did not survive its own round trip,
//! which is a bug in this crate or a value the writer cannot represent.

use thiserror::Error;

/// A tag or movie that could not be produced.
#[derive(Debug, Error)]
pub enum PublishError {
    /// The `swf` writer refused a tag (for example a bit field overflow).
    #[error("cannot encode {tag}: {source}")]
    Encode {
        /// The tag kind.
        tag: &'static str,
        /// The writer's error.
        source: swf::error::Error,
    },
    /// A tag read back from its own bytes differs from what was written: the
    /// writer truncated something silently.
    #[error("{tag} does not survive a write/read round trip: {detail}")]
    RoundTrip {
        /// The tag kind.
        tag: &'static str,
        /// What differed.
        detail: String,
    },
    /// Every character id (1..=65534) is in use.
    #[error("too many characters: SWF character ids are 16-bit")]
    OutOfIds,
    /// A value does not fit the field the SWF format gives it.
    #[error("{0}")]
    OutOfRange(String),
}
