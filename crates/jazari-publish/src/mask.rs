//! Mask layers: the mask content placed with `clip_depth` over the layers it
//! masks.
//!
//! # How Flash 8 publishes masks (`tests/fixtures/MANIFEST.md`, masks)
//!
//! - The mask is placed **directly** at a depth below the masked layers,
//!   with `clipDepth` = the highest depth the masked layers use: a raw mask
//!   is a DefineShape, a movie clip mask is the clip's own DefineSprite placed
//!   with its instance name (so ActionScript reaches it). There is no wrapper
//!   sprite.
//! - **Only the backmost element** of a mask keyframe is published; the
//!   others are dropped silently (their instances are `undefined` at run
//!   time). Raw shapes are always the backmost (see [`crate::group`]).
//! - A motion-tweened mask is one PlaceObject2 Modify per frame at the mask
//!   depth, like any tween; masks inside a movie clip follow the same
//!   pattern inside its DefineSprite.
//!
//! # What this module provides
//!
//! - [`plan_layers`]: the published layers bottom first, each mask directly
//!   below the layers it masks (so the clip range covers exactly them). A
//!   mask with no published, non-empty masked layer is left out, since it
//!   would only clip nothing.
//! - [`mask_items`]: the expansion of a mask keyframe reduced to what Flash
//!   publishes, its backmost element (with a warning naming the elements
//!   left out). When that element would take more than one depth (a graphic
//!   symbol, or a group holding instances) it is wrapped in a generated
//!   one-frame sprite, so a single placement can carry the clip depth. This
//!   is the Flash-faithful seam: the timeline places it like ordinary
//!   content (matrix, instance name, motion tween) with the band's clip
//!   depth.
//!
//! Generated wrappers follow the sprite rules: `stop()` on frame 1 in the
//! stage build.

use std::collections::HashMap;
use std::rc::Rc;

use jazari_as2::Origin;
use jazari_xfl::model::{Element, Frame, Layer, LayerKind, Matrix, Timeline};
use swf::CharacterId;

use crate::BuildMode;
use crate::context::{CharKind, Ctx, MAX_NESTING, Scope};
use crate::display::{DefSink, DisplayList, FrameOps, encode_frame};
use crate::group;
use crate::script;
use crate::tags::{self, RawTag};
use crate::timeline::{self, Expanded, Item, Target, Xform, is_published_layer};

/// Per-publish state of the mask module.
#[derive(Debug, Default)]
pub struct MaskState {
    /// Mask keyframes reduced to their backmost element, by the address of
    /// their `Frame`.
    items: HashMap<usize, Rc<Expanded>>,
    /// Mask keyframes being built (recursion guard).
    building: Vec<usize>,
    /// The generated timelines behind wrappers. The timeline caches are keyed
    /// by addresses, so these live as long as the publish.
    keep: Vec<Rc<Timeline>>,
}

/// The role of a published layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerRole {
    /// Ordinary content.
    Content,
    /// A mask: its character clips the layers listed (indexes into
    /// `Timeline::layers`).
    Mask {
        /// The masked layers.
        masked: Vec<usize>,
    },
}

/// One published layer, in depth order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerPlan {
    /// Index into `Timeline::layers`.
    pub layer: usize,
    /// Its role.
    pub role: LayerRole,
}

/// The published layers of a timeline, bottom first. Each mask comes
/// directly before (below) the layers it masks, which follow it bottom
/// first.
pub fn plan_layers(ctx: &mut Ctx<'_>, tl: &Timeline, scope: &Scope) -> Vec<LayerPlan> {
    let n = tl.layers.len();
    let published: Vec<bool> = (0..n)
        .map(|i| is_published_layer(ctx, tl, i, scope))
        .collect();
    let groups = mask_groups(tl, &published);
    let mut done = vec![false; n];
    let mut out = Vec::with_capacity(n);
    for i in (0..n).rev() {
        if !published[i] || done[i] {
            continue;
        }
        let Some(layer) = tl.layers.get(i) else {
            continue;
        };
        if layer.kind == LayerKind::Mask {
            // Reached only when no published masked layer brought it in.
            continue;
        }
        let Some(m) = masked_by(tl, &published, i) else {
            out.push(LayerPlan {
                layer: i,
                role: LayerRole::Content,
            });
            continue;
        };
        let masked = groups.get(&m).cloned().unwrap_or_default();
        if !masked.iter().any(|&j| has_content(tl, j)) {
            // Nothing to clip: the masked layers are empty, and a mask
            // without clipped content is never drawn.
            for &j in &masked {
                done[j] = true;
            }
            continue;
        }
        out.push(LayerPlan {
            layer: m,
            role: LayerRole::Mask {
                masked: masked.clone(),
            },
        });
        for &j in &masked {
            done[j] = true;
            out.push(LayerPlan {
                layer: j,
                role: LayerRole::Content,
            });
        }
    }
    out
}

/// The mask layer masking layer `i`, when `i` is a published normal layer
/// whose `parentLayerIndex` names a published mask layer.
fn masked_by(tl: &Timeline, published: &[bool], i: usize) -> Option<usize> {
    let layer = tl.layers.get(i)?;
    if layer.kind != LayerKind::Normal {
        return None;
    }
    let m = layer.parent?;
    let ok = m != i
        && tl.layers.get(m)?.kind == LayerKind::Mask
        && published.get(m).copied().unwrap_or(false);
    ok.then_some(m)
}

/// For each mask layer, its published masked layers, bottom first.
fn mask_groups(tl: &Timeline, published: &[bool]) -> HashMap<usize, Vec<usize>> {
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in (0..tl.layers.len()).rev() {
        if !published.get(i).copied().unwrap_or(false) {
            continue;
        }
        if let Some(m) = masked_by(tl, published, i) {
            groups.entry(m).or_default().push(i);
        }
    }
    groups
}

/// Whether a layer has anything that could be drawn.
fn has_content(tl: &Timeline, i: usize) -> bool {
    tl.layers.get(i).is_some_and(|l| {
        l.frames.iter().any(|f| {
            f.elements
                .iter()
                .any(|e| !matches!(e, Element::Unsupported { .. }))
        })
    })
}

/// The expansion of mask keyframe `keyframe` of layer `layer`, reduced to
/// what Flash 8 publishes: one item, the backmost element (cached). See the
/// module docs. The elements left out are reported once.
pub fn mask_items(
    ctx: &mut Ctx<'_>,
    tl: &Timeline,
    layer: usize,
    keyframe: usize,
    scope: &Scope,
) -> Rc<Expanded> {
    let Some(l) = tl.layers.get(layer) else {
        return Rc::default();
    };
    let Some(frame) = l.frames.get(keyframe) else {
        return Rc::default();
    };
    let key = std::ptr::from_ref(frame) as usize;
    if let Some(e) = ctx.mask.items.get(&key) {
        return e.clone();
    }
    let origin = scope.origin(Some(&l.name), Some(frame.index));
    if !enter(ctx, key, &origin) {
        return Rc::default();
    }
    let ordered = group::drawing_order(&frame.elements);
    let exp = match ordered.split_first() {
        None => Expanded::default(),
        Some((&first, rest)) => {
            report_dropped(ctx, rest, &origin);
            let exp = timeline::expand_elements(
                ctx,
                std::slice::from_ref(first),
                scope,
                &l.name,
                frame.index,
            );
            if takes_one_depth(&exp) {
                exp
            } else {
                let frames = vec![Frame {
                    index: frame.index,
                    duration: 1,
                    elements: vec![first.clone()],
                    ..Frame::default()
                }];
                let wrapper = wrapper_sprite(ctx, frames, frame.index, 1, scope, &l.name);
                Expanded {
                    items: wrapper
                        .map(|char| Item::Text {
                            char,
                            matrix: Matrix::IDENTITY,
                            name: None,
                        })
                        .into_iter()
                        .collect(),
                }
            }
        }
    };
    leave(ctx, key);
    let exp = Rc::new(exp);
    ctx.mask.items.insert(key, exp.clone());
    exp
}

/// Defines a generated sprite showing frames `first..first + count` of a
/// one-layer timeline holding `frames` (keyframes at their real indexes, so
/// diagnostics point at the document). `stop()` goes on frame 1 in the
/// stage build and, for several frames, on the last frame in the release
/// build. Used for a mask element that would take more than one depth.
fn wrapper_sprite(
    ctx: &mut Ctx<'_>,
    frames: Vec<Frame>,
    first: u32,
    count: u32,
    scope: &Scope,
    layer_name: &str,
) -> Option<CharacterId> {
    let origin = scope.origin(Some(layer_name), Some(first));
    let count = count.clamp(1, timeline::MAX_FRAMES);
    let tl = Rc::new(Timeline {
        name: scope.timeline.clone(),
        layers: vec![Layer {
            name: layer_name.to_owned(),
            frames,
            ..Layer::default()
        }],
    });
    ctx.mask.keep.push(tl.clone());
    let mut dl = DisplayList::default();
    let mut ops: Vec<FrameOps> = Vec::with_capacity(count as usize);
    for f in first..first.saturating_add(count) {
        let mut items = Vec::new();
        timeline::plan_frame(
            ctx,
            &tl,
            scope,
            f,
            1,
            &Xform::IDENTITY,
            &[],
            &mut Vec::new(),
            &mut items,
        );
        let (removes, places) = dl.diff(items);
        ops.push(FrameOps {
            removes,
            places,
            ..FrameOps::default()
        });
    }
    if ctx.mode == BuildMode::Stage {
        if let Some(op) = ops.first_mut() {
            op.actions = vec![script::STOP.to_vec()];
        }
    } else if ops.len() > 1
        && let Some(op) = ops.last_mut()
    {
        op.actions = vec![script::STOP.to_vec()];
    }
    let id = ctx.reserve_id(&origin)?;
    let mut deps = Vec::new();
    let mut children: Vec<RawTag> = Vec::new();
    for op in &ops {
        encode_frame(
            ctx,
            op,
            &mut DefSink::Sprite(&mut deps),
            &mut children,
            &origin,
        );
        children.push(RawTag::show_frame());
    }
    deps.sort_unstable();
    deps.dedup();
    let num_frames = u16::try_from(ops.len()).unwrap_or(u16::MAX);
    ctx.set_definition(
        id,
        CharKind::Sprite,
        vec![tags::sprite(id, num_frames, &children)],
        deps,
    );
    Some(id)
}

/// Marks a mask keyframe as being built; `false` (reported) when it is
/// already being built (a symbol that masks with itself) or masks nest too
/// deeply.
fn enter(ctx: &mut Ctx<'_>, key: usize, origin: &Origin) -> bool {
    if ctx.mask.building.contains(&key) || ctx.mask.building.len() >= MAX_NESTING {
        ctx.error(
            origin.clone(),
            "this mask contains itself (or masks nest too deeply); it is left out",
        );
        return false;
    }
    ctx.mask.building.push(key);
    true
}

/// Ends [`enter`].
fn leave(ctx: &mut Ctx<'_>, key: usize) {
    if let Some(pos) = ctx.mask.building.iter().rposition(|k| *k == key) {
        ctx.mask.building.remove(pos);
    }
}

/// Whether an expansion is one item that takes one depth (anything but a
/// flattened graphic), so one placement can carry the clip depth.
fn takes_one_depth(exp: &Expanded) -> bool {
    match exp.items.as_slice() {
        [] => true,
        [Item::Shape { .. } | Item::Text { .. }] => true,
        [Item::Instance(ii)] => !matches!(ii.target, Target::Graphic),
        _ => false,
    }
}

/// Reports the elements of a mask keyframe that Flash 8 leaves out.
fn report_dropped(ctx: &mut Ctx<'_>, dropped: &[&Element], origin: &Origin) {
    let names: Vec<String> = dropped
        .iter()
        .filter(|e| !matches!(e, Element::Unsupported { .. }))
        .map(|e| describe(e))
        .collect();
    if names.is_empty() {
        return;
    }
    ctx.warn(
        origin.clone(),
        format!(
            "only the backmost element of a mask layer is published (as Flash 8 does); left out: {}",
            names.join(", ")
        ),
    );
}

/// A short description of an element for diagnostics.
fn describe(el: &Element) -> String {
    match el {
        Element::Instance(i) => match i.name.as_deref().filter(|n| !n.is_empty()) {
            Some(n) => format!("'{n}'"),
            None => format!("an instance of '{}'", i.library_item),
        },
        Element::Shape(s) if s.is_drawing_object => "a drawing object".to_owned(),
        Element::Shape(_) => "a shape".to_owned(),
        Element::Group(_) => "a group".to_owned(),
        Element::Bitmap(b) => format!("bitmap '{}'", b.library_item),
        Element::Text(_) => "a text".to_owned(),
        Element::Unsupported { tag } => tag.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jazari_xfl::model::{GraphicPlayback, Shape, Symbol, SymbolInstance, SymbolKind};

    fn layer(name: &str, kind: LayerKind, parent: Option<usize>, elements: Vec<Element>) -> Layer {
        Layer {
            name: name.into(),
            kind,
            parent,
            frames: vec![Frame {
                elements,
                ..Frame::default()
            }],
            ..Layer::default()
        }
    }

    fn rect() -> Element {
        use jazari_xfl::model::{Edge, FillStyle, PathCmd, Pt, Rgba};
        Element::Shape(Shape {
            fills: vec![FillStyle::Solid(Rgba::new(0, 0, 0, 255))],
            edges: vec![Edge {
                fill1: 1,
                path: vec![
                    PathCmd::MoveTo(Pt::new(0.0, 0.0)),
                    PathCmd::LineTo(Pt::new(200.0, 0.0)),
                    PathCmd::LineTo(Pt::new(200.0, 200.0)),
                    PathCmd::LineTo(Pt::new(0.0, 0.0)),
                ],
                ..Edge::default()
            }],
            ..Shape::default()
        })
    }

    fn clip_at(name: &str, x: f64) -> Element {
        Element::Instance(SymbolInstance {
            library_item: "Clip".into(),
            kind: SymbolKind::MovieClip,
            name: Some(name.into()),
            matrix: Matrix::translate(x, 10.0),
            ..SymbolInstance::default()
        })
    }

    fn clip(name: &str) -> Element {
        clip_at(name, 10.0)
    }

    fn doc_with_clip() -> jazari_xfl::Document {
        let mut doc = jazari_xfl::Document::default();
        doc.library.symbols.insert(
            "Clip".into(),
            Symbol {
                name: "Clip".into(),
                kind: SymbolKind::MovieClip,
                timeline: Timeline {
                    name: "Clip".into(),
                    layers: vec![layer("l", LayerKind::Normal, None, vec![rect()])],
                },
                ..Symbol::default()
            },
        );
        doc
    }

    fn roles(plan: &[LayerPlan]) -> Vec<(usize, bool)> {
        plan.iter()
            .map(|p| (p.layer, matches!(p.role, LayerRole::Mask { .. })))
            .collect()
    }

    #[test]
    fn masks_go_directly_below_their_masked_layers() {
        // Top to bottom: top, mask (masks 2 and 3), m1, m2, normal, lone mask.
        let tl = Timeline {
            name: "Scene 1".into(),
            layers: vec![
                layer("top", LayerKind::Normal, None, vec![rect()]),
                layer("mask", LayerKind::Mask, None, vec![rect()]),
                layer("m1", LayerKind::Normal, Some(1), vec![rect()]),
                layer("m2", LayerKind::Normal, Some(1), vec![rect()]),
                layer("bottom", LayerKind::Normal, None, vec![rect()]),
                layer("lone", LayerKind::Mask, None, vec![rect()]),
            ],
        };
        let doc = jazari_xfl::Document::default();
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let plan = plan_layers(&mut ctx, &tl, &Scope::new("Scene 1"));
        assert_eq!(
            roles(&plan),
            [(4, false), (1, true), (3, false), (2, false), (0, false)]
        );
        assert_eq!(plan[1].role, LayerRole::Mask { masked: vec![3, 2] });
    }

    #[test]
    fn hidden_or_empty_partners_turn_masks_off() {
        let mut hidden_mask = layer("mask", LayerKind::Mask, None, vec![rect()]);
        hidden_mask.visible = false;
        let tl = Timeline {
            name: "s".into(),
            layers: vec![
                hidden_mask,
                layer("m", LayerKind::Normal, Some(0), vec![rect()]),
                layer("mask2", LayerKind::Mask, None, vec![rect()]),
                layer("empty", LayerKind::Normal, Some(2), vec![]),
            ],
        };
        let mut doc = jazari_xfl::Document::default();
        doc.publish.publish_hidden_layers = false;
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let plan = plan_layers(&mut ctx, &tl, &Scope::new("s"));
        // The hidden mask's layer is shown unmasked; the empty pair is gone.
        assert_eq!(roles(&plan), [(1, false)]);
    }

    #[test]
    fn only_the_backmost_element_is_published_and_the_rest_reported() {
        let doc = doc_with_clip();
        let tl = Timeline {
            name: "s".into(),
            layers: vec![
                // XFL order: instance first, raw shape last (JPEXS).
                layer("mask", LayerKind::Mask, None, vec![clip("disc_mc"), rect()]),
                layer("art", LayerKind::Normal, Some(0), vec![rect()]),
            ],
        };
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let scope = Scope::new("s");
        let exp = mask_items(&mut ctx, &tl, 0, 0, &scope);
        assert_eq!(exp.items.len(), 1);
        let Item::Shape { char, matrix } = &exp.items[0] else {
            panic!("the raw shape is the backmost: {:?}", exp.items);
        };
        assert_eq!(*matrix, Matrix::IDENTITY);
        assert_eq!(ctx.kind_of(*char), Some(CharKind::Shape));
        let diags = ctx.take_diagnostics();
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(
            diags[0].message.contains("'disc_mc'"),
            "{}",
            diags[0].message
        );
    }

    #[test]
    fn instance_masks_keep_their_instance_for_the_timeline() {
        let doc = doc_with_clip();
        let tl = Timeline {
            name: "s".into(),
            layers: vec![
                layer(
                    "mask",
                    LayerKind::Mask,
                    None,
                    vec![clip("a_mc"), clip("b_mc")],
                ),
                layer("art", LayerKind::Normal, Some(0), vec![rect()]),
            ],
        };
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let scope = Scope::new("s");
        let exp = mask_items(&mut ctx, &tl, 0, 0, &scope);
        let [Item::Instance(ii)] = exp.items.as_slice() else {
            panic!("{:?}", exp.items);
        };
        assert_eq!(ii.inst.name.as_deref(), Some("a_mc"));
        let diags = ctx.take_diagnostics();
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(diags[0].message.contains("'b_mc'"), "{diags:?}");
    }

    #[test]
    fn a_graphic_that_masks_with_itself_is_cut() {
        // Graphic "G" has a mask layer whose backmost element is G itself.
        let mut doc = jazari_xfl::Document::default();
        let g = Element::Instance(SymbolInstance {
            library_item: "G".into(),
            kind: SymbolKind::Graphic,
            graphic: Some(GraphicPlayback::default()),
            ..SymbolInstance::default()
        });
        doc.library.symbols.insert(
            "G".into(),
            Symbol {
                name: "G".into(),
                kind: SymbolKind::Graphic,
                timeline: Timeline {
                    name: "G".into(),
                    layers: vec![
                        layer("mask", LayerKind::Mask, None, vec![g.clone()]),
                        layer("art", LayerKind::Normal, Some(0), vec![rect()]),
                    ],
                },
                ..Symbol::default()
            },
        );
        let tl = Timeline {
            name: "s".into(),
            layers: vec![
                layer("mask", LayerKind::Mask, None, vec![g]),
                layer("art", LayerKind::Normal, Some(0), vec![rect()]),
            ],
        };
        let fonts = jazari_text::FontLibrary::embedded_only();
        let as2 = jazari_as2::Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let scope = Scope::new("s");
        let _ = mask_items(&mut ctx, &tl, 0, 0, &scope);
        let diags = ctx.take_diagnostics();
        assert!(
            diags
                .iter()
                .any(|d| d.severity == jazari_as2::Severity::Error),
            "{diags:?}"
        );
    }
}
