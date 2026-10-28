//! The root timeline: header tags, exported symbols, AS2 classes and the
//! scenes concatenated into one timeline.
//!
//! Tag order per frame follows Flash 8 (checked on the fixtures):
//! RemoveObject2, DoAction, FrameLabel, then each placement preceded by the
//! definitions it needs, then ShowFrame. Frame 1 starts with FileAttributes
//! (never AS3), SetBackgroundColor, Protect (when set), the `__Packages`
//! class modules and the symbols exported for ActionScript.

use jazari_as2::Origin;
use jazari_xfl::model::{SymbolKind, Timeline};
use swf::{CharacterId, Color, FileAttributes, Tag};

use crate::BuildMode;
use crate::context::{Ctx, Scope};
use crate::display::{
    DefSink, DisplayList, FrameOps, IdentKind, IdentPart, encode_frame, mark_ratios, push,
};
use crate::script;
use crate::symbol;
use crate::tags::RawTag;
use crate::timeline::{self, MAX_FRAMES};

/// The encoded root timeline.
#[derive(Debug)]
pub struct Movie {
    /// Root tags, without the end tag.
    pub tags: Vec<RawTag>,
    /// Number of root frames (ShowFrame tags).
    pub num_frames: u16,
    /// Scene and frame of every root frame.
    pub frame_map: Vec<(usize, u32)>,
}

/// The display name of a scene.
pub fn scene_name(scene: &Timeline, index: usize) -> String {
    if scene.name.trim().is_empty() {
        format!("Scene {}", index + 1)
    } else {
        scene.name.clone()
    }
}

/// Builds the root timeline.
pub fn build(ctx: &mut Ctx<'_>) -> Movie {
    let doc = ctx.doc;
    let origin = Origin::default();
    check_settings(ctx);

    let mut head = Vec::new();
    let mut attrs = FileAttributes::empty();
    if doc.publish.use_network {
        attrs |= FileAttributes::USE_NETWORK_SANDBOX;
    }
    push(ctx, &mut head, &origin, &Tag::FileAttributes(attrs));
    let bg = doc.background;
    push(
        ctx,
        &mut head,
        &origin,
        &Tag::SetBackgroundColor(Color {
            r: bg.r,
            g: bg.g,
            b: bg.b,
            a: 255,
        }),
    );
    if doc.publish.protect {
        push(ctx, &mut head, &origin, &Tag::Protect(None));
    }

    let exported = exported_symbols(ctx);

    let mut dl = DisplayList::default();
    let mut frames: Vec<FrameOps> = Vec::new();
    let mut frame_map = Vec::new();
    for (si, scene) in doc.scenes.iter().enumerate() {
        let scope = Scope::scene(scene_name(scene, si), si);
        let room = MAX_FRAMES - u32::try_from(frames.len()).unwrap_or(MAX_FRAMES);
        let mut count = scene.frame_count();
        if count > room {
            ctx.error(
                scope.origin(None, None),
                format!(
                    "the movie exceeds the SWF limit of {MAX_FRAMES} frames; the rest is left out"
                ),
            );
            count = room;
        }
        let prefix = [IdentPart {
            layer: u32::MAX,
            slot: 0,
            what: IdentKind::Scene(si),
        }];
        let mut ops = timeline::build_frames(ctx, scene, &scope, &mut dl, &prefix, count);
        let first = u32::try_from(frames.len()).unwrap_or(u32::MAX);
        mark_ratios(ctx, &mut ops, first);
        frame_map.extend((0..ops.len()).map(|f| (si, f as u32)));
        frames.extend(ops);
    }
    if frames.is_empty() {
        // An SWF needs at least one frame.
        frames.push(FrameOps::default());
        if !doc.scenes.is_empty() {
            frame_map.push((0, 0));
        }
    }
    if ctx.mode == BuildMode::Stage
        && let Some(hook) = script::stage_hook(ctx)
    {
        frames[0].actions.insert(0, hook);
    }

    let linked = linked_classes(ctx);
    let class_tags = if ctx.scripts_enabled() {
        script::class_tags(ctx, &linked)
    } else {
        Vec::new()
    };

    let mut out = head;
    out.extend(class_tags);
    for id in exported {
        ctx.emit_definition(id, &mut out);
    }
    for (f, ops) in frames.iter().enumerate() {
        let o = match frame_map.get(f) {
            Some((si, sf)) => Scope::new(scene_name(&doc.scenes[*si], *si)).origin(None, Some(*sf)),
            None => Origin::default(),
        };
        encode_frame(ctx, ops, &mut DefSink::Root, &mut out, &o);
        out.push(RawTag::show_frame());
    }
    Movie {
        tags: out,
        num_frames: u16::try_from(frames.len()).unwrap_or(u16::MAX),
        frame_map,
    }
}

/// Reports publish settings we do not honour.
fn check_settings(ctx: &mut Ctx<'_>) {
    let p = &ctx.doc.publish;
    let origin = Origin {
        file: Some("PublishSettings.xml".to_owned()),
        ..Origin::default()
    };
    if p.flash_version != 8 {
        let v = p.flash_version;
        ctx.info(
            origin.clone(),
            format!("published as SWF 8 (the document asks for Flash Player {v})"),
        );
    }
    if p.actionscript_version != 2 {
        let v = p.actionscript_version;
        ctx.warn(
            origin.clone(),
            format!("published as ActionScript 2 (the document asks for ActionScript {v})"),
        );
    }
    if p.omit_trace {
        ctx.info(
            origin,
            "\"Omit trace actions\" is not supported; trace() calls are kept",
        );
    }
}

/// Builds every symbol exported for ActionScript (they must exist in frame
/// 1 even when no instance places them).
fn exported_symbols(ctx: &mut Ctx<'_>) -> Vec<CharacterId> {
    let doc = ctx.doc;
    let mut out = Vec::new();
    for (name, sym) in &doc.library.symbols {
        let Some(link) = sym.linkage.as_ref() else {
            continue;
        };
        if !link.export_for_as || link.identifier.is_empty() {
            continue;
        }
        let origin = Scope::new(name.clone()).origin(None, None);
        let id = match sym.kind {
            SymbolKind::MovieClip | SymbolKind::Graphic => symbol::sprite_char(ctx, name, &origin),
            SymbolKind::Button => {
                let inst = jazari_xfl::model::SymbolInstance {
                    library_item: name.clone(),
                    kind: SymbolKind::Button,
                    ..Default::default()
                };
                symbol::button_char(ctx, &inst, None, None, &origin)
            }
        };
        out.extend(id);
    }
    out
}

/// The AS2 classes named by symbol linkage.
fn linked_classes(ctx: &Ctx<'_>) -> Vec<String> {
    ctx.doc
        .library
        .symbols
        .values()
        .filter_map(|s| s.linkage.as_ref())
        .filter(|l| l.export_for_as)
        .filter_map(|l| l.class_name.clone())
        .filter(|c| !c.trim().is_empty())
        .collect()
}
