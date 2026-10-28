//! ActionScript through `jazari-as2`: frame scripts, instance handlers,
//! linkage class registration, AS2 class modules and the stage hook.
//!
//! A script that fails to compile is skipped: its diagnostics (errors,
//! located by the origin passed in) are added and the publish continues.

use std::rc::Rc;

use jazari_as2::{ButtonHandler, ClipHandler, Origin, SceneContext};
use swf::{ClipEventFlag, ExportedAsset, Tag};

use crate::context::{CharKind, Ctx};
use crate::display::push;
use crate::movie;
use crate::tags::{self, RawTag};

/// `stop();` as a DoAction payload.
pub const STOP: [u8; 2] = [0x07, 0x00];

/// The scene context for scripts of scene `scene`: every scene's name and
/// first root frame, in order (scenes are concatenated, see `movie.rs`).
pub fn scene_context(ctx: &Ctx<'_>, scene: usize) -> SceneContext {
    let mut first = 0u32;
    let scenes = ctx
        .doc
        .scenes
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let entry = (movie::scene_name(s, i), first);
            first = first.saturating_add(s.frame_count());
            entry
        })
        .collect();
    SceneContext {
        scenes,
        current: scene,
    }
}

/// Compiles a frame script; `scene` is the scene index for a scene timeline
/// (scene gotos then resolve at compile time). `None` when it has no code
/// or fails (reported).
pub fn frame_actions(
    ctx: &mut Ctx<'_>,
    src: &str,
    origin: &Origin,
    scene: Option<usize>,
) -> Option<Vec<u8>> {
    let scenes = scene.map(|s| scene_context(ctx, s));
    match ctx.as2.compile_frame_in(src, origin, scenes.as_ref()) {
        Ok(bytes) if bytes.is_empty() => None,
        Ok(bytes) => Some(bytes),
        Err(diags) => {
            ctx.extend(diags);
            None
        }
    }
}

/// Compiles the `onClipEvent`/`on` handlers of a movie clip instance.
/// `None` when there are none or the instance fails (reported).
pub fn clip_handlers(
    ctx: &mut Ctx<'_>,
    src: &str,
    origin: &Origin,
) -> Option<Rc<Vec<ClipHandler>>> {
    match ctx.as2.compile_clip_handlers(src, origin) {
        Ok(handlers) => {
            let mut ok = Vec::with_capacity(handlers.len());
            for h in handlers {
                let key_press = h.events.contains(ClipEventFlag::KEY_PRESS);
                if key_press && h.key_code.is_none() {
                    ctx.error(
                        origin.clone(),
                        "an on(keyPress) handler has no key code; it is left out",
                    );
                    continue;
                }
                ok.push(h);
            }
            (!ok.is_empty()).then(|| Rc::new(ok))
        }
        Err(diags) => {
            ctx.extend(diags);
            None
        }
    }
}

/// Compiles the `on` handlers of a button instance; `scene` is the scene
/// index when the button sits on a scene timeline. `None` when the instance
/// fails (reported).
pub fn button_handlers(
    ctx: &mut Ctx<'_>,
    src: &str,
    scene: Option<usize>,
    origin: &Origin,
) -> Option<Vec<ButtonHandler>> {
    let scenes = scene.map(|s| scene_context(ctx, s));
    match ctx
        .as2
        .compile_button_handlers_in(src, origin, scenes.as_ref())
    {
        Ok(handlers) => Some(handlers),
        Err(diags) => {
            ctx.extend(diags);
            None
        }
    }
}

/// `Object.registerClass("<identifier>", <class>);` for a linked symbol.
pub fn register_class(
    ctx: &mut Ctx<'_>,
    identifier: &str,
    class: &str,
    origin: &Origin,
) -> Option<Vec<u8>> {
    let ok_class = !class.is_empty()
        && class.split('.').all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        });
    if !ok_class {
        ctx.error(
            origin.clone(),
            format!("'{class}' is not a valid AS2 class name"),
        );
        return None;
    }
    let id = identifier.replace('\\', "\\\\").replace('"', "\\\"");
    let src = format!("Object.registerClass(\"{id}\", {class});\n");
    frame_actions(ctx, &src, origin, None)
}

/// The stage build's frame 1 script (`stop()` plus the `jz_goto` callback).
pub fn stage_hook(ctx: &mut Ctx<'_>) -> Option<Vec<u8>> {
    match jazari_as2::stage_goto_hook() {
        Ok(bytes) => Some(bytes),
        Err(diags) => {
            ctx.extend(diags);
            None
        }
    }
}

/// Compiles the AS2 classes the scripts referenced plus `linked` (linkage
/// classes) into `__Packages` modules: for each, an empty DefineSprite, its
/// ExportAssets and its DoInitAction, in the order the modules must run.
/// Best effort: failures are reported and leave the classes out.
pub fn class_tags(ctx: &mut Ctx<'_>, linked: &[String]) -> Vec<RawTag> {
    let mut names = ctx.as2.referenced_classes();
    names.extend(linked.iter().cloned());
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Vec::new();
    }
    let modules = match ctx.as2.compile_classes(&names) {
        Ok(m) => m,
        Err(diags) => {
            ctx.extend(diags);
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for m in modules {
        let origin = Origin {
            file: Some(format!("class {}", m.name)),
            ..Origin::default()
        };
        let Some(id) = ctx.reserve_id(&origin) else {
            break;
        };
        let mut defs = vec![tags::sprite(id, 0, &[])];
        let export = format!("__Packages.{}", m.name);
        push(
            ctx,
            &mut defs,
            &origin,
            &Tag::ExportAssets(vec![ExportedAsset {
                id,
                name: export.as_str().into(),
            }]),
        );
        push(
            ctx,
            &mut defs,
            &origin,
            &Tag::DoInitAction {
                id,
                action_data: &m.actions,
            },
        );
        ctx.set_definition(id, CharKind::Sprite, defs, Vec::new());
        ctx.emit_definition(id, &mut out);
    }
    out
}
