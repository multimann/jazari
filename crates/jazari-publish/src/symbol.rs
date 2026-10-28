//! Movie clips (DefineSprite) and buttons (DefineButton2).
//!
//! A movie clip symbol is built once, the first time an instance needs it,
//! into a sprite whose `num_frames` equals its ShowFrame count. Its
//! definitions stay at root level: the sprite lists the characters it places
//! as dependencies. In the stage build every sprite gets `stop()` on frame 1.
//!
//! A button is built from frames 0 to 3 (up, over, down, hit) across all
//! layers, records merged when identical across states. A state past the
//! end of the button's timeline shows its last frame: a Flash 8 probe
//! published a one-frame button as one record for all four states, and a
//! two-frame button's down and hit states as its over frame (the Adobe
//! documentation's "the up frame is the hit area" does not hold). A button
//! instance with `on()`
//! handlers gets its own DefineButton2 copy carrying them; copies with the
//! same handlers are shared.
//!
//! Symbols with linkage are exported with ExportAssets right after their
//! definition, and `Object.registerClass` is emitted as their DoInitAction
//! when they name an AS2 class.

use jazari_as2::Origin;
use jazari_xfl::model::{Symbol, SymbolInstance};
use swf::{Button, ButtonAction, ButtonRecord, ButtonState, CharacterId, ExportedAsset, Tag};

use crate::BuildMode;
use crate::context::{CharKind, Ctx, MAX_NESTING, Scope};
use crate::display::{DefSink, DisplayList, encode_frame, mark_ratios, push};
use crate::script;
use crate::tags::{self, RawTag};
use crate::timeline::{self, MAX_FRAMES, Xform};

/// The largest action body a DefineButton2 condition can carry: offsets
/// are u16 and include the 4-byte condition header.
const MAX_BUTTON_ACTION: usize = 65535 - 4;

/// The sprite of a movie clip symbol, built on first use. `None` (reported
/// at `origin`) for a missing or recursive symbol.
pub fn sprite_char(ctx: &mut Ctx<'_>, name: &str, origin: &Origin) -> Option<CharacterId> {
    if let Some(r) = ctx.sprites.get(name) {
        return *r;
    }
    if ctx.building.iter().any(|b| b == name) || ctx.building.len() >= MAX_NESTING {
        ctx.error(
            origin.clone(),
            format!(
                "symbol '{name}' contains itself (or nests too deeply); this instance is left out"
            ),
        );
        return None;
    }
    let doc = ctx.doc;
    let Some(sym) = doc.library.symbols.get(name) else {
        ctx.error(
            origin.clone(),
            format!("symbol '{name}' is not in the library"),
        );
        return None;
    };
    ctx.building.push(name.to_owned());
    let id = build_sprite(ctx, sym, origin);
    ctx.building.pop();
    ctx.sprites.insert(name.to_owned(), id);
    id
}

fn build_sprite(ctx: &mut Ctx<'_>, sym: &Symbol, origin: &Origin) -> Option<CharacterId> {
    let scope = Scope::new(sym.name.clone());
    let total = sym.timeline.frame_count();
    if total > MAX_FRAMES {
        ctx.error(
            scope.origin(None, None),
            format!("{total} frames exceed the SWF limit of {MAX_FRAMES}; the rest is left out"),
        );
    }
    if sym.scale9.is_some() {
        ctx.warn(
            scope.origin(None, None),
            "9-slice scaling is not published (DefineScalingGrid); the clip scales normally",
        );
    }
    let count = total.clamp(1, MAX_FRAMES);
    let mut dl = DisplayList::default();
    let mut frames = timeline::build_frames(ctx, &sym.timeline, &scope, &mut dl, &[], count);
    mark_ratios(ctx, &mut frames, 0);
    if ctx.mode == BuildMode::Stage
        && let Some(first) = frames.first_mut()
    {
        first.actions = vec![script::STOP.to_vec()];
    }
    let id = ctx.reserve_id(origin)?;
    let mut deps = Vec::new();
    let mut children: Vec<RawTag> = Vec::new();
    for (f, ops) in frames.iter().enumerate() {
        let o = scope.origin(None, u32::try_from(f).ok());
        encode_frame(ctx, ops, &mut DefSink::Sprite(&mut deps), &mut children, &o);
        children.push(RawTag::show_frame());
    }
    let num_frames = u16::try_from(frames.len()).unwrap_or(u16::MAX);
    let mut defs = vec![tags::sprite(id, num_frames, &children)];
    defs.extend(linkage_tags(ctx, sym, id, &scope.origin(None, None)));
    deps.sort_unstable();
    deps.dedup();
    ctx.set_definition(id, CharKind::Sprite, defs, deps);
    Some(id)
}

/// ExportAssets for a linked symbol, and its `Object.registerClass`
/// DoInitAction in release builds.
pub fn linkage_tags(
    ctx: &mut Ctx<'_>,
    sym: &Symbol,
    id: CharacterId,
    origin: &Origin,
) -> Vec<RawTag> {
    let mut out = Vec::new();
    let Some(link) = sym.linkage.as_ref().filter(|l| l.export_for_as) else {
        return out;
    };
    if link.identifier.is_empty() {
        return out;
    }
    push(
        ctx,
        &mut out,
        origin,
        &Tag::ExportAssets(vec![ExportedAsset {
            id,
            name: link.identifier.as_str().into(),
        }]),
    );
    if let Some(class) = link.class_name.as_deref().filter(|c| !c.trim().is_empty())
        && ctx.scripts_enabled()
        && let Some(bytes) = script::register_class(ctx, &link.identifier, class, origin)
    {
        push(
            ctx,
            &mut out,
            origin,
            &Tag::DoInitAction {
                id,
                action_data: &bytes,
            },
        );
    }
    out
}

/// The DefineButton2 for a button instance (shared unless the instance has
/// handlers or track-as-menu differs). `None` (reported) for a missing or
/// recursive symbol.
pub fn button_char(
    ctx: &mut Ctx<'_>,
    inst: &SymbolInstance,
    script_src: Option<&str>,
    scene: Option<usize>,
    origin: &Origin,
) -> Option<CharacterId> {
    let name = inst.library_item.as_str();
    let script_key = script_src
        .filter(|_| ctx.scripts_enabled())
        .map(|s| (s.to_owned(), scene));
    let key = (name.to_owned(), inst.track_as_menu, script_key.clone());
    if let Some(r) = ctx.buttons.get(&key) {
        return *r;
    }
    if ctx.building.iter().any(|b| b == name) || ctx.building.len() >= MAX_NESTING {
        ctx.error(
            origin.clone(),
            format!(
                "symbol '{name}' contains itself (or nests too deeply); this instance is left out"
            ),
        );
        return None;
    }
    let doc = ctx.doc;
    let sym = doc.library.symbols.get(name)?;
    ctx.building.push(name.to_owned());
    let (records, deps) = button_records(ctx, sym);
    ctx.building.pop();
    let mut handlers = match &script_key {
        Some((src, scene)) => script::button_handlers(ctx, src, *scene, origin).unwrap_or_default(),
        None => Vec::new(),
    };
    if handlers.iter().any(|h| h.actions.len() > MAX_BUTTON_ACTION) {
        ctx.error(
            origin.clone(),
            "a button handler is larger than 64 KB, which DefineButton2 cannot hold; the handlers are left out",
        );
        handlers.clear();
    }
    let track = inst.track_as_menu;
    let id = ctx.define(CharKind::Button, deps, origin, |id| {
        let actions = handlers
            .iter()
            .map(|h| ButtonAction {
                conditions: h.conditions,
                action_data: &h.actions,
            })
            .collect();
        let button = Button {
            id,
            is_track_as_menu: track,
            records,
            actions,
        };
        Ok(vec![tags::encode(&Tag::DefineButton2(Box::new(button)))?])
    });
    if let Some(id) = id
        && script_key.is_none()
        && !track
    {
        let o = Scope::new(sym.name.clone()).origin(None, None);
        let extra = linkage_tags(ctx, sym, id, &o);
        ctx.append_definition_tags(id, extra);
    }
    ctx.buttons.insert(key, id);
    id
}

/// Button records for the four states, and the characters they use.
pub fn button_records(ctx: &mut Ctx<'_>, sym: &Symbol) -> (Vec<ButtonRecord>, Vec<CharacterId>) {
    let scope = Scope::new(sym.name.clone());
    let tl = &sym.timeline;
    let last = tl.frame_count().saturating_sub(1);
    let states = [
        (0, ButtonState::UP),
        (1, ButtonState::OVER),
        (2, ButtonState::DOWN),
        (3, ButtonState::HIT_TEST),
    ];
    let mut records: Vec<ButtonRecord> = Vec::new();
    let mut deps = Vec::new();
    for (frame, bit) in states {
        let frame = frame.min(last);
        let mut items = Vec::new();
        timeline::plan_frame(
            ctx,
            tl,
            &scope,
            frame,
            1,
            &Xform::IDENTITY,
            &[],
            &mut Vec::new(),
            &mut items,
        );
        for it in items {
            if it.clip_depth.is_some() {
                // A button record cannot clip: drawing the mask as content
                // would cover the button with it.
                ctx.warn(
                    scope.origin(None, Some(frame)),
                    "masks inside buttons are not published (a button record cannot clip); the masked layers show unmasked",
                );
                continue;
            }
            deps.push(it.char);
            let same = records.iter_mut().find(|r| {
                r.id == it.char
                    && r.depth == it.depth
                    && r.matrix == it.matrix
                    && r.color_transform == it.cxform
                    && r.filters == it.filters
                    && r.blend_mode == it.blend
            });
            match same {
                Some(r) => r.states |= bit,
                None => records.push(ButtonRecord {
                    states: bit,
                    id: it.char,
                    depth: it.depth,
                    matrix: it.matrix,
                    color_transform: it.cxform,
                    filters: it.filters,
                    blend_mode: it.blend,
                }),
            }
        }
    }
    records.sort_by_key(|r| r.depth);
    deps.sort_unstable();
    deps.dedup();
    (records, deps)
}
