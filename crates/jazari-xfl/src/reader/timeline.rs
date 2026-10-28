//! Timelines, layers, keyframes, labels, scripts and tweens.
//!
//! Tween semantics come from `tweenType` and the explicit attributes only;
//! `keyMode` is opaque (converted files use values outside the schema's
//! table, `docs/research/xfl-format.md` F08).

use crate::model::{
    Frame, FrameLabel, LabelKind, Layer, LayerKind, MotionTween, Rgba, RotateMode, ShapeTween,
    Timeline, Tween,
};
use crate::{LayerRef, WarningKind};

use super::ctx::Ctx;
use super::element::{read_elements, read_script};
use super::values::excerpt;
use super::xml::Node;

/// Children a `DOMFrame` may have. Those other than `Actionscript`,
/// `elements` and `tweens` are handled as unsupported content.
const FRAME_CHILDREN: [&str; 9] = [
    "Actionscript",
    "elements",
    "tweens",
    "MorphShape",
    "SoundEnvelope",
    "motionObjectXML",
    "betweenFrameMatrixList",
    "IKTree",
    "persistentData",
];

/// Most frames a timeline may span: a SWF frame count is a `u16`.
const MAX_FRAMES: u32 = u16::MAX as u32;

/// Reads a `DOMTimeline`. The caller sets the location's timeline.
pub(crate) fn read_timeline(ctx: &mut Ctx, node: &Node) -> Timeline {
    let name = node.attr("name").unwrap_or_default().to_owned();
    if ctx.bool(node, "cameraLayerEnabled", false) {
        ctx.warn(
            WarningKind::AnimateFeature,
            "the timeline camera (Animate) is ignored",
        );
    }
    if ctx.bool(node, "layerDepthEnabled", false) {
        ctx.warn(
            WarningKind::AnimateFeature,
            "layer depth (Animate) is ignored",
        );
    }
    let mut layers = Vec::new();
    if let Some(container) = node.child("layers") {
        for layer_node in &container.children {
            if layer_node.name != "DOMLayer" {
                ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown <{}> inside <layers> was skipped", layer_node.name),
                );
                continue;
            }
            ctx.loc.layer = Some(LayerRef {
                index: layers.len(),
                name: layer_node.attr("name").unwrap_or_default().to_owned(),
            });
            layers.push(read_layer(ctx, layer_node));
        }
    }
    resolve_parents(ctx, &mut layers);
    ctx.loc.layer = None;
    Timeline { name, layers }
}

/// Checks `parentLayerIndex` links once every layer is known.
fn resolve_parents(ctx: &mut Ctx, layers: &mut [Layer]) {
    for i in 0..layers.len() {
        let Some(p) = layers[i].parent else { continue };
        ctx.loc.layer = Some(LayerRef {
            index: i,
            name: layers[i].name.clone(),
        });
        let problem = if p == i {
            Some("is its own parent layer".to_owned())
        } else if p >= layers.len() {
            Some(format!(
                "has parentLayerIndex {p}, but there are only {} layers",
                layers.len()
            ))
        } else {
            None
        };
        if let Some(problem) = problem {
            ctx.warn(
                WarningKind::BadValue,
                format!("the layer {problem}; it is read as a top-level layer"),
            );
            layers[i].parent = None;
        } else if matches!(layers[p].kind, LayerKind::Normal | LayerKind::Other(_)) {
            // Animate's layer parenting: the stored matrices are already in
            // stage space, so the child is drawn as a top-level layer.
            ctx.warn(
                WarningKind::AnimateFeature,
                "layer parenting (Animate) is ignored; the layer is drawn as a top-level layer",
            );
            layers[i].parent = None;
        }
    }
    break_parent_cycles(ctx, layers);
}

/// Cuts loops in the `parent` chains (folder A in folder B in folder A), so
/// that consumers walking a chain up to its mask or guide always stop. In
/// each loop, the link of the layer with the lowest index is removed.
fn break_parent_cycles(ctx: &mut Ctx, layers: &mut [Layer]) {
    for i in 0..layers.len() {
        // Whether the chain above layer `i` comes back to it. A chain has
        // fewer links than there are layers unless it loops.
        let mut current = layers[i].parent;
        let mut in_loop = false;
        for _ in 0..layers.len() {
            match current {
                Some(p) if p == i => {
                    in_loop = true;
                    break;
                }
                Some(p) => current = layers.get(p).and_then(|l| l.parent),
                None => break,
            }
        }
        if !in_loop {
            continue;
        }
        ctx.loc.layer = Some(LayerRef {
            index: i,
            name: layers[i].name.clone(),
        });
        ctx.warn(
            WarningKind::BadValue,
            "the parentLayerIndex links of this layer form a loop; it is read as a top-level layer",
        );
        layers[i].parent = None;
    }
}

/// Reads a `DOMLayer`.
fn read_layer(ctx: &mut Ctx, node: &Node) -> Layer {
    ctx.check_children(node, &["frames"]);
    let kind = match node.attr("layerType") {
        // "masked" and "guided" are not schema values, but the meaning is
        // clear: an ordinary layer under a mask or guide.
        None | Some("normal" | "masked" | "guided") => LayerKind::Normal,
        Some("guide") => LayerKind::Guide,
        Some("mask") => LayerKind::Mask,
        Some("folder") => LayerKind::Folder,
        Some("camera") => {
            ctx.warn(
                WarningKind::AnimateFeature,
                "a camera layer (Animate) is not drawn",
            );
            LayerKind::Other("camera".to_owned())
        }
        Some(other) => {
            ctx.warn(
                WarningKind::UnknownElement,
                format!(
                    "layerType=\"{}\" is unknown; the layer is kept as is",
                    excerpt(other)
                ),
            );
            LayerKind::Other(other.to_owned())
        }
    };
    match node.attr("animationType") {
        None | Some("none") => {}
        Some(other) => ctx.warn(
            WarningKind::AnimateFeature,
            format!(
                "a layer with animationType=\"{}\" shows its keyframes only",
                excerpt(other)
            ),
        ),
    }
    if ctx.bool(node, "attachedToCamera", false) {
        ctx.warn(
            WarningKind::AnimateFeature,
            "attachment to the camera (Animate) is ignored",
        );
    }
    if ctx.bool(node, "transparent", false) {
        ctx.warn(
            WarningKind::AnimateFeature,
            "layer transparency (Animate) is ignored",
        );
    }

    let mut frames = Vec::new();
    if let Some(container) = node.child("frames") {
        let mut next_index = 0u32;
        for frame_node in &container.children {
            if frame_node.name != "DOMFrame" {
                ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown <{}> inside <frames> was skipped", frame_node.name),
                );
                continue;
            }
            let frame = read_frame(ctx, frame_node, next_index);
            next_index = frame.end();
            if let Some(frame) = cap_span(ctx, frame) {
                frames.push(frame);
            }
        }
    }
    sort_spans(ctx, &mut frames);

    Layer {
        name: node.attr("name").unwrap_or_default().to_owned(),
        kind,
        parent: ctx.opt_u32(node, "parentLayerIndex").map(|p| p as usize),
        visible: ctx.bool(node, "visible", true),
        locked: ctx.bool(node, "locked", false),
        outline: ctx.bool(node, "outline", false),
        color: ctx.color(node, "color", Rgba::BLACK),
        frames,
    }
}

/// Keeps a keyframe span inside the first [`MAX_FRAMES`] frames: a span that
/// starts later is dropped and one that runs past the limit is cut, each with
/// a warning. A damaged `index` or `duration` (say `4000000000`) would
/// otherwise make every consumer walk billions of frames.
fn cap_span(ctx: &mut Ctx, mut frame: Frame) -> Option<Frame> {
    if frame.index >= MAX_FRAMES {
        ctx.loc.frame = Some(frame.index);
        ctx.warn(
            WarningKind::BadValue,
            format!("a keyframe starts after frame {MAX_FRAMES}, the most a SWF can hold; it is left out"),
        );
        ctx.loc.frame = None;
        return None;
    }
    if frame.end() > MAX_FRAMES {
        ctx.loc.frame = Some(frame.index);
        ctx.warn(
            WarningKind::BadValue,
            format!(
                "a keyframe lasts {} frames, past frame {MAX_FRAMES}, the most a SWF can hold; it is cut there",
                frame.duration
            ),
        );
        ctx.loc.frame = None;
        frame.duration = MAX_FRAMES - frame.index;
    }
    Some(frame)
}

/// Sorts keyframe spans by index and removes overlaps (the later span wins).
fn sort_spans(ctx: &mut Ctx, frames: &mut Vec<Frame>) {
    if frames.windows(2).all(|w| w[0].end() <= w[1].index) {
        return;
    }
    frames.sort_by_key(|f| f.index);
    let mut out: Vec<Frame> = Vec::with_capacity(frames.len());
    for f in frames.drain(..) {
        if let Some(prev) = out.last_mut()
            && f.index < prev.end()
        {
            ctx.loc.frame = Some(f.index);
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "the keyframe at frame {} overlaps the one at frame {}; the earlier one is cut short",
                    u64::from(f.index) + 1,
                    u64::from(prev.index) + 1
                ),
            );
            ctx.loc.frame = None;
            if f.index == prev.index {
                out.pop();
            } else {
                prev.duration = f.index - prev.index;
            }
        }
        out.push(f);
    }
    *frames = out;
}

/// Reads a `DOMFrame`. `fallback_index` (the end of the previous span) is
/// used when `index` is missing.
fn read_frame(ctx: &mut Ctx, node: &Node, fallback_index: u32) -> Frame {
    let index = match ctx.opt_u32(node, "index") {
        Some(i) => i,
        None => {
            if !node.has_attr("index") {
                ctx.loc.frame = Some(fallback_index);
                ctx.warn(
                    WarningKind::BadValue,
                    "a keyframe has no index; it is placed after the previous one",
                );
            }
            fallback_index
        }
    };
    ctx.loc.frame = Some(index);
    ctx.check_children(node, &FRAME_CHILDREN);

    let mut duration = ctx.u32(node, "duration", 1);
    if duration == 0 {
        ctx.warn(
            WarningKind::BadValue,
            "a keyframe has duration 0; 1 is used",
        );
        duration = 1;
    }

    let label = read_label(ctx, node);
    let script = read_script(ctx, node);
    let tween = read_tween(ctx, node);
    warn_unsupported_frame_content(ctx, node, &tween);
    let elements = read_elements(ctx, node.child("elements"));
    ctx.loc.frame = None;

    Frame {
        index,
        duration,
        label,
        script,
        tween,
        elements,
    }
}

/// `name` and `labelType` of a keyframe.
fn read_label(ctx: &mut Ctx, node: &Node) -> Option<FrameLabel> {
    let name = node.attr("name").filter(|n| !n.is_empty())?;
    let kind = match node.attr("labelType") {
        None | Some("name") => LabelKind::Name,
        Some("comment") => LabelKind::Comment,
        Some("anchor") => LabelKind::Anchor,
        Some("none") => return None,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "labelType=\"{}\" is unknown; a frame label is used",
                    excerpt(other)
                ),
            );
            LabelKind::Name
        }
    };
    Some(FrameLabel {
        name: name.to_owned(),
        kind,
    })
}

/// The tween that starts on this keyframe.
fn read_tween(ctx: &mut Ctx, node: &Node) -> Tween {
    match node.attr("tweenType") {
        None | Some("none") => Tween::None,
        Some("motion") => Tween::Motion(read_motion_tween(ctx, node)),
        Some("shape") => {
            ctx.warn(
                WarningKind::Unsupported,
                "shape tween: milestone 1 publishes its keyframes only",
            );
            Tween::Shape(ShapeTween {
                ease: ctx.f64(node, "acceleration", 0.0).clamp(-100.0, 100.0),
                blend: node
                    .attr("shapeTweenBlend")
                    .unwrap_or("distributive")
                    .to_owned(),
            })
        }
        Some(other @ ("motion object" | "IK pose")) => {
            ctx.warn(
                WarningKind::AnimateFeature,
                format!("a {other} tween shows its first pose only"),
            );
            Tween::Other(other.to_owned())
        }
        Some(other) => {
            ctx.warn(
                WarningKind::UnknownElement,
                format!(
                    "tweenType=\"{}\" is unknown; the keyframe is not tweened",
                    excerpt(other)
                ),
            );
            Tween::Other(other.to_owned())
        }
    }
}

/// Reads the classic motion tween attributes.
fn read_motion_tween(ctx: &mut Ctx, node: &Node) -> MotionTween {
    let rotate = match node.attr("motionTweenRotate") {
        // Absent is the schema default (none); keyMode is never consulted.
        None | Some("none") => RotateMode::None,
        Some("auto") => RotateMode::Auto,
        Some("clockwise") => RotateMode::Clockwise,
        Some("counter-clockwise") => RotateMode::CounterClockwise,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "motionTweenRotate=\"{}\" is unknown; none is used",
                    excerpt(other)
                ),
            );
            RotateMode::None
        }
    };
    MotionTween {
        ease: ctx.f64(node, "acceleration", 0.0).clamp(-100.0, 100.0),
        custom_ease: read_custom_ease(ctx, node),
        rotate,
        rotate_times: ctx.u32(node, "motionTweenRotateTimes", 0),
        scale: ctx.bool(node, "motionTweenScale", true),
        orient_to_path: ctx.bool(node, "motionTweenOrientToPath", false),
        sync: ctx.bool(node, "motionTweenSync", false),
        snap: ctx.bool(node, "motionTweenSnap", false),
    }
}

/// The custom ease curve of a classic tween (`hasCustomEase`), from
/// `<tweens><CustomEase target><Point x y/>…</CustomEase></tweens>`.
///
/// The model holds one curve. The `all` curve is used when present, then
/// `position`, then the first one; separate property curves are reported.
fn read_custom_ease(ctx: &mut Ctx, node: &Node) -> Option<Vec<[f64; 2]>> {
    let tweens = node.child("tweens");
    if let Some(t) = tweens {
        for child in &t.children {
            match child.name.as_str() {
                "CustomEase" => {}
                "Ease" => ctx.warn(
                    WarningKind::AnimateFeature,
                    "an Animate <Ease> preset is ignored",
                ),
                other => ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown <{other}> inside <tweens> was skipped"),
                ),
            }
        }
    }
    if !ctx.bool(node, "hasCustomEase", false) {
        return None;
    }
    let curves: Vec<&Node> = tweens
        .map(|t| t.children_named("CustomEase").collect())
        .unwrap_or_default();
    let target = |name: &str| {
        curves
            .iter()
            .copied()
            .find(|c| c.attr("target").unwrap_or("all") == name)
    };
    let Some(curve) = target("all")
        .or_else(|| target("position"))
        .or_else(|| curves.first().copied())
    else {
        ctx.warn(
            WarningKind::BadValue,
            "hasCustomEase is set but there is no <CustomEase>; the simple ease is used",
        );
        return None;
    };
    if curves.len() > 1 && !ctx.bool(node, "useSingleEaseCurve", true) {
        ctx.warn(
            WarningKind::Unsupported,
            format!(
                "separate ease curves per property are not supported; the '{}' curve is used for all",
                curve.attr("target").unwrap_or("all")
            ),
        );
    }
    let points: Vec<[f64; 2]> = curve
        .children_named("Point")
        .map(|p| [ctx.f64(p, "x", 0.0), ctx.f64(p, "y", 0.0)])
        .collect();
    if points.len() < 4 || !(points.len() - 1).is_multiple_of(3) {
        ctx.warn(
            WarningKind::BadValue,
            format!(
                "a custom ease has {} points, not 3n+1; it is kept as written",
                points.len()
            ),
        );
    }
    (!points.is_empty()).then_some(points)
}

/// Warns about frame content milestone 1 does not publish.
fn warn_unsupported_frame_content(ctx: &mut Ctx, node: &Node, tween: &Tween) {
    if let Some(sound) = node.attr("soundName").filter(|s| !s.is_empty()) {
        ctx.warn(
            WarningKind::Unsupported,
            format!(
                "the sound \"{}\" on this keyframe is not published",
                excerpt(sound)
            ),
        );
    }
    if node.child("MorphShape").is_some() && !matches!(tween, Tween::Shape(_)) {
        ctx.warn(
            WarningKind::Unsupported,
            "shape tween data on a keyframe without a shape tween is ignored",
        );
    }
    let animate_children = ["betweenFrameMatrixList", "IKTree"];
    if animate_children.iter().any(|c| node.child(c).is_some()) && !matches!(tween, Tween::Other(_))
    {
        ctx.warn(
            WarningKind::AnimateFeature,
            "IK pose data (Animate) is ignored",
        );
    }
    if node.child("motionObjectXML").is_some() && !matches!(tween, Tween::Other(_)) {
        ctx.warn(
            WarningKind::AnimateFeature,
            "motion object data (Animate) is ignored",
        );
    }
    if node.attr("frameZDepth").is_some_and(|v| v.trim() != "0") {
        ctx.warn(
            WarningKind::AnimateFeature,
            "layer depth (frameZDepth, Animate) is ignored",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Element;
    use crate::reader::xml::parse_str;

    fn timeline(xml: &str) -> (Timeline, crate::ImportReport) {
        let mut ctx = Ctx::default();
        let t = read_timeline(&mut ctx, &parse_str(xml).unwrap());
        (t, ctx.report)
    }

    #[test]
    fn layers_frames_labels_and_scripts() {
        let (t, report) = timeline(
            r##"<DOMTimeline name="Scene 1"><layers>
              <DOMLayer name="actions" color="#808080" locked="true">
                <frames>
                  <DOMFrame index="0" duration="4" keyMode="1536" motionTweenRotate="auto"><Actionscript><script>stop();</script></Actionscript><elements/></DOMFrame>
                  <DOMFrame index="4" duration="7" name="loop" labelType="name"><elements/></DOMFrame>
                  <DOMFrame index="11" name="note" labelType="comment"/>
                  <DOMFrame index="14" name="a" labelType="anchor"/>
                </frames>
              </DOMLayer>
              <DOMLayer name="mask" layerType="mask" visible="false"/>
              <DOMLayer name="art" parentLayerIndex="1" outline="true"><frames><DOMFrame index="0"><elements><DOMShape><edges/></DOMShape></elements></DOMFrame></frames></DOMLayer>
              <DOMLayer name="g" layerType="guide"/>
              <DOMLayer name="f" layerType="folder"/>
            </layers></DOMTimeline>"##,
        );
        assert!(report.is_empty(), "{report:?}");
        assert_eq!(t.name, "Scene 1");
        assert_eq!(t.layers.len(), 5);
        let actions = &t.layers[0];
        assert!(actions.locked);
        assert_eq!(actions.frames[0].script.as_deref(), Some("stop();"));
        assert_eq!(actions.frames[0].tween, Tween::None, "keyMode is ignored");
        assert_eq!(
            actions.frames[1].label,
            Some(FrameLabel {
                name: "loop".into(),
                kind: LabelKind::Name
            })
        );
        assert_eq!(
            actions.frames[2].label.as_ref().map(|l| l.kind),
            Some(LabelKind::Comment)
        );
        assert_eq!(
            actions.frames[3].label.as_ref().map(|l| l.kind),
            Some(LabelKind::Anchor)
        );
        assert_eq!(t.frame_count(), 15);
        assert_eq!(t.layers[1].kind, LayerKind::Mask);
        assert!(!t.layers[1].visible);
        assert_eq!(t.layers[2].parent, Some(1));
        assert!(t.layers[2].outline);
        assert!(matches!(
            t.layers[2].frames[0].elements[0],
            Element::Shape(_)
        ));
        assert_eq!(t.layers[3].kind, LayerKind::Guide);
        assert_eq!(t.layers[4].kind, LayerKind::Folder);
    }

    #[test]
    fn classic_tween_attributes() {
        let (t, report) = timeline(
            r#"<DOMTimeline name="T"><layers><DOMLayer name="l"><frames>
              <DOMFrame index="0" duration="23" acceleration="-100" keyMode="7681" motionTweenOrientToPath="false" motionTweenRotate="counter-clockwise" motionTweenRotateTimes="2" motionTweenScale="false" motionTweenSnap="true" motionTweenSync="true" tweenType="motion"/>
              <DOMFrame index="23" tweenType="motion" hasCustomEase="true" useSingleEaseCurve="true">
                <tweens><CustomEase target="all"><Point x="0" y="0"/><Point x="0.2" y="0.6"/><Point x="0.6" y="0.9"/><Point x="1" y="1"/></CustomEase></tweens>
              </DOMFrame>
              <DOMFrame index="24" tweenType="shape" acceleration="30"/>
              <DOMFrame index="25" tweenType="motion object"/>
            </frames></DOMLayer></layers></DOMTimeline>"#,
        );
        let f = &t.layers[0].frames;
        let Tween::Motion(m) = &f[0].tween else {
            panic!()
        };
        assert_eq!(m.ease, -100.0);
        assert_eq!(
            (m.rotate, m.rotate_times),
            (RotateMode::CounterClockwise, 2)
        );
        assert!(!m.scale && m.snap && m.sync && !m.orient_to_path);
        assert_eq!(m.custom_ease, None);
        let Tween::Motion(c) = &f[1].tween else {
            panic!()
        };
        assert_eq!(
            c.rotate,
            RotateMode::None,
            "absent rotate is the schema default"
        );
        assert!(c.scale, "absent scale is on");
        assert_eq!(
            c.custom_ease.as_deref(),
            Some(&[[0.0, 0.0], [0.2, 0.6], [0.6, 0.9], [1.0, 1.0]][..])
        );
        assert_eq!(
            f[2].tween,
            Tween::Shape(ShapeTween {
                ease: 30.0,
                blend: "distributive".into()
            })
        );
        assert_eq!(f[3].tween, Tween::Other("motion object".into()));
        let kinds: Vec<WarningKind> = report.iter().map(|w| w.kind).collect();
        assert_eq!(
            kinds,
            [WarningKind::Unsupported, WarningKind::AnimateFeature]
        );
        assert_eq!(report.warnings[0].location.frame, Some(24));
    }

    #[test]
    fn overlapping_and_unordered_spans_are_repaired() {
        let (t, report) = timeline(
            r#"<DOMTimeline name="T"><layers><DOMLayer name="l"><frames>
              <DOMFrame index="10" duration="2"/>
              <DOMFrame index="0" duration="5"/>
              <DOMFrame index="3" duration="2"/>
              <DOMFrame index="10" duration="1"/>
              <DOMFrame duration="0"/>
            </frames></DOMLayer></layers></DOMTimeline>"#,
        );
        let spans: Vec<(u32, u32)> = t.layers[0]
            .frames
            .iter()
            .map(|f| (f.index, f.duration))
            .collect();
        assert_eq!(spans, [(0, 3), (3, 2), (10, 1), (11, 1)]);
        // Missing index, duration 0, and two overlaps.
        assert_eq!(report.len(), 4, "{report:?}");
    }

    #[test]
    fn spans_past_the_swf_frame_limit_are_cut_or_dropped() {
        let (t, report) = timeline(
            r#"<DOMTimeline name="T"><layers><DOMLayer name="l"><frames>
              <DOMFrame index="0" duration="10"/>
              <DOMFrame index="10" duration="4000000000"/>
              <DOMFrame index="4294967295" duration="5"/>
            </frames></DOMLayer></layers></DOMTimeline>"#,
        );
        let spans: Vec<(u32, u32)> = t.layers[0]
            .frames
            .iter()
            .map(|f| (f.index, f.duration))
            .collect();
        assert_eq!(spans, [(0, 10), (10, MAX_FRAMES - 10)]);
        assert_eq!(t.frame_count(), 65_535);
        assert_eq!(report.len(), 2, "{report:?}");
        assert_eq!(report.warnings[0].location.frame, Some(10));
        assert!(report.iter().all(|w| w.kind == WarningKind::BadValue));
    }

    #[test]
    fn parent_loops_are_cut() {
        // Layer 0 hangs below a loop of folders 1 -> 2 -> 1; layer 3 is a
        // folder inside a mask that is inside it.
        let (t, report) = timeline(
            r#"<DOMTimeline name="T"><layers>
              <DOMLayer name="art" parentLayerIndex="1"/>
              <DOMLayer name="f1" layerType="folder" parentLayerIndex="2"/>
              <DOMLayer name="f2" layerType="folder" parentLayerIndex="1"/>
              <DOMLayer name="f3" layerType="folder" parentLayerIndex="4"/>
              <DOMLayer name="m" layerType="mask" parentLayerIndex="3"/>
            </layers></DOMTimeline>"#,
        );
        let parents: Vec<Option<usize>> = t.layers.iter().map(|l| l.parent).collect();
        assert_eq!(parents, [Some(1), None, Some(1), None, Some(3)]);
        let cut: Vec<usize> = report
            .iter()
            .map(|w| w.location.layer.as_ref().map_or(99, |l| l.index))
            .collect();
        assert_eq!(cut, [1, 3], "{report:?}");
        assert!(report.iter().all(|w| w.kind == WarningKind::BadValue));
    }

    #[test]
    fn bad_parents_and_layer_parenting() {
        let (t, report) = timeline(
            r#"<DOMTimeline name="T"><layers>
              <DOMLayer name="a" parentLayerIndex="0"/>
              <DOMLayer name="b" parentLayerIndex="9"/>
              <DOMLayer name="c" parentLayerIndex="0"/>
              <DOMLayer name="cam" layerType="camera"/>
            </layers></DOMTimeline>"#,
        );
        assert!(t.layers.iter().all(|l| l.parent.is_none()));
        assert_eq!(t.layers[3].kind, LayerKind::Other("camera".into()));
        let kinds: Vec<WarningKind> = report.iter().map(|w| w.kind).collect();
        assert_eq!(
            kinds,
            [
                WarningKind::AnimateFeature, // camera layer
                WarningKind::BadValue,       // a is its own parent
                WarningKind::BadValue,       // b points past the end
                WarningKind::AnimateFeature, // c is parented to a normal layer
            ]
        );
        assert_eq!(
            report.warnings[2].location.layer.as_ref().map(|l| l.index),
            Some(1)
        );
    }
}
