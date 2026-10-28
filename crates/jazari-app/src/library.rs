//! The library as a tree of folders and items, for the Library panel.

use jazari_xfl::model::{Document, SymbolKind};

/// The kind of a library entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibKind {
    /// A folder.
    Folder,
    /// A movie clip symbol.
    MovieClip,
    /// A graphic symbol.
    Graphic,
    /// A button symbol.
    Button,
    /// A bitmap.
    Bitmap,
    /// A font item.
    Font,
}

impl LibKind {
    /// The kind of a symbol.
    pub fn of_symbol(kind: SymbolKind) -> Self {
        match kind {
            SymbolKind::MovieClip => LibKind::MovieClip,
            SymbolKind::Graphic => LibKind::Graphic,
            SymbolKind::Button => LibKind::Button,
        }
    }

    /// Whether the entry is a symbol with its own timeline.
    pub fn is_symbol(self) -> bool {
        matches!(
            self,
            LibKind::MovieClip | LibKind::Graphic | LibKind::Button
        )
    }
}

/// A node of the library tree.
#[derive(Debug, Clone, PartialEq)]
pub struct LibNode {
    /// The last path segment.
    pub name: String,
    /// The full library path (the key in the document's library).
    pub path: String,
    /// What it is.
    pub kind: LibKind,
    /// Folder contents: folders first, then items, each by name.
    pub children: Vec<LibNode>,
}

/// The library tree of `doc`. Folders that items imply but the document
/// does not list are created; empty path segments are ignored.
pub fn build_tree(doc: &Document) -> Vec<LibNode> {
    let lib = &doc.library;
    let mut roots: Vec<LibNode> = Vec::new();
    for folder in &lib.folders {
        insert(&mut roots, folder, LibKind::Folder);
    }
    for (name, symbol) in &lib.symbols {
        insert(&mut roots, name, LibKind::of_symbol(symbol.kind));
    }
    for name in lib.bitmaps.keys() {
        insert(&mut roots, name, LibKind::Bitmap);
    }
    for name in lib.fonts.keys() {
        insert(&mut roots, name, LibKind::Font);
    }
    sort(&mut roots);
    roots
}

/// Number of items (not folders) in a tree.
pub fn item_count(nodes: &[LibNode]) -> usize {
    nodes
        .iter()
        .map(|n| usize::from(n.kind != LibKind::Folder) + item_count(&n.children))
        .sum()
}

fn insert(roots: &mut Vec<LibNode>, path: &str, kind: LibKind) {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let Some((last, parents)) = segments.split_last() else {
        return;
    };
    let mut level = roots;
    let mut prefix = String::new();
    for seg in parents {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(seg);
        let idx = match level
            .iter()
            .position(|n| n.kind == LibKind::Folder && n.name == *seg)
        {
            Some(i) => i,
            None => {
                level.push(LibNode {
                    name: (*seg).to_owned(),
                    path: prefix.clone(),
                    kind: LibKind::Folder,
                    children: Vec::new(),
                });
                level.len() - 1
            }
        };
        level = &mut level[idx].children;
    }
    let exists = level.iter().any(|n| n.kind == kind && n.name == *last);
    if !exists {
        level.push(LibNode {
            name: (*last).to_owned(),
            path: path.to_owned(),
            kind,
            children: Vec::new(),
        });
    }
}

fn sort(nodes: &mut [LibNode]) {
    nodes.sort_by(|a, b| {
        (a.kind != LibKind::Folder)
            .cmp(&(b.kind != LibKind::Folder))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    for n in nodes {
        sort(&mut n.children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jazari_xfl::model::{BitmapItem, Symbol};

    #[test]
    fn tree_with_folders_and_implicit_folders() {
        let mut doc = Document::default();
        doc.library.folders = vec!["art".into(), "art/old".into(), "empty".into()];
        for (name, kind) in [
            ("art/Ball", SymbolKind::MovieClip),
            ("art/old/Star", SymbolKind::Graphic),
            ("ui/buttons/Play", SymbolKind::Button),
            ("Root", SymbolKind::MovieClip),
            ("زر", SymbolKind::Button),
        ] {
            doc.library.symbols.insert(
                name.into(),
                Symbol {
                    name: name.into(),
                    kind,
                    ..Symbol::default()
                },
            );
        }
        doc.library.bitmaps.insert(
            "art/photo.jpg".into(),
            BitmapItem {
                name: "art/photo.jpg".into(),
                ..BitmapItem::default()
            },
        );
        let tree = build_tree(&doc);
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["art", "empty", "ui", "Root", "زر"]);
        let art = &tree[0];
        let art_names: Vec<(&str, LibKind)> = art
            .children
            .iter()
            .map(|n| (n.name.as_str(), n.kind))
            .collect();
        assert_eq!(
            art_names,
            [
                ("old", LibKind::Folder),
                ("Ball", LibKind::MovieClip),
                ("photo.jpg", LibKind::Bitmap)
            ]
        );
        assert_eq!(art.children[0].children[0].path, "art/old/Star");
        let ui = &tree[2];
        assert_eq!(ui.children[0].path, "ui/buttons");
        assert_eq!(ui.children[0].children[0].kind, LibKind::Button);
        assert_eq!(item_count(&tree), 6);
    }

    #[test]
    fn odd_paths_do_not_break_the_tree() {
        let mut doc = Document::default();
        doc.library.folders = vec!["".into(), "/".into(), "a//b/".into()];
        doc.library
            .symbols
            .insert("a//b/c".into(), Symbol::default());
        let tree = build_tree(&doc);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children[0].name, "b");
        assert_eq!(tree[0].children[0].children[0].path, "a//b/c");
    }
}
