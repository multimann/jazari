//! The Properties panel (read-only in milestone 1): the document, the shown
//! timeline, the selected layer, frame and tween, and the selected library
//! item.

use eframe::egui::{self, Color32, CornerRadius, RichText, ScrollArea, Sense, Ui, Vec2};
use jazari_xfl::model::{
    BitmapData, ContainerKind, Document, LabelKind, Layer, LayerKind, Rgba, RotateMode, Tween,
};

use crate::doc::{OpenDoc, TimelineRef};
use crate::library::{LibKind, item_count};
use crate::panels::centered_message;
use crate::shaped::ShapedText;
use crate::workspace::Workspace;

/// Draws the panel.
pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    let Workspace {
        doc,
        shaped,
        view,
        selected_layer,
        selected_frame,
        selected_item,
        library,
        ..
    } = ws;
    let Some(doc) = doc.as_ref() else {
        centered_message(ui, tr!("props.no_document"), None);
        return;
    };
    ScrollArea::vertical()
        .id_salt("properties")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The selection first, as Flash's Property inspector does; the
            // document last.
            let layer = selected_layer.and_then(|l| {
                doc.timeline(view)
                    .and_then(|tl| tl.layers.get(l))
                    .map(|layer| (l, layer))
            });
            if let Some((index, layer)) = layer {
                if let Some(frame) = *selected_frame {
                    section(ui, "props-frame", tr!("props.frame"), |ui| {
                        frame_rows(ui, shaped, layer, frame);
                    });
                }
                section(ui, "props-layer", tr!("props.layer"), |ui| {
                    layer_rows(ui, shaped, doc, view, index, layer);
                });
            }
            if let Some(item) = selected_item.as_deref() {
                section(ui, "props-item", tr!("props.item"), |ui| {
                    item_rows(ui, shaped, &doc.doc, item);
                });
            }
            section(ui, "props-timeline", tr!("props.timeline"), |ui| {
                let name = doc.timeline_name(view);
                row(ui, tr!("props.name"), |ui| {
                    shaped.label(ui, &name, None);
                });
                let kind = match view {
                    TimelineRef::Scene(_) => tr!("props.kind_scene"),
                    TimelineRef::Symbol(_) => tr!("props.kind_symbol"),
                };
                text_row(ui, tr!("props.kind"), kind);
                if let Some(tl) = doc.timeline(view) {
                    text_row(ui, tr!("props.layers"), &tl.layers.len().to_string());
                    text_row(ui, tr!("props.frames"), &tl.frame_count().to_string());
                }
            });
            section(ui, "props-doc", tr!("props.document"), |ui| {
                document_rows(ui, shaped, doc, item_count(library));
            });
        });
}

fn section(ui: &mut Ui, id: &str, title: &str, body: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).strong())
        .id_salt(id)
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new(id)
                .num_columns(2)
                .spacing([12.0, 4.0])
                .striped(true)
                .show(ui, body);
        });
}

fn row(ui: &mut Ui, label: &str, value: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(label).weak());
    value(ui);
    ui.end_row();
}

fn text_row(ui: &mut Ui, label: &str, value: &str) {
    row(ui, label, |ui| {
        ui.label(value);
    });
}

fn yes_no(b: bool) -> &'static str {
    if b { tr!("props.yes") } else { tr!("props.no") }
}

fn swatch(ui: &mut Ui, c: Rgba) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(28.0, 14.0), Sense::hover());
    let color = Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a);
    ui.painter().rect_filled(rect, CornerRadius::same(2), color);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(2),
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        egui::StrokeKind::Inside,
    );
}

fn hex(c: Rgba) -> String {
    if c.a == 255 {
        format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
    } else {
        format!(
            "#{:02X}{:02X}{:02X} ({:.0}%)",
            c.r,
            c.g,
            c.b,
            c.alpha_ratio() * 100.0
        )
    }
}

fn document_rows(ui: &mut Ui, shaped: &mut ShapedText, open: &OpenDoc, items: usize) {
    let d = &open.doc;
    text_row(
        ui,
        tr!("props.size"),
        &tr!("props.size_value", width = d.width, height = d.height),
    );
    text_row(
        ui,
        tr!("props.fps"),
        &tr!("props.fps_value", fps = d.frame_rate),
    );
    row(ui, tr!("props.background"), |ui| {
        ui.horizontal(|ui| {
            swatch(ui, d.background);
            ui.label(hex(d.background));
        });
    });
    let version = if d.xfl_version.is_empty() {
        tr!("props.unknown")
    } else {
        d.xfl_version.as_str()
    };
    text_row(ui, tr!("props.xfl_version"), version);
    text_row(
        ui,
        tr!("props.player"),
        &tr!(
            "props.player_value",
            player = d.publish.flash_version,
            script = d.publish.actionscript_version
        ),
    );
    if d.publish.flash_version != 8 || d.publish.actionscript_version != 2 {
        row(ui, "", |ui| {
            ui.label(RichText::new(tr!("props.publishes_as_8")).color(ui.visuals().warn_fg_color));
        });
    }
    text_row(ui, tr!("props.compressed"), yes_no(d.publish.compress));
    text_row(ui, tr!("props.scenes"), &d.scenes.len().to_string());
    text_row(ui, tr!("props.library_items"), &items.to_string());
    let container = match d.source.container {
        ContainerKind::Folder => tr!("props.container_folder"),
        ContainerKind::Zip => tr!("props.container_zip"),
        ContainerKind::InMemory => tr!("props.container_memory"),
    };
    text_row(ui, tr!("props.container"), container);
    row(ui, tr!("props.source"), |ui| {
        let text = open.path.display().to_string();
        if crate::shaped::needs_shaping(&text) {
            shaped.label(ui, &text, None);
        } else {
            ui.add(egui::Label::new(text.as_str()).truncate())
                .on_hover_text(text.as_str());
        }
    });
}

fn layer_kind(kind: &LayerKind) -> String {
    match kind {
        LayerKind::Normal => tr!("props.layer_normal").to_owned(),
        LayerKind::Guide => tr!("props.layer_guide").to_owned(),
        LayerKind::Mask => tr!("props.layer_mask").to_owned(),
        LayerKind::Folder => tr!("props.layer_folder").to_owned(),
        LayerKind::Other(name) => tr!("props.layer_other", name = name),
    }
}

fn layer_rows(
    ui: &mut Ui,
    shaped: &mut ShapedText,
    doc: &OpenDoc,
    view: &TimelineRef,
    index: usize,
    layer: &Layer,
) {
    row(ui, tr!("props.name"), |ui| {
        shaped.label(ui, &layer.name, None);
    });
    text_row(ui, tr!("props.kind"), &layer_kind(&layer.kind));
    text_row(
        ui,
        tr!("props.index"),
        &tr!("props.index_value", n = index + 1),
    );
    if let Some(parent) = layer.parent {
        let name = doc
            .timeline(view)
            .and_then(|tl| tl.layers.get(parent))
            .map_or_else(|| tr!("props.missing").to_owned(), |p| p.name.clone());
        row(ui, tr!("props.parent"), |ui| {
            shaped.label(ui, &name, None);
        });
    }
    text_row(ui, tr!("props.visible"), yes_no(layer.visible));
    text_row(ui, tr!("props.locked"), yes_no(layer.locked));
    text_row(ui, tr!("props.outline"), yes_no(layer.outline));
    row(ui, tr!("props.color"), |ui| {
        ui.horizontal(|ui| {
            swatch(ui, layer.color);
            ui.label(hex(layer.color));
        });
    });
    text_row(ui, tr!("props.keyframes"), &layer.frames.len().to_string());
}

fn frame_rows(ui: &mut Ui, shaped: &mut ShapedText, layer: &Layer, frame: u32) {
    text_row(
        ui,
        tr!("props.frame_number"),
        &(u64::from(frame) + 1).to_string(),
    );
    let Some(span) = layer.frame_at(frame) else {
        text_row(ui, tr!("props.keyframe"), tr!("props.no_keyframe"));
        return;
    };
    text_row(
        ui,
        tr!("props.keyframe"),
        &tr!(
            "props.span_value",
            first = u64::from(span.index) + 1,
            last = u64::from(span.end()),
            count = span.duration
        ),
    );
    text_row(ui, tr!("props.elements"), &span.elements.len().to_string());
    if let Some(label) = &span.label {
        let kind = match label.kind {
            LabelKind::Name => tr!("props.label_name"),
            LabelKind::Comment => tr!("props.label_comment"),
            LabelKind::Anchor => tr!("props.label_anchor"),
        };
        row(ui, tr!("props.label"), |ui| {
            ui.horizontal(|ui| {
                shaped.label(ui, &label.name, None);
                ui.label(RichText::new(format!("({kind})")).weak());
            });
        });
    }
    let script = match &span.script {
        Some(s) => tr!("props.script_lines", lines = s.lines().count()),
        None => tr!("props.none").to_owned(),
    };
    text_row(ui, tr!("props.script"), &script);
    match &span.tween {
        Tween::None => text_row(ui, tr!("props.tween"), tr!("props.none")),
        Tween::Motion(m) => {
            text_row(ui, tr!("props.tween"), tr!("props.tween_motion"));
            text_row(ui, tr!("props.ease"), &format!("{:+.0}", m.ease));
            if m.custom_ease.is_some() {
                text_row(ui, tr!("props.custom_ease"), tr!("props.yes"));
            }
            let rotate = match m.rotate {
                RotateMode::Auto => tr!("props.rotate_auto").to_owned(),
                RotateMode::None => tr!("props.none").to_owned(),
                RotateMode::Clockwise => tr!("props.rotate_cw", times = m.rotate_times),
                RotateMode::CounterClockwise => tr!("props.rotate_ccw", times = m.rotate_times),
            };
            text_row(ui, tr!("props.rotate"), &rotate);
            text_row(ui, tr!("props.scale"), yes_no(m.scale));
            text_row(ui, tr!("props.orient"), yes_no(m.orient_to_path));
            text_row(ui, tr!("props.sync"), yes_no(m.sync));
            text_row(ui, tr!("props.snap"), yes_no(m.snap));
        }
        Tween::Shape(t) => {
            text_row(ui, tr!("props.tween"), tr!("props.tween_shape"));
            text_row(ui, tr!("props.ease"), &format!("{:+.0}", t.ease));
            text_row(ui, tr!("props.blend"), &t.blend);
            row(ui, "", |ui| {
                ui.label(RichText::new(tr!("props.shape_tween_note")).weak());
            });
        }
        Tween::Other(name) => {
            text_row(
                ui,
                tr!("props.tween"),
                &tr!("props.tween_other", name = name),
            );
        }
    }
}

fn item_rows(ui: &mut Ui, shaped: &mut ShapedText, doc: &Document, item: &str) {
    row(ui, tr!("props.name"), |ui| {
        shaped.label(ui, item, None);
    });
    let lib = &doc.library;
    if let Some(sym) = lib.symbols.get(item) {
        let kind = match LibKind::of_symbol(sym.kind) {
            LibKind::Graphic => tr!("library.graphic"),
            LibKind::Button => tr!("library.button"),
            _ => tr!("library.movie_clip"),
        };
        text_row(ui, tr!("props.kind"), kind);
        text_row(
            ui,
            tr!("props.layers"),
            &sym.timeline.layers.len().to_string(),
        );
        text_row(
            ui,
            tr!("props.frames"),
            &sym.timeline.frame_count().to_string(),
        );
        if let Some(link) = &sym.linkage {
            text_row(ui, tr!("props.linkage"), &link.identifier);
            if let Some(class) = &link.class_name {
                text_row(ui, tr!("props.class"), class);
            }
        }
        text_row(ui, tr!("props.scale9"), yes_no(sym.scale9.is_some()));
    } else if let Some(bmp) = lib.bitmaps.get(item) {
        text_row(ui, tr!("props.kind"), tr!("library.bitmap"));
        text_row(
            ui,
            tr!("props.size"),
            &tr!("props.size_value", width = bmp.width, height = bmp.height),
        );
        let format = match &bmp.data {
            BitmapData::Rgba { .. } => tr!("props.lossless"),
            BitmapData::Jpeg { alpha: Some(_), .. } => tr!("props.jpeg_alpha"),
            BitmapData::Jpeg { .. } => tr!("props.jpeg"),
        };
        text_row(ui, tr!("props.format"), format);
        text_row(ui, tr!("props.smoothing"), yes_no(bmp.smoothing));
    } else if let Some(font) = lib.fonts.get(item) {
        text_row(ui, tr!("props.kind"), tr!("library.font"));
        text_row(ui, tr!("props.face"), &font.face);
        text_row(ui, tr!("props.font_size"), &format!("{}", font.size));
    } else {
        text_row(ui, tr!("props.kind"), tr!("library.folder"));
    }
}
