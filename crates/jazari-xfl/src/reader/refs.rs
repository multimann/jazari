//! Checks, once everything is read, that every symbol and bitmap an element
//! names exists in the library.

use std::collections::HashMap;

use crate::model::{Document, Element, FillStyle, Library, Timeline};
use crate::{LayerRef, Location, TimelineRef, WarningKind};

use super::ctx::Ctx;

/// Records a [`WarningKind::MissingItem`] for every dangling reference.
/// `symbol_files` maps symbol names to the file they were read from.
pub(crate) fn check_references(
    ctx: &mut Ctx,
    doc: &Document,
    doc_file: &str,
    symbol_files: &HashMap<String, String>,
) {
    for (index, scene) in doc.scenes.iter().enumerate() {
        ctx.loc = Location {
            file: Some(doc_file.to_owned()),
            timeline: Some(TimelineRef::Scene {
                index,
                name: scene.name.clone(),
            }),
            ..Location::default()
        };
        check_timeline(ctx, &doc.library, scene);
    }
    for (name, symbol) in &doc.library.symbols {
        ctx.loc = Location {
            file: symbol_files.get(name).cloned(),
            timeline: Some(TimelineRef::Symbol(name.clone())),
            ..Location::default()
        };
        check_timeline(ctx, &doc.library, &symbol.timeline);
    }
    check_symbol_cycles(ctx, &doc.library, symbol_files);
    ctx.loc = Location::default();
}

/// Warns about symbols that contain themselves, directly or through other
/// symbols. The Flash IDE does not allow this, so such a file is damaged;
/// anything that expands symbols recursively must stop there.
fn check_symbol_cycles(ctx: &mut Ctx, library: &Library, symbol_files: &HashMap<String, String>) {
    // Symbol index -> indices of the symbols its timeline places.
    let edges: Vec<Vec<usize>> = library
        .symbols
        .values()
        .map(|symbol| {
            let mut out = Vec::new();
            for layer in &symbol.timeline.layers {
                for frame in &layer.frames {
                    collect_instances(library, &frame.elements, &mut out);
                }
            }
            out.sort_unstable();
            out.dedup();
            out
        })
        .collect();

    // Iterative depth-first search; 0 = unvisited, 1 = on the stack, 2 = done.
    let mut state = vec![0u8; edges.len()];
    for root in 0..edges.len() {
        if state[root] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        state[root] = 1;
        while let Some(top) = stack.last_mut() {
            let node = top.0;
            let Some(&child) = edges[node].get(top.1) else {
                state[node] = 2;
                stack.pop();
                continue;
            };
            top.1 += 1;
            match state[child] {
                0 => {
                    state[child] = 1;
                    stack.push((child, 0));
                }
                1 => {
                    // The loop runs from `child` down the stack to `node`.
                    let start = stack.iter().position(|&(n, _)| n == child).unwrap_or(0);
                    let names: Vec<&str> = stack[start..]
                        .iter()
                        .chain(std::iter::once(&(child, 0)))
                        .filter_map(|&(n, _)| library.symbols.get_index(n).map(|(k, _)| k.as_str()))
                        .collect();
                    let Some((name, _)) = library.symbols.get_index(child) else {
                        continue;
                    };
                    ctx.loc = Location {
                        file: symbol_files.get(name).cloned(),
                        timeline: Some(TimelineRef::Symbol(name.clone())),
                        ..Location::default()
                    };
                    ctx.warn(
                        WarningKind::BadValue,
                        format!(
                            "the symbol contains itself ({}); it cannot be published",
                            names.join(" → ")
                        ),
                    );
                }
                _ => {}
            }
        }
    }
}

/// Library indices of the symbols placed by `elements`, groups included.
fn collect_instances(library: &Library, elements: &[Element], out: &mut Vec<usize>) {
    for element in elements {
        match element {
            Element::Instance(inst) => {
                if let Some(i) = library.symbols.get_index_of(&inst.library_item) {
                    out.push(i);
                }
            }
            Element::Group(group) => collect_instances(library, &group.members, out),
            _ => {}
        }
    }
}

fn check_timeline(ctx: &mut Ctx, library: &Library, timeline: &Timeline) {
    for (index, layer) in timeline.layers.iter().enumerate() {
        ctx.loc.layer = Some(LayerRef {
            index,
            name: layer.name.clone(),
        });
        for frame in &layer.frames {
            ctx.loc.frame = Some(frame.index);
            for (i, element) in frame.elements.iter().enumerate() {
                ctx.with_element(i, |ctx| check_element(ctx, library, element));
            }
        }
    }
    ctx.loc.layer = None;
    ctx.loc.frame = None;
}

fn check_element(ctx: &mut Ctx, library: &Library, element: &Element) {
    match element {
        Element::Instance(inst) => {
            if !inst.library_item.is_empty() && !library.symbols.contains_key(&inst.library_item) {
                ctx.warn(
                    WarningKind::MissingItem,
                    format!("the symbol '{}' is not in the library", inst.library_item),
                );
            }
        }
        Element::Bitmap(b) => check_bitmap(ctx, library, &b.library_item),
        Element::Shape(shape) => {
            let fills = shape
                .fills
                .iter()
                .chain(shape.strokes.iter().map(|s| &s.fill));
            for fill in fills {
                if let FillStyle::Bitmap(bf) = fill {
                    check_bitmap(ctx, library, &bf.library_item);
                }
            }
        }
        Element::Group(group) => {
            for (i, member) in group.members.iter().enumerate() {
                ctx.with_element(i, |ctx| check_element(ctx, library, member));
            }
        }
        Element::Text(_) | Element::Unsupported { .. } => {}
    }
}

fn check_bitmap(ctx: &mut Ctx, library: &Library, name: &str) {
    if !name.is_empty() && !library.bitmaps.contains_key(name) {
        ctx.warn(
            WarningKind::MissingItem,
            format!("the bitmap '{name}' is not in the library"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Frame, Group, Layer, Symbol, SymbolInstance, SymbolKind};

    fn instance(name: &str) -> Element {
        Element::Instance(SymbolInstance {
            library_item: name.to_owned(),
            ..SymbolInstance::default()
        })
    }

    fn symbol(name: &str, elements: Vec<Element>) -> Symbol {
        let mut timeline = Timeline::new(name);
        timeline.layers.push(Layer {
            frames: vec![Frame {
                elements,
                ..Frame::default()
            }],
            ..Layer::default()
        });
        Symbol {
            name: name.to_owned(),
            kind: SymbolKind::MovieClip,
            timeline,
            linkage: None,
            scale9: None,
        }
    }

    #[test]
    fn symbols_that_contain_themselves_are_reported() {
        let mut doc = Document::default();
        let group = Element::Group(Group {
            matrix: None,
            members: vec![instance("A")],
        });
        for s in [
            symbol("A", vec![instance("B")]),
            symbol("B", vec![group]),
            symbol("Self", vec![instance("Self")]),
            symbol("Fine", vec![instance("A"), instance("Missing")]),
        ] {
            doc.library.symbols.insert(s.name.clone(), s);
        }
        let mut ctx = Ctx::default();
        check_references(&mut ctx, &doc, "DOMDocument.xml", &HashMap::new());
        let loops: Vec<&str> = ctx
            .report
            .iter()
            .filter(|w| w.kind == WarningKind::BadValue)
            .map(|w| w.message.as_str())
            .collect();
        assert_eq!(
            loops,
            [
                "the symbol contains itself (A → B → A); it cannot be published",
                "the symbol contains itself (Self → Self); it cannot be published",
            ]
        );
        let missing = ctx
            .report
            .iter()
            .filter(|w| w.kind == WarningKind::MissingItem)
            .count();
        assert_eq!(missing, 1, "the dangling 'Missing'");
        assert_eq!(
            ctx.report.warnings[0].location.timeline,
            Some(TimelineRef::Symbol("Fine".to_owned()))
        );
    }
}
