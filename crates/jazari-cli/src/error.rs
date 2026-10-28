//! The error type of the command line.

use std::fmt;
use std::path::{Path, PathBuf};

/// What can go wrong in a command. Each variant says which file or step it
/// concerns, so the message alone tells the user what to fix.
#[derive(Debug)]
pub enum CliError {
    /// Bad command line: the message explains it; the exit code is 2.
    Usage(String),
    /// A file or folder could not be read or written.
    Io {
        /// The path concerned.
        path: PathBuf,
        /// What was being done ("reading", "writing", "creating").
        action: &'static str,
        /// The underlying error.
        source: std::io::Error,
    },
    /// A document could not be opened.
    Open {
        /// The document path.
        path: PathBuf,
        /// The reader's error.
        source: jazari_xfl::Error,
    },
    /// A file is not a readable SWF.
    Swf {
        /// The SWF path, or a description such as "our SWF".
        what: String,
        /// The parser's message.
        message: String,
    },
    /// Publishing produced no SWF (the reasons are in the diagnostics,
    /// which the caller has already shown).
    PublishFailed,
    /// Ruffle could not render or run a movie.
    Preview {
        /// Which movie.
        what: String,
        /// The preview's message, with its causes.
        message: String,
    },
    /// A PNG could not be written.
    Image {
        /// The image path.
        path: PathBuf,
        /// The encoder's error.
        source: image::ImageError,
    },
    /// The comparison found differences beyond tolerance (`compare`).
    Mismatch,
}

impl CliError {
    /// An I/O error on `path`.
    pub fn io(path: &Path, action: &'static str, source: std::io::Error) -> Self {
        CliError::Io {
            path: path.to_path_buf(),
            action,
            source,
        }
    }

    /// A preview (Ruffle) failure for the movie `what`.
    pub fn preview(what: impl Into<String>, err: &anyhow::Error) -> Self {
        CliError::Preview {
            what: what.into(),
            message: format!("{err:#}"),
        }
    }

    /// The process exit code for this error: 2 for usage errors, 1 otherwise.
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Usage(_) => 2,
            _ => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usage(msg) => f.write_str(msg),
            CliError::Io {
                path,
                action,
                source,
            } => write!(f, "{action} {}: {source}", path.display()),
            CliError::Open { path, source } => {
                write!(f, "cannot open {}: {source}", path.display())
            }
            CliError::Swf { what, message } => write!(f, "{what} is not a readable SWF: {message}"),
            CliError::PublishFailed => f.write_str("publishing failed"),
            CliError::Preview { what, message } => write!(f, "cannot play {what}: {message}"),
            CliError::Image { path, source } => {
                write!(f, "writing {}: {source}", path.display())
            }
            CliError::Mismatch => f.write_str("the movies differ beyond tolerance"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CliError::Io { source, .. } => Some(source),
            CliError::Open { source, .. } => Some(source),
            CliError::Image { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Result of the command-line functions.
pub type Result<T> = std::result::Result<T, CliError>;
