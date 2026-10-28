//! `DOMDocument.xml` and the library: folders, fonts, bitmaps (with their
//! `bin/` data) and the symbol files under `LIBRARY/`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::model::{
    BitmapData, BitmapItem, Document, FontItem, Linkage, Rect, Rgba, SourceInfo, Symbol,
    SymbolKind, Timeline,
};
use crate::{Error, ImportReport, Location, Result, TimelineRef, WarningKind};

use super::ctx::Ctx;
use super::dat::{Decoded, decode_media};
use super::element::symbol_kind;
use super::publish::read_publish_settings;
use super::refs::check_references;
use super::store::{Store, normalize};
use super::timeline::read_timeline;
use super::values::{excerpt, parse_f64, parse_u32, parse_version};
use super::xml::{self, Node};

/// Children of `DOMDocument`. Those after `timelines` are editor state and
/// are skipped without a warning.
const DOCUMENT_CHILDREN: [&str; 13] = [
    "folders",
    "fonts",
    "media",
    "symbols",
    "timelines",
    "swatchLists",
    "extendedSwatchLists",
    "persistentData",
    "scripts",
    "PrinterSettings",
    "publishHistory",
    "swcCache",
    "classDefinitions",
];

/// Newest XFL version written by Flash CS6.
const CS6_XFL_VERSION: f64 = 2.2;

/// Reads a whole document from `store`. `container` is the folder or archive,
/// for error messages.
pub(crate) fn read_document(
    store: &mut Store,
    container: &Path,
    source: SourceInfo,
) -> Result<(Document, ImportReport)> {
    let doc_file = store
        .find("DOMDocument.xml")
        .map(str::to_owned)
        .ok_or_else(|| Error::MissingDocument {
            path: container.to_owned(),
        })?;
    let bytes = store.read(&doc_file).map_err(|source| Error::Io {
        path: container.join(&doc_file),
        source,
    })?;
    let root = xml::parse(&bytes).map_err(|e| Error::Xml {
        file: doc_file.clone(),
        position: e.position,
        message: e.message,
    })?;
    if root.name != "DOMDocument" {
        return Err(Error::Xml {
            file: doc_file,
            position: 0,
            message: format!("the root element is <{}>, not <DOMDocument>", root.name),
        });
    }

    let mut ctx = Ctx {
        loc: Location::file(&doc_file),
        ..Ctx::default()
    };
    let mut doc = Document {
        source,
        ..Document::default()
    };
    read_document_attributes(&mut ctx, &root, &mut doc);
    ctx.check_children(&root, &DOCUMENT_CHILDREN);

    if let Some(folders) = root.child("folders") {
        for f in &folders.children {
            match (f.name.as_str(), f.attr("name")) {
                ("DOMFolderItem", Some(name)) if !name.is_empty() => {
                    doc.library.folders.push(name.to_owned());
                }
                ("DOMFolderItem", _) => {
                    ctx.warn(WarningKind::BadValue, "a library folder has no name");
                }
                (other, _) => ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown <{other}> inside <folders> was skipped"),
                ),
            }
        }
    }
    if let Some(fonts) = root.child("fonts") {
        for f in &fonts.children {
            if f.name != "DOMFontItem" {
                ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown <{}> inside <fonts> was skipped", f.name),
                );
                continue;
            }
            if let Some(font) = read_font_item(&mut ctx, f) {
                if doc.library.fonts.contains_key(&font.name) {
                    ctx.warn(
                        WarningKind::BadValue,
                        format!(
                            "two font items are named '{}'; the first is kept",
                            font.name
                        ),
                    );
                } else {
                    doc.library.fonts.insert(font.name.clone(), font);
                }
            }
        }
    }
    if let Some(media) = root.child("media") {
        read_media(&mut ctx, store, media, &mut doc);
    }

    let mut symbol_files = HashMap::new();
    read_symbols(
        &mut ctx,
        store,
        &root,
        &doc_file,
        &mut doc,
        &mut symbol_files,
    );

    ctx.loc = Location::file(&doc_file);
    if let Some(timelines) = root.child("timelines") {
        for t in &timelines.children {
            if t.name != "DOMTimeline" {
                ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown <{}> inside <timelines> was skipped", t.name),
                );
                continue;
            }
            ctx.loc.timeline = Some(TimelineRef::Scene {
                index: doc.scenes.len(),
                name: t.attr("name").unwrap_or_default().to_owned(),
            });
            let scene = read_timeline(&mut ctx, t);
            doc.scenes.push(scene);
            ctx.loc.timeline = None;
        }
    }
    if doc.scenes.is_empty() {
        ctx.warn(
            WarningKind::BadValue,
            "the document has no timeline; an empty scene is used",
        );
        doc.scenes.push(Timeline::new("Scene 1"));
    }

    if let Some(path) = store.find("PublishSettings.xml").map(str::to_owned) {
        ctx.loc = Location::file(&path);
        match store.read(&path) {
            Ok(bytes) => match xml::parse(&bytes) {
                Ok(settings) => doc.publish = read_publish_settings(&mut ctx, &settings),
                Err(e) => ctx.warn(
                    WarningKind::Other,
                    format!(
                        "malformed XML at byte {}: {}; the default publish settings are used",
                        e.position, e.message
                    ),
                ),
            },
            Err(e) => ctx.warn(
                WarningKind::Other,
                format!("cannot read the file ({e}); the default publish settings are used"),
            ),
        }
    }

    check_references(&mut ctx, &doc, &doc_file, &symbol_files);
    Ok((doc, ctx.report))
}

/// Stage size, frame rate, background and version.
fn read_document_attributes(ctx: &mut Ctx, root: &Node, doc: &mut Document) {
    let positive = |ctx: &mut Ctx, name: &str, default: f64| {
        let v = ctx.f64(root, name, default);
        if v > 0.0 {
            v
        } else {
            ctx.warn(
                WarningKind::BadValue,
                format!("<DOMDocument> {name}=\"{v}\" is not positive; {default} is used"),
            );
            default
        }
    };
    doc.width = positive(ctx, "width", 550.0);
    doc.height = positive(ctx, "height", 400.0);
    doc.frame_rate = positive(ctx, "frameRate", 24.0);
    doc.background = ctx.color(root, "backgroundColor", Rgba::WHITE);
    doc.xfl_version = root.attr("xflVersion").unwrap_or_default().to_owned();
    match parse_version(&doc.xfl_version) {
        Some(v) if v > CS6_XFL_VERSION => ctx.warn(
            WarningKind::AnimateFeature,
            format!(
                "xflVersion {} means the file was saved by Flash CC or Animate; \
                 features newer than CS6 are ignored",
                doc.xfl_version
            ),
        ),
        Some(_) => {}
        None => ctx.warn(
            WarningKind::BadValue,
            format!(
                "<DOMDocument> xflVersion=\"{}\" is missing or not a number",
                excerpt(&doc.xfl_version)
            ),
        ),
    }
}

/// Linkage attributes of a library item, or `None` when it is not exported.
fn read_linkage(ctx: &mut Ctx, node: &Node) -> Option<Linkage> {
    if ctx.bool(node, "linkageImportForRS", false) || ctx.bool(node, "linkageExportForRS", false) {
        ctx.warn(
            WarningKind::Unsupported,
            "runtime sharing of library items is not supported",
        );
    }
    let export_for_as = ctx.bool(node, "linkageExportForAS", false);
    let identifier = Ctx::text(node, "linkageIdentifier");
    let class_name = Ctx::text(node, "linkageClassName");
    if !export_for_as && identifier.is_none() && class_name.is_none() {
        return None;
    }
    Some(Linkage {
        identifier: identifier.unwrap_or_default(),
        export_for_as,
        export_in_first_frame: ctx.bool(node, "linkageExportInFirstFrame", true),
        class_name,
    })
}

/// Reads a `DOMFontItem`.
fn read_font_item(ctx: &mut Ctx, node: &Node) -> Option<FontItem> {
    let Some(name) = Ctx::text(node, "name") else {
        ctx.warn(
            WarningKind::BadValue,
            "a font item has no name; it is skipped",
        );
        return None;
    };
    let mut embed_ranges = Vec::new();
    if let Some(ranges) = node.attr("embedRanges") {
        for token in ranges.split('|').map(str::trim).filter(|t| !t.is_empty()) {
            match parse_u32(token) {
                Some(r) => embed_ranges.push(r),
                None => ctx.warn(
                    WarningKind::BadValue,
                    format!(
                        "font '{name}': embed range \"{}\" is not a number",
                        excerpt(token)
                    ),
                ),
            }
        }
    }
    Some(FontItem {
        face: node.attr("font").unwrap_or_default().to_owned(),
        size: ctx.f64(node, "size", 0.0),
        embed_ranges,
        embedded_characters: node
            .attr("embeddedCharacters")
            .unwrap_or_default()
            .to_owned(),
        linkage: read_linkage(ctx, node),
        name,
    })
}

/// Reads `<media>`: bitmaps are loaded; sounds, video and compiled clips
/// are reported.
fn read_media(ctx: &mut Ctx, store: &mut Store, media: &Node, doc: &mut Document) {
    for item in &media.children {
        match item.name.as_str() {
            "DOMBitmapItem" => {
                let Some(bitmap) = read_bitmap_item(ctx, store, item) else {
                    continue;
                };
                if doc.library.bitmaps.contains_key(&bitmap.name) {
                    ctx.warn(
                        WarningKind::BadValue,
                        format!("two bitmaps are named '{}'; the first is kept", bitmap.name),
                    );
                } else {
                    doc.library.bitmaps.insert(bitmap.name.clone(), bitmap);
                }
            }
            "DOMSoundItem" | "DOMVideoItem" | "DOMCompiledClipItem" => {
                let what = match item.name.as_str() {
                    "DOMSoundItem" => "sound",
                    "DOMVideoItem" => "video",
                    _ => "compiled clip",
                };
                ctx.warn(
                    WarningKind::Unsupported,
                    format!(
                        "the {what} '{}' is not supported and is left out",
                        item.attr("name").unwrap_or_default()
                    ),
                );
            }
            other => ctx.warn(
                WarningKind::UnknownElement,
                format!("unknown <{other}> inside <media> was skipped"),
            ),
        }
    }
}

/// Reads a `DOMBitmapItem` and decodes its data from `bin/` (or, failing
/// that, the original file under `LIBRARY/`).
fn read_bitmap_item(ctx: &mut Ctx, store: &mut Store, node: &Node) -> Option<BitmapItem> {
    let Some(name) = Ctx::text(node, "name") else {
        ctx.warn(
            WarningKind::BadValue,
            "a bitmap item has no name; it is skipped",
        );
        return None;
    };
    let twips_w = ctx.f64(node, "frameRight", 0.0) - ctx.f64(node, "frameLeft", 0.0);
    let twips_h = ctx.f64(node, "frameBottom", 0.0) - ctx.f64(node, "frameTop", 0.0);
    let declared = (px_from_twips(twips_w), px_from_twips(twips_h));

    let mut candidates = Vec::new();
    if let Some(data) = Ctx::text(node, "bitmapDataHRef") {
        candidates.push(format!("bin/{}", normalize(&data)));
    }
    if let Some(href) = Ctx::text(node, "href") {
        candidates.push(format!("LIBRARY/{}", normalize(&href)));
    }
    let mut decoded = None;
    for wanted in &candidates {
        let Some(path) = store
            .find(wanted)
            .or_else(|| store.find_by_basename(folder_of(wanted), wanted))
            .map(str::to_owned)
        else {
            continue;
        };
        let result = store
            .read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| decode_media(&bytes));
        match result {
            Ok(d) => {
                decoded = Some(d);
                break;
            }
            Err(e) => ctx.warn(
                WarningKind::BadMedia,
                format!("bitmap '{name}': {path}: {e}"),
            ),
        }
    }

    let (width, height, data) = match decoded {
        Some(Decoded::Rgba {
            width,
            height,
            premultiplied,
            pixels,
        }) => (
            width,
            height,
            BitmapData::Rgba {
                premultiplied,
                pixels,
            },
        ),
        Some(Decoded::Jpeg {
            width,
            height,
            bytes,
        }) => (
            width.unwrap_or(declared.0),
            height.unwrap_or(declared.1),
            BitmapData::Jpeg { bytes, alpha: None },
        ),
        None => {
            ctx.warn(
                WarningKind::MissingItem,
                format!(
                    "the data of bitmap '{name}' was not found (looked for {}); it is left empty",
                    if candidates.is_empty() {
                        "no file name".to_owned()
                    } else {
                        candidates.join(", ")
                    }
                ),
            );
            (declared.0, declared.1, BitmapData::default())
        }
    };
    if declared != (0, 0) && declared != (width, height) {
        ctx.warn(
            WarningKind::BadMedia,
            format!(
                "bitmap '{name}' is {width} × {height} px, but the item says {} × {}",
                declared.0, declared.1
            ),
        );
    }
    Some(BitmapItem {
        smoothing: ctx.bool(node, "allowSmoothing", false),
        linkage: read_linkage(ctx, node),
        name,
        width,
        height,
        data,
    })
}

/// Twips to whole pixels (frame bounds are whole twips in practice).
fn px_from_twips(twips: f64) -> u32 {
    let px = (twips / 20.0).round();
    if px.is_finite() && px > 0.0 {
        // Clamped below u32::MAX by the comparison, so the cast is exact.
        px.min(f64::from(u32::MAX)) as u32
    } else {
        0
    }
}

/// The folder part of a store path, with a trailing `/` (`"bin/"`).
fn folder_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "",
    }
}

/// Reads every library symbol: the files named by `<symbols><Include>` in
/// order, then any other `LIBRARY/**/*.xml` (keyed by the name inside).
fn read_symbols(
    ctx: &mut Ctx,
    store: &mut Store,
    root: &Node,
    doc_file: &str,
    doc: &mut Document,
    symbol_files: &mut HashMap<String, String>,
) {
    ctx.loc = Location::file(doc_file);
    let mut includes = Vec::new();
    if let Some(symbols) = root.child("symbols") {
        for inc in &symbols.children {
            match (inc.name.as_str(), inc.attr("href")) {
                ("Include", Some(href)) if !href.is_empty() => includes.push(href.to_owned()),
                ("Include", _) => ctx.warn(WarningKind::BadValue, "an <Include> has no href"),
                (other, _) => ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown <{other}> inside <symbols> was skipped"),
                ),
            }
        }
    }

    let mut done: HashSet<String> = HashSet::new();
    for href in includes {
        let wanted = format!("LIBRARY/{}", normalize(&href));
        let found = store
            .find(&wanted)
            .or_else(|| store.find_by_basename("LIBRARY/", &wanted))
            .map(str::to_owned);
        match found {
            Some(path) => {
                if done.insert(path.clone()) {
                    read_symbol_file(ctx, store, &path, true, doc, symbol_files);
                }
            }
            None => {
                ctx.loc = Location::file(doc_file);
                ctx.warn(
                    WarningKind::MissingItem,
                    format!("the symbol file {wanted} (from <Include>) was not found"),
                );
            }
        }
    }

    let others: Vec<String> = store
        .paths()
        .iter()
        .filter(|p| {
            let lower = p.to_lowercase();
            lower.starts_with("library/") && lower.ends_with(".xml") && !done.contains(*p)
        })
        .cloned()
        .collect();
    for path in others {
        read_symbol_file(ctx, store, &path, false, doc, symbol_files);
    }
}

/// Reads one `LIBRARY/*.xml` file. `listed` says whether DOMDocument names it.
fn read_symbol_file(
    ctx: &mut Ctx,
    store: &mut Store,
    path: &str,
    listed: bool,
    doc: &mut Document,
    symbol_files: &mut HashMap<String, String>,
) {
    ctx.loc = Location::file(path);
    let bytes = match store.read(path) {
        Ok(b) => b,
        Err(e) => {
            ctx.warn(
                WarningKind::MissingItem,
                format!("cannot read the symbol file ({e}); it is skipped"),
            );
            return;
        }
    };
    let root = match xml::parse(&bytes) {
        Ok(r) => r,
        Err(e) => {
            ctx.warn(
                WarningKind::Other,
                format!(
                    "malformed XML at byte {}: {}; the symbol is skipped",
                    e.position, e.message
                ),
            );
            return;
        }
    };
    let is_component = root.name == "DOMComponentItem";
    if root.name != "DOMSymbolItem" && !is_component {
        if listed {
            ctx.warn(
                WarningKind::UnknownElement,
                format!(
                    "the file holds <{}>, not a symbol; it is skipped",
                    root.name
                ),
            );
        }
        return;
    }
    if !listed {
        ctx.warn(
            WarningKind::Other,
            "this symbol file is not listed in DOMDocument.xml; it is loaded anyway",
        );
    }
    let name = match Ctx::text(&root, "name") {
        Some(n) => n,
        None => {
            let stem = path
                .strip_prefix("LIBRARY/")
                .unwrap_or(path)
                .trim_end_matches(".xml")
                .to_owned();
            ctx.warn(
                WarningKind::BadValue,
                format!("the symbol has no name; '{stem}' (from the file name) is used"),
            );
            stem
        }
    };
    if doc.library.symbols.contains_key(&name) {
        ctx.warn(
            WarningKind::BadValue,
            format!("another symbol is already named '{name}'; this one is skipped"),
        );
        return;
    }
    ctx.loc.timeline = Some(TimelineRef::Symbol(name.clone()));
    ctx.check_children(&root, &["timeline", "dataBindingXML", "persistentData"]);

    let mut kind = symbol_kind(ctx, root.attr("symbolType"));
    if is_component {
        ctx.warn(
            WarningKind::Unsupported,
            "a component is read as a plain movie clip; its code is not included",
        );
        kind = SymbolKind::MovieClip;
    }
    let linkage = read_linkage(ctx, &root);
    let scale9 = read_scale9(ctx, &root);
    let timeline = match root.child("timeline").and_then(|t| t.child("DOMTimeline")) {
        Some(t) => read_timeline(ctx, t),
        None => {
            ctx.warn(
                WarningKind::BadValue,
                "the symbol has no timeline; it is empty",
            );
            Timeline::new(name.clone())
        }
    };
    symbol_files.insert(name.clone(), path.to_owned());
    doc.library.symbols.insert(
        name.clone(),
        Symbol {
            name,
            kind,
            timeline,
            linkage,
            scale9,
        },
    );
    ctx.loc.timeline = None;
}

/// The 9-slice grid: `scaleGridLeft/Right/Top/Bottom` in pixels (schema), or
/// `scalingGrid="true"` with `scalingGridRect="l t r b"` in twips.
fn read_scale9(ctx: &mut Ctx, node: &Node) -> Option<Rect> {
    let names = [
        "scaleGridLeft",
        "scaleGridTop",
        "scaleGridRight",
        "scaleGridBottom",
    ];
    if names.iter().any(|n| node.has_attr(n)) {
        let [l, t, r, b] = names.map(|n| ctx.f64(node, n, 0.0));
        return (r > l && b > t).then(|| Rect::new(l, t, r, b));
    }
    if ctx.bool(node, "scalingGrid", false) {
        let values: Option<Vec<f64>> = node
            .attr("scalingGridRect")
            .and_then(|s| s.split_whitespace().map(parse_f64).collect());
        match values.as_deref() {
            Some(&[l, t, r, b]) if r > l && b > t => {
                return Some(Rect::new(l / 20.0, t / 20.0, r / 20.0, b / 20.0));
            }
            _ => ctx.warn(
                WarningKind::BadValue,
                "scalingGridRect is not four numbers (left top right bottom); no 9-slice grid is used",
            ),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::xml::parse_str;

    #[test]
    fn linkage_only_when_exported() {
        let mut ctx = Ctx::default();
        let none =
            parse_str(r#"<DOMSymbolItem name="A" linkageExportInFirstFrame="true"/>"#).unwrap();
        assert_eq!(read_linkage(&mut ctx, &none), None);
        let some = parse_str(
            r#"<DOMSymbolItem name="A" linkageExportForAS="true" linkageIdentifier="ball_id" linkageExportInFirstFrame="false" linkageClassName="Ball"/>"#,
        )
        .unwrap();
        assert_eq!(
            read_linkage(&mut ctx, &some),
            Some(Linkage {
                identifier: "ball_id".into(),
                export_for_as: true,
                export_in_first_frame: false,
                class_name: Some("Ball".into()),
            })
        );
        assert!(ctx.report.is_empty());
    }

    #[test]
    fn scale9_from_either_form() {
        let mut ctx = Ctx::default();
        let a = parse_str(
            r#"<S scaleGridLeft="5" scaleGridRight="95" scaleGridTop="4" scaleGridBottom="36"/>"#,
        )
        .unwrap();
        assert_eq!(
            read_scale9(&mut ctx, &a),
            Some(Rect::new(5.0, 4.0, 95.0, 36.0))
        );
        let b = parse_str(r#"<S scalingGrid="true" scalingGridRect="100 80 1900 720"/>"#).unwrap();
        assert_eq!(
            read_scale9(&mut ctx, &b),
            Some(Rect::new(5.0, 4.0, 95.0, 36.0))
        );
        let zero = parse_str(r#"<S scaleGridLeft="0" scaleGridRight="0"/>"#).unwrap();
        assert_eq!(read_scale9(&mut ctx, &zero), None);
        assert!(ctx.report.is_empty());
    }

    #[test]
    fn font_items() {
        let mut ctx = Ctx::default();
        let n = parse_str(r#"<DOMFontItem name="Font 1" font="Tahoma" size="12" embedRanges="1|2|x" embeddedCharacters="مرحبا"/>"#).unwrap();
        let f = read_font_item(&mut ctx, &n).unwrap();
        assert_eq!(
            (f.name.as_str(), f.face.as_str(), f.size),
            ("Font 1", "Tahoma", 12.0)
        );
        assert_eq!(f.embed_ranges, [1, 2]);
        assert_eq!(f.embedded_characters, "مرحبا");
        assert_eq!(ctx.report.len(), 1);
    }

    #[test]
    fn xfl_versions_compare_as_numbers() {
        let warnings = |version: &str| {
            let mut ctx = Ctx::default();
            let root = parse_str(&format!(r#"<DOMDocument xflVersion="{version}"/>"#)).unwrap();
            let mut doc = Document::default();
            read_document_attributes(&mut ctx, &root, &mut doc);
            assert_eq!(doc.xfl_version, version);
            ctx.report.iter().map(|w| w.kind).collect::<Vec<_>>()
        };
        assert!(warnings("2.0").is_empty());
        assert!(warnings("2.2").is_empty());
        // Animate 2022+ writes "22.0" and later; "10.0" sorts before "2.2"
        // as text but is newer.
        assert_eq!(warnings("22.0"), [WarningKind::AnimateFeature]);
        assert_eq!(warnings("10.0"), [WarningKind::AnimateFeature]);
        assert_eq!(warnings("2.4"), [WarningKind::AnimateFeature]);
        assert_eq!(warnings("CS6"), [WarningKind::BadValue]);
    }

    #[test]
    fn twips_to_pixels() {
        assert_eq!(px_from_twips(2560.0), 128);
        assert_eq!(px_from_twips(3210.0), 161);
        assert_eq!(px_from_twips(-5.0), 0);
        assert_eq!(px_from_twips(f64::NAN), 0);
        assert_eq!(folder_of("bin/M 1.dat"), "bin/");
        assert_eq!(folder_of("x.dat"), "");
    }
}
