//! `jazari-cli dump <doc>`: a readable summary of a document model and its
//! import report.
//!
//! Frames are shown 1-based, as in the Flash IDE; layers are listed top
//! first, elements back to front.

use std::fmt::Write as _;

use jazari_xfl::ImportReport;
use jazari_xfl::model::{
    BitmapData, ContainerKind, Document, Element, Frame, LabelKind, Layer, LayerKind, LineType,
    MotionTween, RotateMode, SymbolKind, TextKind, Timeline, Tween,
};

/// How much detail to print.
#[derive(Debug, Clone, Copy, Default)]
pub struct DumpOptions {
    /// Print frame scripts and instance scripts in full.
    pub scripts: bool,
}

/// Formats the summary of `doc` followed by the import report.
pub fn summary(doc: &Document, report: &ImportReport, opts: DumpOptions) -> String {
    let mut out = String::new();
    document_header(&mut out, doc);
    for (i, scene) in doc.scenes.iter().enumerate() {
        let _ = writeln!(out);
        let title = format!("scene {} '{}'", i + 1, scene.name);
        timeline(&mut out, &title, scene, opts);
    }
    library(&mut out, doc, opts);
    import_report(&mut out, report);
    out
}

fn document_header(out: &mut String, doc: &Document) {
    let container = match doc.source.container {
        ContainerKind::InMemory => "in memory",
        ContainerKind::Folder => "XFL folder",
        ContainerKind::Zip => "FLA archive",
    };
    let _ = writeln!(out, "document: {} ({container})", doc.source.path.display());
    let _ = writeln!(out, "base folder: {}", doc.source.base_dir.display());
    let _ = writeln!(out, "XFL version: {}", or_dash(&doc.xfl_version));
    let bg = doc.background;
    let _ = writeln!(
        out,
        "stage: {} x {} px, {} fps, background #{:02X}{:02X}{:02X}{}",
        doc.width,
        doc.height,
        doc.frame_rate,
        bg.r,
        bg.g,
        bg.b,
        if bg.a == 255 {
            String::new()
        } else {
            format!(" alpha {}", bg.a)
        }
    );
    let p = &doc.publish;
    let mut flags = Vec::new();
    flags.push(if p.compress {
        "compressed"
    } else {
        "uncompressed"
    });
    if !p.publish_hidden_layers {
        flags.push("hidden layers left out");
    }
    if p.omit_trace {
        flags.push("omit trace");
    }
    if p.protect {
        flags.push("protected");
    }
    if p.debugging_permitted {
        flags.push("debugging permitted");
    }
    if p.use_network {
        flags.push("network access");
    }
    let _ = writeln!(
        out,
        "publish: Flash {}, ActionScript {}, {}, JPEG quality {}",
        p.flash_version,
        p.actionscript_version,
        flags.join(", "),
        p.jpeg_quality
    );
    if !p.package_paths.is_empty() {
        let _ = writeln!(out, "class paths: {}", p.package_paths.join("; "));
    }
    let frames: u32 = doc.scenes.iter().map(Timeline::frame_count).sum();
    let _ = writeln!(
        out,
        "scenes: {} ({} frame{} in all)",
        doc.scenes.len(),
        frames,
        plural(frames as usize)
    );
}

fn timeline(out: &mut String, title: &str, tl: &Timeline, opts: DumpOptions) {
    let frames = tl.frame_count();
    let _ = writeln!(
        out,
        "{title}: {frames} frame{}, {} layer{}",
        plural(frames as usize),
        tl.layers.len(),
        plural(tl.layers.len())
    );
    for (index, layer) in tl.layers.iter().enumerate() {
        layer_lines(out, tl, index, layer, opts);
    }
}

fn layer_lines(out: &mut String, tl: &Timeline, index: usize, layer: &Layer, opts: DumpOptions) {
    let mut kind = match &layer.kind {
        LayerKind::Normal => "normal".to_owned(),
        LayerKind::Guide => "guide".to_owned(),
        LayerKind::Mask => "mask".to_owned(),
        LayerKind::Folder => "folder".to_owned(),
        LayerKind::Other(other) => format!("other: {other}"),
    };
    if let Some(parent) = layer.parent {
        let parent_name = tl
            .layers
            .get(parent)
            .map(|p| p.name.as_str())
            .unwrap_or("?");
        let role = match tl.layers.get(parent).map(|p| &p.kind) {
            Some(LayerKind::Mask) => "masked by",
            Some(LayerKind::Guide) => "guided by",
            _ => "in",
        };
        let _ = write!(kind, ", {role} '{parent_name}'");
    }
    let mut state = Vec::new();
    if !layer.visible {
        state.push("hidden");
    }
    if layer.locked {
        state.push("locked");
    }
    if layer.outline {
        state.push("outline");
    }
    let state = if state.is_empty() {
        String::new()
    } else {
        format!(", {}", state.join(", "))
    };
    let keyframes = layer.frames.len();
    let _ = writeln!(
        out,
        "  layer {} '{}' [{kind}{state}]: {} frame{}, {keyframes} keyframe{}",
        index + 1,
        layer.name,
        layer.frame_count(),
        plural(layer.frame_count() as usize),
        plural(keyframes)
    );
    for frame in &layer.frames {
        frame_line(out, frame, opts);
    }
}

fn frame_line(out: &mut String, frame: &Frame, opts: DumpOptions) {
    let first = u64::from(frame.index) + 1;
    let last = u64::from(frame.end());
    let span = if frame.duration <= 1 {
        format!("frame {first}")
    } else {
        format!("frames {first}-{last}")
    };
    let mut parts = Vec::new();
    match &frame.tween {
        Tween::None => {}
        Tween::Motion(m) => parts.push(motion(m)),
        Tween::Shape(s) => parts.push(format!("shape tween (ease {})", s.ease)),
        Tween::Other(t) => parts.push(format!("tween '{t}'")),
    }
    if let Some(label) = &frame.label {
        let kind = match label.kind {
            LabelKind::Name => "label",
            LabelKind::Comment => "comment",
            LabelKind::Anchor => "anchor",
        };
        parts.push(format!("{kind} '{}'", label.name));
    }
    if let Some(script) = &frame.script {
        let lines = script.lines().count();
        parts.push(format!("script ({lines} line{})", plural(lines)));
    }
    if frame.elements.is_empty() {
        parts.push("no elements".to_owned());
    } else {
        let list: Vec<String> = frame.elements.iter().map(element).collect();
        parts.push(format!(
            "{} element{}: {}",
            list.len(),
            plural(list.len()),
            list.join(", ")
        ));
    }
    let _ = writeln!(out, "    {span}: {}", parts.join("; "));
    if opts.scripts {
        if let Some(script) = &frame.script {
            script_block(out, "frame script", script);
        }
        for el in &frame.elements {
            if let Element::Instance(inst) = el
                && let Some(script) = &inst.script
            {
                let who = inst.name.as_deref().unwrap_or(inst.library_item.as_str());
                script_block(out, &format!("script of '{who}'"), script);
            }
        }
    }
}

fn script_block(out: &mut String, title: &str, script: &str) {
    let _ = writeln!(out, "      {title}:");
    for line in script.lines() {
        let _ = writeln!(out, "      | {line}");
    }
}

fn motion(m: &MotionTween) -> String {
    let mut s = format!("motion tween (ease {}", m.ease);
    match m.rotate {
        RotateMode::None => {}
        RotateMode::Auto => s.push_str(", rotate auto"),
        RotateMode::Clockwise => {
            let _ = write!(s, ", rotate cw x{}", m.rotate_times);
        }
        RotateMode::CounterClockwise => {
            let _ = write!(s, ", rotate ccw x{}", m.rotate_times);
        }
    }
    if !m.scale {
        s.push_str(", no scale");
    }
    if m.custom_ease.is_some() {
        s.push_str(", custom ease");
    }
    if m.orient_to_path {
        s.push_str(", orient to path");
    }
    s.push(')');
    s
}

fn element(el: &Element) -> String {
    match el {
        Element::Shape(shape) => {
            let kind = if shape.is_drawing_object {
                "drawing object"
            } else {
                "shape"
            };
            format!(
                "{kind} ({} fill{}, {} stroke{}, {} edge{})",
                shape.fills.len(),
                plural(shape.fills.len()),
                shape.strokes.len(),
                plural(shape.strokes.len()),
                shape.edges.len(),
                plural(shape.edges.len())
            )
        }
        Element::Group(group) => {
            let members: Vec<String> = group.members.iter().map(element).collect();
            format!("group [{}]", members.join(", "))
        }
        Element::Instance(inst) => {
            let kind = symbol_kind(inst.kind);
            let mut s = format!("{kind} '{}'", inst.library_item);
            if let Some(name) = &inst.name {
                let _ = write!(s, " named '{name}'");
            }
            let mut extras = Vec::new();
            if !inst.filters.is_empty() {
                extras.push(format!(
                    "{} filter{}",
                    inst.filters.len(),
                    plural(inst.filters.len())
                ));
            }
            if !inst.color.is_identity() {
                extras.push("colour effect".to_owned());
            }
            if inst.blend_mode != Default::default() {
                extras.push(format!("blend {:?}", inst.blend_mode).to_lowercase());
            }
            if inst.cache_as_bitmap {
                extras.push("cached".to_owned());
            }
            if inst.script.is_some() {
                extras.push("script".to_owned());
            }
            if let Some(g) = inst.graphic {
                extras.push(format!(
                    "{:?} from frame {}",
                    g.mode,
                    u64::from(g.first_frame) + 1
                ));
            }
            if !extras.is_empty() {
                let _ = write!(s, " ({})", extras.join(", "));
            }
            s
        }
        Element::Bitmap(bitmap) => format!("bitmap '{}'", bitmap.library_item),
        Element::Text(text) => {
            let kind = match text.kind {
                TextKind::Static => "static text",
                TextKind::Dynamic => "dynamic text",
                TextKind::Input => "input text",
            };
            let content: String = text.runs.iter().map(|r| r.text.as_str()).collect();
            let mut s = format!("{kind} {:?}", shorten(&content, 32));
            if let Some(name) = &text.name {
                let _ = write!(s, " named '{name}'");
            }
            if let Some(var) = &text.variable {
                let _ = write!(s, " var '{var}'");
            }
            if text.line_type != LineType::SingleLine {
                let _ = write!(s, " ({:?})", text.line_type);
            }
            s
        }
        Element::Unsupported { tag } => format!("unsupported <{tag}>"),
    }
}

fn library(out: &mut String, doc: &Document, opts: DumpOptions) {
    let lib = &doc.library;
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "library: {} symbol{}, {} bitmap{}, {} font{}, {} folder{}",
        lib.symbols.len(),
        plural(lib.symbols.len()),
        lib.bitmaps.len(),
        plural(lib.bitmaps.len()),
        lib.fonts.len(),
        plural(lib.fonts.len()),
        lib.folders.len(),
        plural(lib.folders.len())
    );
    for folder in &lib.folders {
        let _ = writeln!(out, "  folder '{folder}'");
    }
    for bitmap in lib.bitmaps.values() {
        let data = match &bitmap.data {
            BitmapData::Rgba {
                premultiplied,
                pixels,
            } => format!(
                "lossless{}, {} bytes",
                if *premultiplied {
                    " (premultiplied)"
                } else {
                    ""
                },
                pixels.len()
            ),
            BitmapData::Jpeg { bytes, alpha } => format!(
                "JPEG, {} bytes{}",
                bytes.len(),
                if alpha.is_some() { ", with alpha" } else { "" }
            ),
        };
        let _ = writeln!(
            out,
            "  bitmap '{}': {} x {} px, {data}, smoothing {}{}",
            bitmap.name,
            bitmap.width,
            bitmap.height,
            if bitmap.smoothing { "on" } else { "off" },
            linkage(bitmap.linkage.as_ref())
        );
    }
    for font in lib.fonts.values() {
        let _ = writeln!(
            out,
            "  font '{}': face '{}', {} pt, {} range{}, {} extra character{}{}",
            font.name,
            font.face,
            font.size,
            font.embed_ranges.len(),
            plural(font.embed_ranges.len()),
            font.embedded_characters.chars().count(),
            plural(font.embedded_characters.chars().count()),
            linkage(font.linkage.as_ref())
        );
    }
    for symbol in lib.symbols.values() {
        let _ = writeln!(out);
        let title = format!(
            "symbol '{}' ({}{}{})",
            symbol.name,
            symbol_kind(symbol.kind),
            if symbol.scale9.is_some() {
                ", 9-slice"
            } else {
                ""
            },
            linkage(symbol.linkage.as_ref())
        );
        timeline(out, &title, &symbol.timeline, opts);
    }
}

fn import_report(out: &mut String, report: &ImportReport) {
    let _ = writeln!(out);
    if report.is_empty() {
        let _ = writeln!(out, "import report: no warnings");
        return;
    }
    let _ = writeln!(
        out,
        "import report: {} warning{}",
        report.len(),
        plural(report.len())
    );
    for w in report {
        let _ = writeln!(out, "  [{:?}] {w}", w.kind);
    }
}

fn symbol_kind(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::MovieClip => "movie clip",
        SymbolKind::Graphic => "graphic",
        SymbolKind::Button => "button",
    }
}

fn linkage(l: Option<&jazari_xfl::model::Linkage>) -> String {
    let Some(l) = l else {
        return String::new();
    };
    let mut s = format!(", linkage '{}'", l.identifier);
    if let Some(class) = &l.class_name {
        let _ = write!(s, " class {class}");
    }
    if !l.export_for_as {
        s.push_str(" (not exported)");
    } else if !l.export_in_first_frame {
        s.push_str(" (not in first frame)");
    }
    s
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn or_dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

/// The first `max` characters of `s` (with an ellipsis when cut), with line
/// breaks shown as spaces.
fn shorten(s: &str, max: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
        .collect();
    if flat.chars().count() <= max {
        flat
    } else {
        let mut cut: String = flat.chars().take(max).collect();
        cut.push('…');
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jazari_xfl::model::{Frame, Layer, SymbolInstance, Timeline};

    #[test]
    fn summary_lists_scenes_layers_frames_and_report() {
        let mut doc = Document::default();
        let mut scene = Timeline::new("Scene 1");
        let mut layer = Layer {
            name: "art".to_owned(),
            ..Layer::default()
        };
        layer.frames = vec![Frame {
            index: 0,
            duration: 12,
            script: Some("trace(1);\nstop();".to_owned()),
            elements: vec![Element::Instance(SymbolInstance {
                library_item: "Ball".to_owned(),
                name: Some("ball_mc".to_owned()),
                ..SymbolInstance::default()
            })],
            ..Frame::default()
        }];
        scene.layers.push(layer);
        doc.scenes.push(scene);
        let text = summary(&doc, &ImportReport::new(), DumpOptions { scripts: true });
        assert!(
            text.contains("scene 1 'Scene 1': 12 frames, 1 layer"),
            "{text}"
        );
        assert!(text.contains("layer 1 'art' [normal]: 12 frames, 1 keyframe"));
        assert!(text.contains(
            "frames 1-12: script (2 lines); 1 element: movie clip 'Ball' named 'ball_mc'"
        ));
        assert!(text.contains("      | stop();"));
        assert!(text.contains("import report: no warnings"));
    }

    #[test]
    fn shorten_cuts_and_flattens() {
        assert_eq!(shorten("a\rb", 10), "a b");
        assert_eq!(shorten("abcdef", 3), "abc…");
    }
}
