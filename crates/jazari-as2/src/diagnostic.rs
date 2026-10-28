//! Diagnostics and their origins.
//!
//! The same [`Diagnostic`] type carries compiler errors and the publisher's
//! own warnings (`jazari-publish` re-exports it), so the Output panel shows
//! one kind of entry and can jump to its location.

use std::fmt;

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    /// Informational note.
    Info,
    /// Something was approximated or left out; output was still produced.
    Warning,
    /// The script (or element) was skipped.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        })
    }
}

/// Where a script or element comes from in the document.
///
/// Every part is optional. `jazari-publish` fills it from the model; the
/// compiler adds `file` when a diagnostic points into an `#include` or class
/// file rather than the snippet itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Origin {
    /// Scene or symbol, as shown to the user (for example `Scene 1` or
    /// `ui/PlayBtn`).
    pub timeline: Option<String>,
    /// Layer name.
    pub layer: Option<String>,
    /// 0-based frame index.
    pub frame: Option<u32>,
    /// Instance name, or a description of an unnamed instance, for
    /// `on()`/`onClipEvent()` scripts.
    pub instance: Option<String>,
    /// External file (`#include` or AS2 class) the position refers to.
    pub file: Option<String>,
}

impl fmt::Display for Origin {
    /// Formats as `timeline › layer › frame › instance › file`, omitting the
    /// parts that are not set. Frames are shown 1-based, as in the Flash IDE.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(timeline) = &self.timeline {
            parts.push(timeline.clone());
        }
        if let Some(layer) = &self.layer {
            parts.push(format!("layer '{layer}'"));
        }
        if let Some(frame) = self.frame {
            parts.push(format!("frame {}", u64::from(frame) + 1));
        }
        if let Some(instance) = &self.instance {
            parts.push(format!("instance '{instance}'"));
        }
        if let Some(file) = &self.file {
            parts.push(file.clone());
        }
        f.write_str(&parts.join(" › "))
    }
}

/// A compiler or publisher message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity.
    pub severity: Severity,
    /// Message (English).
    pub message: String,
    /// Where it comes from.
    pub origin: Origin,
    /// 1-based line in the script, or 0 when there is no position.
    pub line: u32,
    /// 1-based column in characters (not bytes), or 0 when there is no
    /// position.
    pub column: u32,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.severity)?;
        let origin = self.origin.to_string();
        if !origin.is_empty() {
            write!(f, "{origin}")?;
            if self.line > 0 {
                write!(f, ":{}:{}", self.line, self.column)?;
            }
            f.write_str(": ")?;
        } else if self.line > 0 {
            write!(f, "{}:{}: ", self.line, self.column)?;
        }
        f.write_str(&self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_origin_and_position() {
        let d = Diagnostic {
            severity: Severity::Error,
            message: "unexpected token".to_owned(),
            origin: Origin {
                timeline: Some("Scene 1".to_owned()),
                layer: Some("actions".to_owned()),
                frame: Some(0),
                ..Origin::default()
            },
            line: 3,
            column: 7,
        };
        assert_eq!(
            d.to_string(),
            "error: Scene 1 › layer 'actions' › frame 1:3:7: unexpected token"
        );
    }
}
