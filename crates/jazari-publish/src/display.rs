//! The display list: what should be on screen at each frame, and the
//! PlaceObject/RemoveObject tags that get there from the previous frame.
//!
//! The timeline code describes every frame declaratively as a list of
//! [`DisplayItem`]s (one per depth). [`DisplayList::diff`] compares it with
//! the previous frame, depth by depth, following Flash's identity rules:
//!
//! - same [`Ident`] and same character: PlaceObject2 *Modify* with only the
//!   properties that changed (nothing at all when none did);
//! - same identity, both plain shapes, another character: *Replace*, as Flash
//!   swaps the frames of a flattened graphic;
//! - otherwise: RemoveObject2, then a new *Place*.
//!
//! Instance names, clip depths and clip actions are part of the identity:
//! Flash applies them only at first placement, so a change re-places.
//! PlaceObject3 is used only when filters, a blend mode or cacheAsBitmap are
//! written.

use std::collections::BTreeMap;
use std::rc::Rc;

use jazari_as2::{ClipHandler, Origin};
use swf::{
    BlendMode, CharacterId, ClipAction, ClipActions, ClipEventFlag, ColorTransform, Filter,
    FrameLabel, Matrix, PlaceObject, PlaceObjectAction, RemoveObject, Tag,
};

use crate::context::Ctx;
use crate::tags::{self, RawTag};

/// The highest timeline depth. Depths from 16384 up belong to scripts
/// (`AVM_DEPTH_BIAS`): objects there survive gotos and loops.
pub const MAX_TIMELINE_DEPTH: u32 = 16383;

/// One step of an identity path: which slot of which layer, holding what.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdentPart {
    /// Layer index in its timeline (or a scene number for the root step).
    pub layer: u32,
    /// Slot (item index) in the keyframe.
    pub slot: u32,
    /// What the slot holds.
    pub what: IdentKind,
}

/// What a slot holds, as far as identity is concerned.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdentKind {
    /// The root step of a scene: nothing survives a scene change.
    Scene(usize),
    /// A run of shapes (swapped with *Replace* when it changes).
    Shape,
    /// A text or another character that is identical only to itself.
    Unique(CharacterId),
    /// A symbol instance.
    Symbol {
        /// Library item.
        item: String,
        /// 0 = movie clip, 1 = graphic, 2 = button.
        kind: u8,
        /// Instance name.
        name: Option<String>,
        /// Hash of the instance script (handlers are set at placement).
        script: Option<u64>,
        /// Button track-as-menu (part of the button definition).
        track_as_menu: bool,
    },
}

/// An identity: the path from the root timeline to the item.
pub type Ident = Vec<IdentPart>;

/// What should be at one depth in one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayItem {
    /// Depth (1..=16383).
    pub depth: u16,
    /// Identity across frames.
    pub ident: Ident,
    /// The character.
    pub char: CharacterId,
    /// Whether a character change may use *Replace* (plain shapes).
    pub replaceable: bool,
    /// Placement matrix, rounded.
    pub matrix: Matrix,
    /// Colour transform, rounded.
    pub cxform: ColorTransform,
    /// Instance name.
    pub name: Option<String>,
    /// Clip depth for masks.
    pub clip_depth: Option<u16>,
    /// Clip actions (movie clips).
    pub clip_actions: Option<Rc<Vec<ClipHandler>>>,
    /// Filters.
    pub filters: Vec<Filter>,
    /// Blend mode.
    pub blend: BlendMode,
    /// cacheAsBitmap.
    pub cache: bool,
}

impl DisplayItem {
    /// An item with default properties.
    pub fn new(depth: u16, ident: Ident, char: CharacterId, matrix: Matrix) -> Self {
        Self {
            depth,
            ident,
            char,
            replaceable: false,
            matrix,
            cxform: ColorTransform::IDENTITY,
            name: None,
            clip_depth: None,
            clip_actions: None,
            filters: Vec::new(),
            blend: BlendMode::Normal,
            cache: false,
        }
    }
}

/// One PlaceObject2/3 to write.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceOp {
    /// Depth.
    pub depth: u16,
    /// Place, Modify or Replace.
    pub action: PlaceObjectAction,
    /// Matrix, when written.
    pub matrix: Option<Matrix>,
    /// Colour transform, when written.
    pub cxform: Option<ColorTransform>,
    /// Instance name (first placement only).
    pub name: Option<String>,
    /// Clip depth (first placement only).
    pub clip_depth: Option<u16>,
    /// Ratio (first placement only): see [`mark_ratios`].
    pub ratio: Option<u16>,
    /// Clip actions (first placement only).
    pub clip_actions: Option<Rc<Vec<ClipHandler>>>,
    /// Filters, when written (an empty list clears them).
    pub filters: Option<Vec<Filter>>,
    /// Blend mode, when written.
    pub blend: Option<BlendMode>,
    /// cacheAsBitmap, when written.
    pub cache: Option<bool>,
}

impl PlaceOp {
    /// The character this op places, if any.
    pub fn character(&self) -> Option<CharacterId> {
        match self.action {
            PlaceObjectAction::Place(id) | PlaceObjectAction::Replace(id) => Some(id),
            PlaceObjectAction::Modify => None,
        }
    }
}

/// Everything one frame writes, before encoding.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameOps {
    /// Depths to remove, ascending.
    pub removes: Vec<u16>,
    /// DoAction payloads, in execution order.
    pub actions: Vec<Vec<u8>>,
    /// Frame labels (`true` = named anchor).
    pub labels: Vec<(String, bool)>,
    /// Placements, ascending by depth.
    pub places: Vec<PlaceOp>,
}

/// The objects on a timeline after the last frame.
#[derive(Debug, Default)]
pub struct DisplayList {
    items: BTreeMap<u16, DisplayItem>,
}

impl DisplayList {
    /// Moves to the `desired` state and returns the removals and placements
    /// that do it. Later items at an already used depth are ignored.
    pub fn diff(&mut self, desired: Vec<DisplayItem>) -> (Vec<u16>, Vec<PlaceOp>) {
        let mut next: BTreeMap<u16, DisplayItem> = BTreeMap::new();
        for item in desired {
            next.entry(item.depth).or_insert(item);
        }
        let mut removes = Vec::new();
        let mut places = Vec::new();
        for (depth, old) in &self.items {
            match next.get(depth) {
                Some(new) if continues(old, new) => {}
                _ => removes.push(*depth),
            }
        }
        for (depth, new) in &next {
            match self.items.get(depth) {
                Some(old) if continues(old, new) => {
                    if let Some(op) = modify(old, new) {
                        places.push(op);
                    }
                }
                _ => places.push(place(new)),
            }
        }
        self.items = next;
        (removes, places)
    }
}

/// Whether `new` continues `old` at the same depth (Modify or Replace).
fn continues(old: &DisplayItem, new: &DisplayItem) -> bool {
    old.ident == new.ident
        && old.name == new.name
        && old.clip_depth == new.clip_depth
        && (old.char == new.char || (old.replaceable && new.replaceable))
}

fn place(new: &DisplayItem) -> PlaceOp {
    PlaceOp {
        depth: new.depth,
        action: PlaceObjectAction::Place(new.char),
        matrix: Some(new.matrix),
        cxform: (new.cxform != ColorTransform::IDENTITY).then_some(new.cxform),
        name: new.name.clone(),
        clip_depth: new.clip_depth,
        ratio: None,
        clip_actions: new.clip_actions.clone().filter(|a| !a.is_empty()),
        filters: (!new.filters.is_empty()).then(|| new.filters.clone()),
        blend: (new.blend != BlendMode::Normal).then_some(new.blend),
        cache: new.cache.then_some(true),
    }
}

fn modify(old: &DisplayItem, new: &DisplayItem) -> Option<PlaceOp> {
    let action = if old.char == new.char {
        PlaceObjectAction::Modify
    } else {
        PlaceObjectAction::Replace(new.char)
    };
    let op = PlaceOp {
        depth: new.depth,
        action,
        matrix: (old.matrix != new.matrix).then_some(new.matrix),
        cxform: (old.cxform != new.cxform).then_some(new.cxform),
        name: None,
        clip_depth: None,
        ratio: None,
        clip_actions: None,
        filters: (old.filters != new.filters).then(|| new.filters.clone()),
        blend: (old.blend != new.blend).then_some(new.blend),
        cache: (old.cache != new.cache).then_some(new.cache),
    };
    let unchanged = action == PlaceObjectAction::Modify
        && op.matrix.is_none()
        && op.cxform.is_none()
        && op.filters.is_none()
        && op.blend.is_none()
        && op.cache.is_none();
    (!unchanged).then_some(op)
}

/// Gives every new placement of a movie clip after the timeline's first
/// frame `ratio` = its 0-based frame number, as Flash 8 writes it (the
/// `scenes` fixture: the dot clip placed again at root frame 4 has ratio 4;
/// shapes never get one, and frame 0 placements have none). Players compare
/// ratios on a backwards goto to tell a clip placed later, which must be
/// replaced by a fresh instance, from the one the target frame placed.
/// `first` is the timeline frame of `frames[0]` (scenes are concatenated on
/// the root). Frames past `u16::MAX` cannot exist in an SWF timeline.
pub fn mark_ratios(ctx: &Ctx<'_>, frames: &mut [FrameOps], first: u32) {
    for (i, ops) in frames.iter_mut().enumerate() {
        let frame = u32::try_from(i).map_or(u32::MAX, |i| first.saturating_add(i));
        let Ok(ratio) = u16::try_from(frame) else {
            break;
        };
        if ratio == 0 {
            continue;
        }
        for op in &mut ops.places {
            if let PlaceObjectAction::Place(id) = op.action
                && ctx.kind_of(id) == Some(crate::context::CharKind::Sprite)
            {
                op.ratio = Some(ratio);
            }
        }
    }
}

/// Where the definitions of placed characters go.
pub enum DefSink<'s> {
    /// The root timeline: definitions are written inline, before first use.
    Root,
    /// A sprite: placed characters become dependencies of the sprite.
    Sprite(&'s mut Vec<CharacterId>),
}

/// Encodes a frame's removals, actions, labels and placements (not its
/// ShowFrame) into `out`. Tags that fail to encode are reported at `origin`
/// and left out.
pub fn encode_frame(
    ctx: &mut Ctx<'_>,
    ops: &FrameOps,
    sink: &mut DefSink<'_>,
    out: &mut Vec<RawTag>,
    origin: &Origin,
) {
    for depth in &ops.removes {
        push(
            ctx,
            out,
            origin,
            &Tag::RemoveObject(RemoveObject {
                depth: *depth,
                character_id: None,
            }),
        );
    }
    for a in &ops.actions {
        push(ctx, out, origin, &Tag::DoAction(a));
    }
    for (label, anchor) in &ops.labels {
        push(
            ctx,
            out,
            origin,
            &Tag::FrameLabel(FrameLabel {
                label: label.as_str().into(),
                is_anchor: *anchor,
            }),
        );
    }
    for op in &ops.places {
        if let Some(id) = op.character() {
            match sink {
                DefSink::Root => ctx.emit_definition(id, out),
                DefSink::Sprite(deps) => deps.push(id),
            }
        }
        let tag = place_tag(op);
        push(ctx, out, origin, &Tag::PlaceObject(Box::new(tag)));
    }
}

/// Builds the PlaceObject for an op (borrowing its strings and actions).
pub fn place_tag(op: &PlaceOp) -> PlaceObject<'_> {
    let clip_actions = op.clip_actions.as_deref().map(|handlers| {
        let records: Vec<ClipAction<'_>> = handlers
            .iter()
            .map(|h| ClipAction {
                events: h.events,
                key_code: h
                    .key_code
                    .filter(|_| h.events.contains(ClipEventFlag::KEY_PRESS)),
                action_data: &h.actions,
            })
            .collect();
        let all_event_flags = records
            .iter()
            .fold(ClipEventFlag::empty(), |acc, r| acc | r.events);
        ClipActions {
            all_event_flags,
            records,
        }
    });
    let v3 = op.filters.is_some() || op.blend.is_some() || op.cache.is_some();
    PlaceObject {
        version: if v3 { 3 } else { 2 },
        action: op.action,
        depth: op.depth,
        matrix: op.matrix,
        color_transform: op.cxform,
        ratio: op.ratio,
        name: op.name.as_deref().map(Into::into),
        clip_depth: op.clip_depth,
        class_name: None,
        filters: op.filters.clone(),
        background_color: None,
        blend_mode: op.blend,
        clip_actions,
        has_image: false,
        is_bitmap_cached: op.cache,
        is_visible: None,
        amf_data: None,
    }
}

/// Encodes one tag into `out`, reporting failures.
pub fn push(ctx: &mut Ctx<'_>, out: &mut Vec<RawTag>, origin: &Origin, tag: &Tag<'_>) {
    match tags::encode(tag) {
        Ok(raw) => out.push(raw),
        Err(e) => ctx.error(origin.clone(), format!("left out: {e}")),
    }
}

#[cfg(test)]
mod tests;
