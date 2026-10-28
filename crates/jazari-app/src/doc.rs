//! The open document and the views onto it: which timeline is shown, where
//! a location in a report points, and how scene frames map to root frames
//! of the published SWF.

use std::path::PathBuf;
use std::sync::Arc;

use jazari_as2::Compiler;
use jazari_publish::{Origin, PublishOutput};
use jazari_text::FontLibrary;
use jazari_xfl::model::{Document, Timeline};
use jazari_xfl::{ImportReport, Location};

/// A timeline of the document: a scene or a symbol's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TimelineRef {
    /// Scene by index into [`Document::scenes`].
    Scene(usize),
    /// Symbol by its full library path.
    Symbol(String),
}

/// A place in the document that a message points at. Every part is
/// optional.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocLocation {
    /// The timeline.
    pub timeline: Option<TimelineRef>,
    /// Layer index in that timeline (0 = top).
    pub layer: Option<usize>,
    /// 0-based frame.
    pub frame: Option<u32>,
    /// 1-based script line, 0 when unknown.
    pub line: u32,
    /// 1-based script column, 0 when unknown.
    pub column: u32,
}

impl DocLocation {
    /// Whether clicking it can select anything.
    pub fn is_useful(&self) -> bool {
        self.timeline.is_some() || self.layer.is_some() || self.frame.is_some()
    }
}

/// An open document with everything needed to publish it.
pub struct OpenDoc {
    /// What the user opened.
    pub path: PathBuf,
    /// The model (shared with publish jobs).
    pub doc: Arc<Document>,
    /// The reader's warnings.
    pub report: ImportReport,
    /// Fonts for publishing (system fonts plus the document's `fonts/`).
    pub fonts: Arc<FontLibrary>,
    /// The AS2 compiler set up for this document.
    pub compiler: Arc<Compiler>,
    /// Root frame map of the last stage build (may be empty).
    pub frame_map: Vec<(usize, u32)>,
    /// Counts opens, so late results of an older document are ignored.
    pub generation: u64,
}

impl OpenDoc {
    /// The timeline `which` refers to, if it exists.
    pub fn timeline(&self, which: &TimelineRef) -> Option<&Timeline> {
        timeline_of(&self.doc, which)
    }

    /// A display name for a timeline.
    pub fn timeline_name(&self, which: &TimelineRef) -> String {
        match which {
            TimelineRef::Scene(i) => self
                .doc
                .scenes
                .get(*i)
                .map_or_else(|| tr!("timeline.scene_n", n = i + 1), |s| s.name.clone()),
            TimelineRef::Symbol(name) => name.clone(),
        }
    }

    /// The 1-based root frame of the SWF that shows `frame` of scene
    /// `scene`; see [`root_frame`].
    pub fn root_frame(&self, scene: usize, frame: u32) -> Option<u32> {
        root_frame(&self.doc, &self.frame_map, scene, frame)
    }

    /// Where an import warning points.
    pub fn locate_import(&self, loc: &Location) -> Option<DocLocation> {
        locate_import(loc)
    }

    /// Where a compiler or publish diagnostic points.
    pub fn locate_origin(&self, origin: &Origin, line: u32, column: u32) -> Option<DocLocation> {
        locate_origin(&self.doc, origin, line, column)
    }

    /// Takes the frame map of a stage build.
    pub fn set_stage_output(&mut self, out: &PublishOutput) {
        self.frame_map = out.frame_map.clone();
    }
}

/// The timeline `which` refers to in `doc`.
pub fn timeline_of<'a>(doc: &'a Document, which: &TimelineRef) -> Option<&'a Timeline> {
    match which {
        TimelineRef::Scene(i) => doc.scenes.get(*i),
        TimelineRef::Symbol(name) => doc.library.symbols.get(name).map(|s| &s.timeline),
    }
}

/// The 1-based root frame for `frame` (0-based) of scene `scene`. Uses the
/// publisher's frame map when it has one; otherwise scenes are taken to be
/// concatenated in order, as the publisher does. `None` past the scene's
/// end.
pub fn root_frame(
    doc: &Document,
    frame_map: &[(usize, u32)],
    scene: usize,
    frame: u32,
) -> Option<u32> {
    if !frame_map.is_empty() {
        return frame_map
            .iter()
            .position(|&(s, f)| s == scene && f == frame)
            .and_then(|i| u32::try_from(i + 1).ok());
    }
    let timeline = doc.scenes.get(scene)?;
    if frame >= timeline.frame_count().max(1) {
        return None;
    }
    let before: u64 = doc.scenes[..scene]
        .iter()
        .map(|s| u64::from(s.frame_count().max(1)))
        .sum();
    u32::try_from(before + u64::from(frame) + 1).ok()
}

/// Where an import warning points.
pub fn locate_import(loc: &Location) -> Option<DocLocation> {
    let timeline = loc.timeline.as_ref().map(|t| match t {
        jazari_xfl::TimelineRef::Scene { index, .. } => TimelineRef::Scene(*index),
        jazari_xfl::TimelineRef::Symbol(name) => TimelineRef::Symbol(name.clone()),
    });
    let out = DocLocation {
        timeline,
        layer: loc.layer.as_ref().map(|l| l.index),
        frame: loc.frame,
        line: 0,
        column: 0,
    };
    out.is_useful().then_some(out)
}

/// Where a compiler or publish diagnostic points. The origin names the
/// timeline as shown to the user (a scene name or a symbol path) and the
/// layer by name; both are looked up in `doc`.
pub fn locate_origin(
    doc: &Document,
    origin: &Origin,
    line: u32,
    column: u32,
) -> Option<DocLocation> {
    let timeline = origin
        .timeline
        .as_deref()
        .and_then(|name| find_timeline(doc, name));
    let layer = match (&timeline, &origin.layer) {
        (Some(t), Some(layer_name)) => {
            timeline_of(doc, t).and_then(|tl| tl.layers.iter().position(|l| l.name == *layer_name))
        }
        _ => None,
    };
    let out = DocLocation {
        timeline,
        layer,
        frame: origin.frame,
        line,
        column,
    };
    out.is_useful().then_some(out)
}

/// A timeline by its display name: a scene name, a symbol path, or either
/// wrapped as `scene 'x'` / `symbol 'x'`.
fn find_timeline(doc: &Document, name: &str) -> Option<TimelineRef> {
    let unquoted = |prefix: &str| {
        name.strip_prefix(prefix)
            .and_then(|r| r.strip_prefix('\''))
            .and_then(|r| r.strip_suffix('\''))
    };
    if let Some(scene) = unquoted("scene ") {
        return doc
            .scenes
            .iter()
            .position(|s| s.name == scene)
            .map(TimelineRef::Scene);
    }
    if let Some(symbol) = unquoted("symbol ") {
        return doc
            .library
            .symbols
            .contains_key(symbol)
            .then(|| TimelineRef::Symbol(symbol.to_owned()));
    }
    if let Some(i) = doc.scenes.iter().position(|s| s.name == name) {
        return Some(TimelineRef::Scene(i));
    }
    doc.library
        .symbols
        .contains_key(name)
        .then(|| TimelineRef::Symbol(name.to_owned()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jazari_xfl::model::{Frame, Layer, Symbol};
    use jazari_xfl::{LayerRef, TimelineRef as XflTimelineRef};

    /// A document with two scenes (10 and 5 frames) and one symbol.
    pub(crate) fn sample_doc() -> Document {
        let layer = |name: &str, frames: u32| Layer {
            name: name.to_owned(),
            frames: vec![Frame {
                index: 0,
                duration: frames,
                ..Frame::default()
            }],
            ..Layer::default()
        };
        let mut doc = Document::default();
        let mut s1 = Timeline::new("Scene 1");
        s1.layers = vec![layer("actions", 10), layer("art", 10)];
        let mut s2 = Timeline::new("Intro");
        s2.layers = vec![layer("طبقة", 5)];
        doc.scenes = vec![s1, s2];
        let mut sym = Symbol {
            name: "ui/Button".to_owned(),
            ..Symbol::default()
        };
        sym.timeline.layers = vec![layer("up", 4)];
        doc.library.symbols.insert(sym.name.clone(), sym);
        doc
    }

    #[test]
    fn root_frames_without_a_frame_map_concatenate_scenes() {
        let doc = sample_doc();
        assert_eq!(root_frame(&doc, &[], 0, 0), Some(1));
        assert_eq!(root_frame(&doc, &[], 0, 9), Some(10));
        assert_eq!(root_frame(&doc, &[], 0, 10), None);
        assert_eq!(root_frame(&doc, &[], 1, 0), Some(11));
        assert_eq!(root_frame(&doc, &[], 1, 4), Some(15));
        assert_eq!(root_frame(&doc, &[], 2, 0), None);
    }

    #[test]
    fn root_frames_follow_the_publishers_frame_map() {
        let doc = sample_doc();
        let map = vec![(1, 0), (1, 1), (0, 0)];
        assert_eq!(root_frame(&doc, &map, 1, 1), Some(2));
        assert_eq!(root_frame(&doc, &map, 0, 0), Some(3));
        assert_eq!(root_frame(&doc, &map, 0, 5), None);
    }

    #[test]
    fn import_locations() {
        let loc = Location {
            file: Some("DOMDocument.xml".into()),
            timeline: Some(XflTimelineRef::Scene {
                index: 1,
                name: "Intro".into(),
            }),
            layer: Some(LayerRef {
                index: 0,
                name: "طبقة".into(),
            }),
            frame: Some(3),
            element: vec![],
        };
        let d = locate_import(&loc).expect("located");
        assert_eq!(d.timeline, Some(TimelineRef::Scene(1)));
        assert_eq!(d.layer, Some(0));
        assert_eq!(d.frame, Some(3));
        assert_eq!(locate_import(&Location::file("PublishSettings.xml")), None);
    }

    #[test]
    fn diagnostic_origins_are_resolved_by_name() {
        let doc = sample_doc();
        let origin = Origin {
            timeline: Some("Scene 1".into()),
            layer: Some("art".into()),
            frame: Some(4),
            ..Origin::default()
        };
        let d = locate_origin(&doc, &origin, 3, 7).expect("located");
        assert_eq!(d.timeline, Some(TimelineRef::Scene(0)));
        assert_eq!(d.layer, Some(1));
        assert_eq!((d.frame, d.line, d.column), (Some(4), 3, 7));

        let symbol = Origin {
            timeline: Some("symbol 'ui/Button'".into()),
            layer: Some("up".into()),
            ..Origin::default()
        };
        let d = locate_origin(&doc, &symbol, 0, 0).expect("located");
        assert_eq!(d.timeline, Some(TimelineRef::Symbol("ui/Button".into())));
        assert_eq!(d.layer, Some(0));

        let unknown = Origin {
            timeline: Some("Nowhere".into()),
            layer: Some("art".into()),
            ..Origin::default()
        };
        assert_eq!(locate_origin(&doc, &unknown, 1, 1), None);
    }
}
