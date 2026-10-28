//! The import report: everything the reader skipped, guessed or approximated.

use std::fmt;

/// Warnings collected while reading a document. Reading never aborts on
/// unknown or unsupported content; it is recorded here instead.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportReport {
    /// Warnings in the order they were found.
    pub warnings: Vec<ImportWarning>,
}

impl ImportReport {
    /// Creates an empty report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a warning.
    pub fn warn(&mut self, kind: WarningKind, message: impl Into<String>, location: Location) {
        self.warnings.push(ImportWarning {
            kind,
            message: message.into(),
            location,
        });
    }

    /// Whether there are no warnings.
    pub fn is_empty(&self) -> bool {
        self.warnings.is_empty()
    }

    /// Number of warnings.
    pub fn len(&self) -> usize {
        self.warnings.len()
    }

    /// Iterates over the warnings.
    pub fn iter(&self) -> std::slice::Iter<'_, ImportWarning> {
        self.warnings.iter()
    }
}

impl<'a> IntoIterator for &'a ImportReport {
    type Item = &'a ImportWarning;
    type IntoIter = std::slice::Iter<'a, ImportWarning>;

    fn into_iter(self) -> Self::IntoIter {
        self.warnings.iter()
    }
}

/// One warning of the import report.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportWarning {
    /// What kind of problem this is.
    pub kind: WarningKind,
    /// A human-readable description (English).
    pub message: String,
    /// Where it was found.
    pub location: Location,
}

impl fmt::Display for ImportWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.location.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.location, self.message)
        }
    }
}

/// Categories of import warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarningKind {
    /// An XML element the reader does not know; its subtree was skipped or
    /// kept as [`crate::model::Element::Unsupported`].
    UnknownElement,
    /// A known construct that milestone 1 does not support (shape tweens,
    /// motion guides, sounds, video, TLF text, ...); it is approximated or
    /// left out.
    Unsupported,
    /// Animate-era content (camera, layer depth, object tweens, IK, 3D) that
    /// an AS2 / SWF 8 document cannot express.
    AnimateFeature,
    /// An attribute value that could not be parsed; the default was used.
    BadValue,
    /// A referenced file or library item is missing.
    MissingItem,
    /// Media (bitmap, sound) that could not be decoded.
    BadMedia,
    /// Anything else.
    Other,
}

/// Where something is in a document. Every part is optional; an empty
/// location refers to the document as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Location {
    /// Path of the XML file inside the document, with `/` separators
    /// (for example `DOMDocument.xml` or `LIBRARY/art/Ball.xml`).
    pub file: Option<String>,
    /// The scene or symbol timeline.
    pub timeline: Option<TimelineRef>,
    /// The layer.
    pub layer: Option<LayerRef>,
    /// 0-based frame index.
    pub frame: Option<u32>,
    /// Path of element indices: the element in the frame, then members of
    /// nested groups. Empty when the location is not an element.
    pub element: Vec<usize>,
}

impl Location {
    /// A location in a file.
    pub fn file(file: impl Into<String>) -> Self {
        Self {
            file: Some(file.into()),
            ..Self::default()
        }
    }

    /// Whether no part is set.
    pub fn is_empty(&self) -> bool {
        self.file.is_none()
            && self.timeline.is_none()
            && self.layer.is_none()
            && self.frame.is_none()
            && self.element.is_empty()
    }
}

impl fmt::Display for Location {
    /// Formats as `file › timeline › layer › frame › element`, omitting the
    /// parts that are not set. Frames are shown 1-based, as in the Flash IDE.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(file) = &self.file {
            parts.push(file.clone());
        }
        if let Some(timeline) = &self.timeline {
            parts.push(timeline.to_string());
        }
        if let Some(layer) = &self.layer {
            parts.push(layer.to_string());
        }
        if let Some(frame) = self.frame {
            parts.push(format!("frame {}", u64::from(frame) + 1));
        }
        if !self.element.is_empty() {
            let path: Vec<String> = self.element.iter().map(usize::to_string).collect();
            parts.push(format!("element {}", path.join("/")));
        }
        write!(f, "{}", parts.join(" › "))
    }
}

/// A reference to a timeline of the document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TimelineRef {
    /// A scene: index into [`crate::model::Document::scenes`] and its name.
    Scene {
        /// 0-based scene index.
        index: usize,
        /// Scene name.
        name: String,
    },
    /// The timeline of a library symbol, by its full library path.
    Symbol(String),
}

impl fmt::Display for TimelineRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimelineRef::Scene { name, .. } => write!(f, "scene '{name}'"),
            TimelineRef::Symbol(name) => write!(f, "symbol '{name}'"),
        }
    }
}

/// A reference to a layer of a timeline.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayerRef {
    /// Index into [`crate::model::Timeline::layers`] (0 = top).
    pub index: usize,
    /// Layer name.
    pub name: String,
}

impl fmt::Display for LayerRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "layer '{}'", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_display_skips_missing_parts() {
        assert_eq!(Location::default().to_string(), "");
        let loc = Location {
            file: Some("LIBRARY/Ball.xml".to_owned()),
            timeline: Some(TimelineRef::Symbol("art/Ball".to_owned())),
            layer: Some(LayerRef {
                index: 1,
                name: "art".to_owned(),
            }),
            frame: Some(4),
            element: vec![2, 0],
        };
        assert_eq!(
            loc.to_string(),
            "LIBRARY/Ball.xml › symbol 'art/Ball' › layer 'art' › frame 5 › element 2/0"
        );
    }

    #[test]
    fn report_collects_warnings() {
        let mut report = ImportReport::new();
        assert!(report.is_empty());
        report.warn(
            WarningKind::Unsupported,
            "shape tween published as keyframes",
            Location::file("DOMDocument.xml"),
        );
        assert_eq!(report.len(), 1);
        let w = report.iter().next().map(ToString::to_string);
        assert_eq!(
            w.as_deref(),
            Some("DOMDocument.xml: shape tween published as keyframes")
        );
    }
}
