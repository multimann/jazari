//! The document scan behind the shared fonts.
//!
//! A DefineFont3 is written when its first text is placed, but it must hold
//! the glyphs of every text that uses it. So before the first static text is
//! defined, every timeline the movie can show (the scenes, and the symbols
//! they reach or that are exported for ActionScript) is walked once, each
//! static text on a published layer is laid out, and its glyphs are noted per
//! face in first-use order. The layouts are cached, so each text is laid out
//! once. A glyph the scan missed still works: it gets a font of its own
//! (see `font`).

use std::collections::HashSet;
use std::rc::Rc;

use jazari_text::{TextBlock, TextLayout};
use jazari_xfl::Document;
use jazari_xfl::model::{Element, LayerKind, TextKind, Timeline};

use crate::context::{Ctx, MAX_NESTING};

/// A cache key for a text block (everything layout depends on).
pub fn block_key(block: &TextBlock) -> String {
    format!("{block:?}")
}

/// The layout of a block, from the cache or freshly made.
pub fn layout(ctx: &mut Ctx<'_>, block: &TextBlock) -> Rc<TextLayout> {
    let key = block_key(block);
    if let Some(l) = ctx.text.layouts.get(&key) {
        return l.clone();
    }
    let l = Rc::new(jazari_text::layout(ctx.fonts, block));
    ctx.text.layouts.insert(key, l.clone());
    l
}

/// Scans the document once (see the module docs).
pub fn ensure(ctx: &mut Ctx<'_>) {
    if ctx.text.scanned {
        return;
    }
    ctx.text.scanned = true;
    for el in static_texts(ctx.doc) {
        let block = TextBlock::from_element(el);
        let l = layout(ctx, &block);
        for g in l.glyphs() {
            ctx.text.fonts.note(g.font, g.glyph);
        }
    }
}

/// Every static text on a published layer of a reachable timeline, in
/// document order: scenes first, then the symbols in the order they are
/// reached.
fn static_texts(doc: &Document) -> Vec<&jazari_xfl::model::TextElement> {
    let mut out = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut queue: Vec<&Timeline> = doc.scenes.iter().collect();
    for (name, sym) in &doc.library.symbols {
        if sym.linkage.as_ref().is_some_and(|l| l.export_for_as) && seen.insert(name.as_str()) {
            queue.push(&sym.timeline);
        }
    }
    let mut next = 0;
    while let Some(&tl) = queue.get(next) {
        next += 1;
        for layer in &tl.layers {
            let published = matches!(layer.kind, LayerKind::Normal | LayerKind::Mask)
                && (layer.visible || doc.publish.publish_hidden_layers);
            if !published {
                continue;
            }
            for frame in &layer.frames {
                // Groups are walked iteratively, as deep as the publisher
                // follows them.
                let mut stack: Vec<(&Element, usize)> =
                    frame.elements.iter().rev().map(|e| (e, 0)).collect();
                while let Some((el, depth)) = stack.pop() {
                    match el {
                        Element::Text(t) if t.kind == TextKind::Static => out.push(t),
                        Element::Group(g) if depth < MAX_NESTING => {
                            stack.extend(g.members.iter().rev().map(|e| (e, depth + 1)));
                        }
                        Element::Instance(i) => {
                            let found = doc.library.symbols.get_key_value(&i.library_item);
                            if let Some((name, sym)) =
                                found.filter(|(n, _)| !seen.contains(n.as_str()))
                            {
                                seen.insert(name.as_str());
                                queue.push(&sym.timeline);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    out
}
