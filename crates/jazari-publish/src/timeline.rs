//! Timelines: layers, depth bands, keyframe expansion and the display list
//! of every frame.
//!
//! # Depth bands
//!
//! Each published layer of a timeline gets a band of consecutive depths,
//! bottom layer lowest, in the order [`mask::plan_layers`] returns. A band is
//! as wide as the largest keyframe of its layer, where a shape run, text,
//! movie clip or button takes one depth and a flattened graphic takes the
//! depth need of its own timeline (a sub-band inside the parent's band).
//! The root starts at depth 1 and everything stays below 16384.
//!
//! # Keyframe expansion
//!
//! Every keyframe is expanded once ([`expand`]): groups are flattened,
//! consecutive shapes with the same matrix become one shape character,
//! bitmap instances become shapes, texts become characters, and symbol
//! instances are resolved (sprites and buttons built, handlers compiled).
//! Each resulting item is one "slot"; a slot of one keyframe continues the
//! same slot of the previous keyframe when it holds the same symbol (see
//! [`crate::display`]).
//!
//! # Frames
//!
//! [`plan_frame`] lists what is visible at a frame, recursing into graphic
//! symbols with [`graphic::inner_frame`] and interpolating motion tweens
//! with [`tween::tweened_state`]. [`build_frames`] diffs consecutive frames
//! into [`FrameOps`] and adds frame scripts and labels.
//!
//! Caches are keyed by the addresses of `Frame` and `Timeline` values, which
//! all belong to the document borrowed for the whole publish.

use std::hash::{Hash, Hasher};
use std::rc::Rc;

use jazari_as2::{ClipHandler, Origin};
use jazari_xfl::model::{
    ColorEffect, Element, LabelKind, LayerKind, Matrix, MotionTween, Shape, SymbolInstance,
    SymbolKind, TextKind, Timeline, Tween,
};
use swf::CharacterId;

use crate::context::{CharKind, Ctx, MAX_NESTING, Scope};
use crate::display::{
    DefSink, DisplayItem, DisplayList, FrameOps, IdentKind, IdentPart, MAX_TIMELINE_DEPTH,
    encode_frame,
};
use crate::mask::{self, LayerRole};
use crate::tags::{self, RawTag};
use crate::{bitmap, filter, graphic, group, script, shape, symbol, text, tween, units};

/// The published layers of a timeline and their depth bands.
#[derive(Debug)]
pub struct TimelinePlan {
    /// Bands, bottom first.
    pub bands: Vec<Band>,
    /// Total number of depths the timeline needs.
    pub need: u32,
}

/// The depth band of one published layer.
#[derive(Debug)]
pub struct Band {
    /// Index into `Timeline::layers`.
    pub layer: usize,
    /// First depth of the band, relative to the timeline's base depth.
    pub offset: u32,
    /// Number of depths.
    pub size: u32,
    /// The layer's role.
    pub role: LayerRole,
    /// For a mask: the top depth of the masked bands, relative to the base.
    pub mask_clip: Option<u32>,
}

/// A keyframe after expansion.
#[derive(Debug, Default)]
pub struct Expanded {
    /// Slots, bottom first.
    pub items: Vec<Item>,
}

/// One slot of an expanded keyframe.
#[derive(Debug)]
pub enum Item {
    /// A run of shapes as one character.
    Shape {
        /// The shape character.
        char: CharacterId,
        /// The run's matrix.
        matrix: Matrix,
    },
    /// A text character.
    Text {
        /// The text character.
        char: CharacterId,
        /// Placement matrix.
        matrix: Matrix,
        /// Instance name (dynamic and input text).
        name: Option<String>,
    },
    /// A symbol instance.
    Instance(Box<InstanceItem>),
}

/// A resolved symbol instance.
#[derive(Debug)]
pub struct InstanceItem {
    /// The instance (matrix already composed with any groups around it).
    pub inst: SymbolInstance,
    /// What it becomes.
    pub target: Target,
    /// Its filters, converted.
    pub filters: Vec<swf::Filter>,
    /// Hash of its script, for identity.
    pub script_hash: Option<u64>,
    /// Where it comes from, for diagnostics.
    pub origin: Origin,
}

/// What an instance becomes.
#[derive(Debug)]
pub enum Target {
    /// A DefineSprite, with clip actions.
    Sprite {
        /// The sprite.
        char: CharacterId,
        /// Compiled `onClipEvent`/`on` handlers.
        actions: Option<Rc<Vec<ClipHandler>>>,
    },
    /// A DefineButton2 (a per-instance copy when it has handlers).
    Button {
        /// The button.
        char: CharacterId,
    },
    /// A graphic, flattened into the parent at planning time.
    Graphic,
}

/// A parent transform for flattened content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xform {
    /// Matrix (pixels).
    pub matrix: Matrix,
    /// Colour effect.
    pub color: ColorEffect,
}

impl Xform {
    /// No transform.
    pub const IDENTITY: Xform = Xform {
        matrix: Matrix::IDENTITY,
        color: ColorEffect::IDENTITY,
    };
}

/// Whether layer `i` is published at all: guide and folder layers never are,
/// unknown layer types are reported and left out, and hidden layers follow
/// the document's "publish hidden layers" setting. Mask layers pass; what
/// happens to them is [`mask::plan_layers`]'s decision.
pub fn is_published_layer(ctx: &mut Ctx<'_>, tl: &Timeline, i: usize, scope: &Scope) -> bool {
    let Some(layer) = tl.layers.get(i) else {
        return false;
    };
    match &layer.kind {
        LayerKind::Guide | LayerKind::Folder => return false,
        LayerKind::Other(kind) => {
            if !layer.frames.is_empty() {
                ctx.warn(
                    scope.origin(Some(&layer.name), None),
                    format!("layers of type '{kind}' are not published"),
                );
            }
            return false;
        }
        LayerKind::Normal | LayerKind::Mask => {}
    }
    layer.visible || ctx.doc.publish.publish_hidden_layers
}

/// The plan of a timeline (cached).
pub fn timeline_plan(ctx: &mut Ctx<'_>, tl: &Timeline, scope: &Scope) -> Rc<TimelinePlan> {
    let key = std::ptr::from_ref(tl) as usize;
    if let Some(p) = ctx.plans.get(&key) {
        return p.clone();
    }
    let layers = mask::plan_layers(ctx, tl, scope);
    let mut bands = Vec::with_capacity(layers.len());
    let mut offset: u32 = 0;
    for lp in layers {
        let size = match lp.role {
            LayerRole::Mask { .. } => 1,
            LayerRole::Content => layer_need(ctx, tl, lp.layer, scope),
        };
        bands.push(Band {
            layer: lp.layer,
            offset,
            size,
            role: lp.role,
            mask_clip: None,
        });
        offset = offset.saturating_add(size);
    }
    let tops: Vec<Option<u32>> = bands
        .iter()
        .map(|b| match &b.role {
            LayerRole::Mask { masked } => bands
                .iter()
                .filter(|o| masked.contains(&o.layer) && o.size > 0)
                .map(|o| (o.offset + o.size).saturating_sub(1))
                .max(),
            LayerRole::Content => None,
        })
        .collect();
    for (b, top) in bands.iter_mut().zip(tops) {
        b.mask_clip = top;
    }
    let plan = Rc::new(TimelinePlan {
        bands,
        need: offset,
    });
    ctx.plans.insert(key, plan.clone());
    plan
}

/// The widest keyframe of a layer, in depths.
fn layer_need(ctx: &mut Ctx<'_>, tl: &Timeline, layer: usize, scope: &Scope) -> u32 {
    let count = tl.layers.get(layer).map_or(0, |l| l.frames.len());
    let mut most = 0u32;
    for ki in 0..count {
        let exp = expand(ctx, tl, layer, ki, scope);
        let mut sum = 0u32;
        for item in &exp.items {
            sum = sum.saturating_add(item_need(ctx, item));
        }
        most = most.max(sum);
    }
    most
}

/// Depths one slot takes.
fn item_need(ctx: &mut Ctx<'_>, item: &Item) -> u32 {
    match item {
        Item::Shape { .. } | Item::Text { .. } => 1,
        Item::Instance(ii) => match ii.target {
            Target::Graphic => symbol_need(ctx, &ii.inst.library_item, &ii.origin),
            _ => 1,
        },
    }
}

/// Depths a graphic symbol takes when flattened (cached; 0 for recursive or
/// missing symbols).
fn symbol_need(ctx: &mut Ctx<'_>, name: &str, origin: &Origin) -> u32 {
    if let Some(n) = ctx.needs.get(name) {
        return *n;
    }
    if ctx.need_stack.iter().any(|s| s == name) || ctx.need_stack.len() >= MAX_NESTING {
        ctx.error(
            origin.clone(),
            format!(
                "graphic symbol '{name}' contains itself (or nests too deeply); it is left out"
            ),
        );
        return 0;
    }
    let doc = ctx.doc;
    let Some(sym) = doc.library.symbols.get(name) else {
        return 0;
    };
    ctx.need_stack.push(name.to_owned());
    let scope = Scope {
        timeline: name.to_owned(),
        flattened: true,
        scene: None,
    };
    let need = timeline_plan(ctx, &sym.timeline, &scope).need;
    ctx.need_stack.pop();
    ctx.needs.insert(name.to_owned(), need);
    need
}

/// The expansion of keyframe `ki` of layer `layer` (cached).
pub fn expand(
    ctx: &mut Ctx<'_>,
    tl: &Timeline,
    layer: usize,
    ki: usize,
    scope: &Scope,
) -> Rc<Expanded> {
    let Some(l) = tl.layers.get(layer) else {
        return Rc::new(Expanded::default());
    };
    let Some(frame) = l.frames.get(ki) else {
        return Rc::new(Expanded::default());
    };
    let key = std::ptr::from_ref(frame) as usize;
    if let Some(e) = ctx.expanded.get(&key) {
        return e.clone();
    }
    let exp = Rc::new(expand_elements(
        ctx,
        &frame.elements,
        scope,
        &l.name,
        frame.index,
    ));
    ctx.expanded.insert(key, exp.clone());
    exp
}

/// Expands a list of elements (not cached; see [`expand`]).
pub fn expand_elements(
    ctx: &mut Ctx<'_>,
    elements: &[Element],
    scope: &Scope,
    layer_name: &str,
    frame: u32,
) -> Expanded {
    let kf_origin = scope.origin(Some(layer_name), Some(frame));
    let flat = group::flatten(ctx, elements, &kf_origin);
    let mut items = Vec::new();
    let mut run: Vec<Shape> = Vec::new();
    for (i, el) in flat.into_iter().enumerate() {
        let origin = scope.element_origin(layer_name, frame, describe(&el, i));
        match el {
            Element::Shape(s) => push_shape(ctx, &mut run, &mut items, s, &kf_origin),
            Element::Bitmap(b) => {
                if let Some(s) = bitmap::instance_shape(ctx, &b, &origin) {
                    push_shape(ctx, &mut run, &mut items, s, &kf_origin);
                }
            }
            Element::Instance(inst) => {
                flush_run(ctx, &mut run, &mut items, &kf_origin);
                if let Some(item) = resolve_instance(ctx, inst, origin, scope.scene) {
                    items.push(Item::Instance(Box::new(item)));
                }
            }
            Element::Text(t) => {
                flush_run(ctx, &mut run, &mut items, &kf_origin);
                if let Some(tc) = text::define_text(ctx, &t, &origin) {
                    let name = if t.kind == TextKind::Static {
                        None
                    } else {
                        t.name.clone().filter(|n| !n.is_empty())
                    };
                    items.push(Item::Text {
                        char: tc.id,
                        matrix: tc.matrix,
                        name,
                    });
                }
            }
            Element::Unsupported { tag } => {
                ctx.warn(origin, format!("{tag} is not published"));
            }
            Element::Group(_) => {} // already flattened
        }
    }
    flush_run(ctx, &mut run, &mut items, &kf_origin);
    Expanded { items }
}

fn push_shape(
    ctx: &mut Ctx<'_>,
    run: &mut Vec<Shape>,
    items: &mut Vec<Item>,
    s: Shape,
    origin: &Origin,
) {
    if run.first().is_some_and(|first| first.matrix != s.matrix) {
        flush_run(ctx, run, items, origin);
    }
    run.push(s);
}

fn flush_run(ctx: &mut Ctx<'_>, run: &mut Vec<Shape>, items: &mut Vec<Item>, origin: &Origin) {
    if run.is_empty() {
        return;
    }
    let refs: Vec<&Shape> = run.iter().collect();
    if let Some(char) = shape::define_shapes(ctx, &refs, origin) {
        items.push(Item::Shape {
            char,
            matrix: run[0].matrix,
        });
    }
    run.clear();
}

/// A short description of an element for diagnostics.
fn describe(el: &Element, index: usize) -> String {
    match el {
        Element::Instance(i) => {
            i.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| {
                format!("element {}: instance of '{}'", index + 1, i.library_item)
            })
        }
        Element::Text(t) => {
            let s: String = t
                .runs
                .iter()
                .flat_map(|r| r.text.chars())
                .take(24)
                .collect();
            match t.name.as_deref().filter(|n| !n.is_empty()) {
                Some(n) => format!("text '{n}'"),
                None => format!("element {}: text \"{}\"", index + 1, s.replace('\r', " ")),
            }
        }
        Element::Bitmap(b) => format!("element {}: bitmap '{}'", index + 1, b.library_item),
        Element::Shape(_) => format!("shape {}", index + 1),
        Element::Group(_) => format!("group {}", index + 1),
        Element::Unsupported { tag } => format!("{tag} {}", index + 1),
    }
}

/// Resolves a symbol instance: builds its sprite or button and compiles its
/// handlers. `None` (reported) when it cannot be published.
fn resolve_instance(
    ctx: &mut Ctx<'_>,
    inst: SymbolInstance,
    origin: Origin,
    scene: Option<usize>,
) -> Option<InstanceItem> {
    let doc = ctx.doc;
    if !doc.library.symbols.contains_key(&inst.library_item) {
        ctx.error(
            origin,
            format!(
                "symbol '{}' is not in the library; the instance is left out",
                inst.library_item
            ),
        );
        return None;
    }
    let script = inst
        .script
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned);
    let target = match inst.kind {
        SymbolKind::Graphic => {
            if script.is_some() {
                ctx.warn(
                    origin.clone(),
                    "graphic instances cannot have scripts; the script is ignored",
                );
            }
            Target::Graphic
        }
        SymbolKind::MovieClip => {
            let char = symbol::sprite_char(ctx, &inst.library_item, &origin)?;
            let actions = match (&script, ctx.scripts_enabled()) {
                (Some(src), true) => script::clip_handlers(ctx, src, &origin),
                _ => None,
            };
            Target::Sprite { char, actions }
        }
        SymbolKind::Button => Target::Button {
            char: symbol::button_char(ctx, &inst, script.as_deref(), scene, &origin)?,
        },
    };
    let filters = match target {
        Target::Graphic => Vec::new(),
        _ => filter::filters(ctx, &inst.filters, &origin),
    };
    let script_hash = script.as_deref().map(hash_str);
    Some(InstanceItem {
        inst,
        target,
        filters,
        script_hash,
        origin,
    })
}

/// A stable-within-a-process hash of a string.
pub fn hash_str(s: &str) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// The keyframe after `ki` when `ki` starts a motion tween that reaches it.
fn tween_partner<'t>(
    ctx: &mut Ctx<'_>,
    tl: &'t Timeline,
    layer: usize,
    ki: usize,
    scope: &Scope,
) -> Option<(Rc<Expanded>, &'t MotionTween)> {
    let frames = &tl.layers.get(layer)?.frames;
    let kf = frames.get(ki)?;
    let Tween::Motion(mt) = &kf.tween else {
        return None;
    };
    let next = frames.get(ki + 1)?;
    if next.index != kf.end() {
        return None;
    }
    Some((expand(ctx, tl, layer, ki + 1, scope), mt))
}

/// Context for placing one keyframe's items.
struct Placing<'p> {
    scope: &'p Scope,
    layer: usize,
    layer_name: &'p str,
    base: u32,
    xform: Xform,
    prefix: &'p [IdentPart],
    /// The keyframe's first frame (for diagnostics).
    keyframe: u32,
    elapsed: u32,
    duration: u32,
}

/// Appends what timeline `tl` shows at `frame` to `out`, at depths from
/// `base`, transformed by `xform`. `stack` holds the graphic symbols being
/// flattened (recursion guard).
#[allow(clippy::too_many_arguments)]
pub fn plan_frame(
    ctx: &mut Ctx<'_>,
    tl: &Timeline,
    scope: &Scope,
    frame: u32,
    base: u32,
    xform: &Xform,
    prefix: &[IdentPart],
    stack: &mut Vec<String>,
    out: &mut Vec<DisplayItem>,
) {
    let plan = timeline_plan(ctx, tl, scope);
    for band in &plan.bands {
        let Some(layer) = tl.layers.get(band.layer) else {
            continue;
        };
        let Some(ki) = layer.keyframe_index_at(frame) else {
            continue;
        };
        let kf = &layer.frames[ki];
        let band_base = base.saturating_add(band.offset);
        if let LayerRole::Mask { .. } = band.role {
            // A mask whose masked layers take no depth would only show
            // unclipped: leave it out.
            let Some(mask_clip) = band.mask_clip else {
                continue;
            };
            // The mask's backmost element, placed like content (matrix,
            // instance name, motion tween) with the band's clip depth.
            let exp = mask::mask_items(ctx, tl, band.layer, ki, scope);
            let partner = match &kf.tween {
                Tween::Motion(mt) => layer
                    .frames
                    .get(ki + 1)
                    .filter(|next| next.index == kf.end())
                    .map(|_| (mask::mask_items(ctx, tl, band.layer, ki + 1, scope), mt)),
                _ => None,
            };
            let p = Placing {
                scope,
                layer: band.layer,
                layer_name: &layer.name,
                base: band_base,
                xform: *xform,
                prefix,
                keyframe: kf.index,
                elapsed: frame.saturating_sub(kf.index),
                duration: kf.duration,
            };
            let start = out.len();
            place_items(
                ctx,
                &exp,
                partner.as_ref().map(|(e, t)| (&**e, *t)),
                &p,
                stack,
                out,
            );
            let clip = u16::try_from(base.saturating_add(mask_clip).min(MAX_TIMELINE_DEPTH)).ok();
            for item in out.iter_mut().skip(start) {
                item.clip_depth = clip;
            }
            continue;
        }
        let exp = expand(ctx, tl, band.layer, ki, scope);
        let partner = tween_partner(ctx, tl, band.layer, ki, scope);
        let p = Placing {
            scope,
            layer: band.layer,
            layer_name: &layer.name,
            base: band_base,
            xform: *xform,
            prefix,
            keyframe: kf.index,
            elapsed: frame.saturating_sub(kf.index),
            duration: kf.duration,
        };
        place_items(
            ctx,
            &exp,
            partner.as_ref().map(|(e, t)| (&**e, *t)),
            &p,
            stack,
            out,
        );
    }
}

/// Places the items of one expanded keyframe.
fn place_items(
    ctx: &mut Ctx<'_>,
    exp: &Expanded,
    partner: Option<(&Expanded, &MotionTween)>,
    p: &Placing<'_>,
    stack: &mut Vec<String>,
    out: &mut Vec<DisplayItem>,
) {
    let origin = p.scope.origin(Some(p.layer_name), Some(p.keyframe));
    let mut off = 0u32;
    for (slot, item) in exp.items.iter().enumerate() {
        let need = item_need(ctx, item);
        let depth = p.base.saturating_add(off);
        off = off.saturating_add(need);
        if depth > MAX_TIMELINE_DEPTH {
            // Depths only grow from here: nothing else in this keyframe can
            // be placed. Stopping also bounds the walk through graphics that
            // nest exponentially.
            depth_overflow(ctx, &origin);
            break;
        }
        if need == 0 && matches!(item, Item::Instance(ii) if matches!(ii.target, Target::Graphic)) {
            // A graphic that never shows anything (or a recursive one,
            // reported by `symbol_need`): nothing to walk into.
            continue;
        }
        let mut ident = p.prefix.to_vec();
        let part = |what| IdentPart {
            layer: p.layer as u32,
            slot: slot as u32,
            what,
        };
        match item {
            Item::Shape { char, matrix } => {
                ident.push(part(IdentKind::Shape));
                let x = Xform {
                    matrix: p.xform.matrix.multiply(matrix),
                    color: p.xform.color,
                };
                if let Some(mut it) = leaf(ctx, depth, ident, *char, &x, &origin) {
                    it.replaceable = ctx.kind_of(*char) == Some(CharKind::Shape);
                    out.push(it);
                }
            }
            Item::Text { char, matrix, name } => {
                ident.push(part(IdentKind::Unique(*char)));
                let x = Xform {
                    matrix: p.xform.matrix.multiply(matrix),
                    color: p.xform.color,
                };
                if let Some(mut it) = leaf(ctx, depth, ident, *char, &x, &origin) {
                    it.name = name.clone();
                    out.push(it);
                }
            }
            Item::Instance(ii) => {
                let inst = &ii.inst;
                let to = partner.and_then(|(e, t)| match e.items.get(slot) {
                    Some(Item::Instance(other))
                        if other.inst.library_item == inst.library_item
                            && other.inst.kind == inst.kind =>
                    {
                        Some((&other.inst, t))
                    }
                    _ => None,
                });
                let (matrix, color, tweened_filters) = match to {
                    Some((to, t)) if p.elapsed > 0 && p.elapsed < p.duration => {
                        let s = tween::tweened_state(t, inst, to, p.elapsed, p.duration);
                        (s.matrix, s.color, Some(s.filters))
                    }
                    _ => (inst.matrix, inst.color, None),
                };
                let x = Xform {
                    matrix: p.xform.matrix.multiply(&matrix),
                    color: units::compose_color(&p.xform.color, &color),
                };
                ident.push(part(IdentKind::Symbol {
                    item: inst.library_item.clone(),
                    kind: match inst.kind {
                        SymbolKind::MovieClip => 0,
                        SymbolKind::Graphic => 1,
                        SymbolKind::Button => 2,
                    },
                    name: inst.name.clone(),
                    script: ii.script_hash,
                    track_as_menu: inst.track_as_menu,
                }));
                let (char, actions) = match &ii.target {
                    Target::Sprite { char, actions } => (*char, actions.clone()),
                    Target::Button { char } => (*char, None),
                    Target::Graphic => {
                        place_graphic(ctx, ii, &x, depth, &ident, p.elapsed, stack, out);
                        continue;
                    }
                };
                let Some(mut it) = leaf(ctx, depth, ident, char, &x, &ii.origin) else {
                    continue;
                };
                it.name = inst.name.clone().filter(|n| !n.is_empty());
                it.clip_actions = actions;
                it.filters = match tweened_filters {
                    Some(f) => filter::filters(ctx, &f, &ii.origin),
                    None => ii.filters.clone(),
                };
                it.blend = filter::blend_mode(inst.blend_mode);
                it.cache = inst.cache_as_bitmap;
                out.push(it);
            }
        }
    }
}

/// Flattens a graphic instance into the parent at `depth`.
#[allow(clippy::too_many_arguments)]
fn place_graphic(
    ctx: &mut Ctx<'_>,
    ii: &InstanceItem,
    x: &Xform,
    depth: u32,
    ident: &[IdentPart],
    elapsed: u32,
    stack: &mut Vec<String>,
    out: &mut Vec<DisplayItem>,
) {
    let name = &ii.inst.library_item;
    let doc = ctx.doc;
    let Some(sym) = doc.library.symbols.get(name) else {
        return;
    };
    if stack.iter().any(|s| s == name) || stack.len() >= MAX_NESTING {
        ctx.error(
            ii.origin.clone(),
            format!(
                "graphic symbol '{name}' contains itself (or nests too deeply); it is left out"
            ),
        );
        return;
    }
    let playback = ii.inst.graphic.unwrap_or_default();
    let Some(inner) = graphic::inner_frame(playback, sym.timeline.frame_count(), elapsed) else {
        return;
    };
    let scope = Scope {
        timeline: name.clone(),
        flattened: true,
        scene: None,
    };
    stack.push(name.clone());
    plan_frame(
        ctx,
        &sym.timeline,
        &scope,
        inner,
        depth,
        x,
        ident,
        stack,
        out,
    );
    stack.pop();
}

/// A display item at `depth`, or `None` (reported) past the last timeline
/// depth.
fn leaf(
    ctx: &mut Ctx<'_>,
    depth: u32,
    ident: Vec<IdentPart>,
    char: CharacterId,
    x: &Xform,
    origin: &Origin,
) -> Option<DisplayItem> {
    if depth == 0 || depth > MAX_TIMELINE_DEPTH {
        depth_overflow(ctx, origin);
        return None;
    }
    let mut item = DisplayItem::new(depth as u16, ident, char, units::matrix(&x.matrix));
    item.cxform = units::color_transform(&x.color);
    Some(item)
}

/// Reports, once per timeline, content past the last timeline depth.
fn depth_overflow(ctx: &mut Ctx<'_>, origin: &Origin) {
    let o = Origin {
        layer: None,
        frame: None,
        instance: None,
        ..origin.clone()
    };
    ctx.error(
        o,
        format!("content needs more than {MAX_TIMELINE_DEPTH} depths; the rest is left out"),
    );
}

/// Most frames an SWF timeline can have (`num_frames` is a u16).
pub const MAX_FRAMES: u32 = 65535;

/// Builds the frames `first..first + count` of a timeline into frame ops,
/// continuing the display list `dl`. `prefix` starts every identity (the
/// root uses it to separate scenes).
pub fn build_frames(
    ctx: &mut Ctx<'_>,
    tl: &Timeline,
    scope: &Scope,
    dl: &mut DisplayList,
    prefix: &[IdentPart],
    count: u32,
) -> Vec<FrameOps> {
    let mut frames = Vec::with_capacity(count.min(MAX_FRAMES) as usize);
    for f in 0..count.min(MAX_FRAMES) {
        let mut desired = Vec::new();
        let mut stack = Vec::new();
        plan_frame(
            ctx,
            tl,
            scope,
            f,
            1,
            &Xform::IDENTITY,
            prefix,
            &mut stack,
            &mut desired,
        );
        let (removes, places) = dl.diff(desired);
        let actions = frame_scripts(ctx, tl, scope, f);
        let labels = frame_labels(ctx, tl, f);
        frames.push(FrameOps {
            removes,
            actions,
            labels,
            places,
        });
    }
    frames
}

/// Whether a layer's scripts and labels are published.
fn layer_has_frame_data(ctx: &Ctx<'_>, kind: &LayerKind, visible: bool) -> bool {
    matches!(kind, LayerKind::Normal | LayerKind::Mask)
        && (visible || ctx.doc.publish.publish_hidden_layers)
}

/// Compiled frame scripts that start at frame `f`, top layer first.
fn frame_scripts(ctx: &mut Ctx<'_>, tl: &Timeline, scope: &Scope, f: u32) -> Vec<Vec<u8>> {
    if !ctx.scripts_enabled() || scope.flattened {
        return Vec::new();
    }
    let mut out = Vec::new();
    for layer in &tl.layers {
        if !layer_has_frame_data(ctx, &layer.kind, layer.visible) {
            continue;
        }
        let Some(kf) = layer.frame_at(f).filter(|k| k.index == f) else {
            continue;
        };
        let Some(src) = kf.script.as_deref().filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        let origin = scope.origin(Some(&layer.name), Some(f));
        if let Some(bytes) = script::frame_actions(ctx, src, &origin, scope.scene) {
            out.push(bytes);
        }
    }
    out
}

/// Frame labels and anchors that start at frame `f`, top layer first.
fn frame_labels(ctx: &Ctx<'_>, tl: &Timeline, f: u32) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for layer in &tl.layers {
        if !layer_has_frame_data(ctx, &layer.kind, layer.visible) {
            continue;
        }
        let Some(kf) = layer.frame_at(f).filter(|k| k.index == f) else {
            continue;
        };
        if let Some(label) = &kf.label {
            match label.kind {
                LabelKind::Name if !label.name.is_empty() => out.push((label.name.clone(), false)),
                LabelKind::Anchor if !label.name.is_empty() => out.push((label.name.clone(), true)),
                _ => {}
            }
        }
    }
    out
}

/// Defines a one-frame sprite showing `elements` (back to front), for
/// generated content such as mask shapes. Graphics show their first frame.
#[allow(dead_code)] // seam: used by the mask implementation
pub fn define_static_sprite(
    ctx: &mut Ctx<'_>,
    elements: &[Element],
    scope: &Scope,
    layer_name: &str,
    frame: u32,
) -> Option<CharacterId> {
    let origin = scope.origin(Some(layer_name), Some(frame));
    let exp = expand_elements(ctx, elements, scope, layer_name, frame);
    let p = Placing {
        scope,
        layer: 0,
        layer_name,
        base: 1,
        xform: Xform::IDENTITY,
        prefix: &[],
        keyframe: frame,
        elapsed: 0,
        duration: 1,
    };
    let mut desired = Vec::new();
    place_items(ctx, &exp, None, &p, &mut Vec::new(), &mut desired);
    let mut dl = DisplayList::default();
    let (removes, places) = dl.diff(desired);
    let ops = FrameOps {
        removes,
        places,
        ..FrameOps::default()
    };
    let id = ctx.reserve_id(&origin)?;
    let mut deps = Vec::new();
    let mut children: Vec<RawTag> = Vec::new();
    encode_frame(
        ctx,
        &ops,
        &mut DefSink::Sprite(&mut deps),
        &mut children,
        &origin,
    );
    children.push(RawTag::show_frame());
    ctx.set_definition(
        id,
        CharKind::Sprite,
        vec![tags::sprite(id, 1, &children)],
        deps,
    );
    Some(id)
}

#[cfg(test)]
mod tests;
