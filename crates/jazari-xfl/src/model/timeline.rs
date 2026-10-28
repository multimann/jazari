//! Timelines, layers, keyframes and tweens.

use super::{Element, Rgba};

/// A timeline: a scene of the document or the timeline of a symbol.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Timeline {
    /// `DOMTimeline@name` (the scene name, or the symbol's timeline name).
    pub name: String,
    /// Layers in XFL order: index 0 is the **top** layer.
    pub layers: Vec<Layer>,
}

impl Timeline {
    /// Creates an empty timeline.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            layers: Vec::new(),
        }
    }

    /// Number of frames: the end of the longest layer, counting every layer
    /// kind (guide layers included, as the Flash timeline does). Zero for a
    /// timeline without frames.
    pub fn frame_count(&self) -> u32 {
        self.layers
            .iter()
            .map(Layer::frame_count)
            .max()
            .unwrap_or(0)
    }
}

/// A layer of a timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// `DOMLayer@name`.
    pub name: String,
    /// Layer type.
    pub kind: LayerKind,
    /// `parentLayerIndex`: index, in the same timeline's `layers`, of the
    /// mask, guide or folder layer this layer belongs to. Walk the chain
    /// through folders to find the effective mask or guide.
    pub parent: Option<usize>,
    /// `visible` (default true).
    pub visible: bool,
    /// `locked` (editor state).
    pub locked: bool,
    /// `outline` (editor state: show as outlines).
    pub outline: bool,
    /// The layer's outline colour in the editor.
    pub color: Rgba,
    /// Keyframe spans, sorted by [`Frame::index`], without overlaps. Gaps are
    /// allowed and show nothing.
    pub frames: Vec<Frame>,
}

impl Default for Layer {
    /// A visible, unlocked normal layer without frames.
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: LayerKind::Normal,
            parent: None,
            visible: true,
            locked: false,
            outline: false,
            color: Rgba::BLACK,
            frames: Vec::new(),
        }
    }
}

impl Layer {
    /// The keyframe span that contains frame `i` (0-based), or `None` when
    /// `i` falls in a gap or past the end of the layer.
    pub fn frame_at(&self, i: u32) -> Option<&Frame> {
        self.keyframe_index_at(i).map(|k| &self.frames[k])
    }

    /// Index into [`Layer::frames`] of the span that contains frame `i`.
    pub fn keyframe_index_at(&self, i: u32) -> Option<usize> {
        // Number of spans that start at or before `i`; the candidate is the
        // last of them.
        let starts_before = self.frames.partition_point(|f| f.index <= i);
        let k = starts_before.checked_sub(1)?;
        self.frames[k].contains(i).then_some(k)
    }

    /// Number of frames in this layer: the end of its last span.
    pub fn frame_count(&self) -> u32 {
        self.frames.iter().map(Frame::end).max().unwrap_or(0)
    }
}

/// The type of a layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum LayerKind {
    /// An ordinary layer (it may still be masked or guided via `parent`).
    #[default]
    Normal,
    /// A guide layer: never published.
    Guide,
    /// A mask layer: its content clips the layers whose `parent` points here.
    Mask,
    /// A layer folder: has no frames of its own.
    Folder,
    /// Any other `layerType` (for example Animate's `camera`), kept verbatim.
    /// The reader adds a warning.
    Other(String),
}

/// A keyframe and the span of frames it covers: `[index, index + duration)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// 0-based first frame of the span (`DOMFrame@index`).
    pub index: u32,
    /// Number of frames in the span (`DOMFrame@duration`, default 1).
    pub duration: u32,
    /// Frame label, if any.
    pub label: Option<FrameLabel>,
    /// Frame script (CR and CRLF normalised to LF), if any.
    pub script: Option<String>,
    /// Tween from this keyframe to the next one.
    pub tween: Tween,
    /// Elements in XFL order: index 0 is the **bottom** of the layer's stack.
    /// Empty for a blank keyframe.
    pub elements: Vec<Element>,
}

impl Default for Frame {
    /// A one-frame blank keyframe at index 0.
    fn default() -> Self {
        Self {
            index: 0,
            duration: 1,
            label: None,
            script: None,
            tween: Tween::None,
            elements: Vec::new(),
        }
    }
}

impl Frame {
    /// One past the last frame of the span (`index + duration`, saturating).
    pub fn end(&self) -> u32 {
        self.index.saturating_add(self.duration)
    }

    /// Whether frame `i` lies in this span.
    pub fn contains(&self, i: u32) -> bool {
        i >= self.index && i < self.end()
    }
}

/// A frame label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct FrameLabel {
    /// The label text (`DOMFrame@name`).
    pub name: String,
    /// `labelType`.
    pub kind: LabelKind,
}

/// The type of a frame label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LabelKind {
    /// A frame label, published as FrameLabel.
    #[default]
    Name,
    /// A comment: never published.
    Comment,
    /// A named anchor (published as a FrameLabel with the anchor flag).
    Anchor,
}

/// The tween that starts on a keyframe.
///
/// Derive it from `tweenType` and the explicit attributes only; `keyMode` is
/// opaque and must not be used for tween semantics.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Tween {
    /// No tween.
    #[default]
    None,
    /// Classic motion tween.
    Motion(MotionTween),
    /// Shape tween (published as keyframes only in milestone 1).
    Shape(ShapeTween),
    /// Any other `tweenType` (`motion object`, `IK pose`), kept verbatim.
    /// The reader adds a warning.
    Other(String),
}

/// Parameters of a classic motion tween.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionTween {
    /// `acceleration` exactly as written, −100..100.
    ///
    /// The sign convention is not settled: the schema says −100 is the IDE's
    /// ease out 100, fla-viewer says negative is ease in (see
    /// `docs/research/xfl-format.md`). One fact is known: in the `tweens`
    /// fixture (Flash 8, converted by JPEXS) the IDE's ease −100, which
    /// starts slowly in the SWF Flash 8 published, is written
    /// `acceleration="100"`. Calibrate the curve against that SWF.
    pub ease: f64,
    /// Custom ease curve (`hasCustomEase`): points in 0..1, a piecewise cubic
    /// Bézier of 3n+1 points. `None` when the simple ease applies.
    pub custom_ease: Option<Vec<[f64; 2]>>,
    /// `motionTweenRotate` (absent is read as [`RotateMode::None`], the
    /// schema default; `keyMode` is never consulted).
    pub rotate: RotateMode,
    /// `motionTweenRotateTimes`: extra whole turns.
    pub rotate_times: u32,
    /// `motionTweenScale`: interpolate scale.
    pub scale: bool,
    /// `motionTweenOrientToPath`.
    pub orient_to_path: bool,
    /// `motionTweenSync`: synchronise graphic symbol frames.
    pub sync: bool,
    /// `motionTweenSnap`: snap to a motion guide.
    pub snap: bool,
}

impl Default for MotionTween {
    /// A new classic tween as the Flash IDE creates it: no ease, no rotation,
    /// scale on.
    fn default() -> Self {
        Self {
            ease: 0.0,
            custom_ease: None,
            rotate: RotateMode::None,
            rotate_times: 0,
            scale: true,
            orient_to_path: false,
            sync: false,
            snap: false,
        }
    }
}

/// `motionTweenRotate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RotateMode {
    /// Turn the short way round.
    Auto,
    /// No rotation beyond what the matrices imply (the schema default).
    #[default]
    None,
    /// Turn clockwise, plus `rotate_times` whole turns.
    Clockwise,
    /// Turn counter-clockwise, plus `rotate_times` whole turns.
    CounterClockwise,
}

/// Parameters of a shape tween.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeTween {
    /// `acceleration`, −100..100.
    pub ease: f64,
    /// `shapeTweenBlend` (`distributive` or `angular`).
    pub blend: String,
}

impl Default for ShapeTween {
    fn default() -> Self {
        Self {
            ease: 0.0,
            blend: "distributive".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(index: u32, duration: u32) -> Frame {
        Frame {
            index,
            duration,
            ..Frame::default()
        }
    }

    fn layer(frames: Vec<Frame>) -> Layer {
        Layer {
            frames,
            ..Layer::default()
        }
    }

    #[test]
    fn frame_at_finds_the_containing_span() {
        let l = layer(vec![span(0, 5), span(5, 1), span(10, 3)]);
        assert_eq!(l.frame_at(0).map(|f| f.index), Some(0));
        assert_eq!(l.frame_at(4).map(|f| f.index), Some(0));
        assert_eq!(l.frame_at(5).map(|f| f.index), Some(5));
        assert_eq!(l.frame_at(6), None, "gap between spans");
        assert_eq!(l.frame_at(12).map(|f| f.index), Some(10));
        assert_eq!(l.frame_at(13), None, "past the end");
        assert_eq!(l.keyframe_index_at(11), Some(2));
    }

    #[test]
    fn frame_at_on_empty_layer_or_before_first_span() {
        assert_eq!(layer(Vec::new()).frame_at(0), None);
        assert_eq!(layer(vec![span(3, 2)]).frame_at(1), None);
    }

    #[test]
    fn frame_count_is_the_longest_layer() {
        let mut t = Timeline::new("Scene 1");
        assert_eq!(t.frame_count(), 0);
        t.layers.push(layer(vec![span(0, 5)]));
        t.layers.push(layer(vec![span(0, 2), span(2, 18)]));
        t.layers.push(layer(Vec::new()));
        assert_eq!(t.frame_count(), 20);
    }

    #[test]
    fn frame_end_saturates() {
        let f = span(u32::MAX - 1, 10);
        assert_eq!(f.end(), u32::MAX);
        assert!(f.contains(u32::MAX - 1));
    }
}
