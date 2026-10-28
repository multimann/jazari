//! The Actions panel (read-only in milestone 1): the script of the selected
//! keyframe with line numbers, and the `on()` / `onClipEvent()` scripts of
//! the instances on it. Lines with right-to-left text are shaped. Only the
//! visible rows are laid out, so long scripts stay fast.

use eframe::egui::{self, CornerRadius, RichText, ScrollArea, Shape, Ui};
use jazari_xfl::model::{Element, Frame};

use crate::panels::centered_message;
use crate::shaped::needs_shaping;
use crate::workspace::Workspace;

/// egui memory key of the line last scrolled into view.
const SCROLLED_TO: &str = "jazari-actions-scrolled-to";

/// Draws the panel.
pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    let Workspace {
        doc,
        shaped,
        view,
        selected_layer,
        selected_frame,
        highlight_line,
        ..
    } = ws;
    let Some(doc) = doc.as_ref() else {
        centered_message(ui, tr!("actions.no_document"), None);
        return;
    };
    let (Some(layer_index), Some(frame)) = (*selected_layer, *selected_frame) else {
        centered_message(ui, tr!("actions.no_selection"), None);
        return;
    };
    let Some(layer) = doc.timeline(view).and_then(|t| t.layers.get(layer_index)) else {
        centered_message(ui, tr!("actions.no_selection"), None);
        return;
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(doc.timeline_name(view)).weak());
        ui.label(RichText::new("›").weak());
        shaped.label(ui, &layer.name, None);
        ui.label(RichText::new("›").weak());
        ui.label(tr!("actions.frame", frame = u64::from(frame) + 1));
        ui.label(RichText::new(tr!("actions.read_only")).weak());
    });
    ui.separator();
    let Some(span) = layer.frame_at(frame) else {
        centered_message(ui, tr!("actions.no_keyframe"), None);
        return;
    };
    let instance_scripts = instance_scripts(span);
    let script = span.script.as_deref().filter(|s| !s.trim().is_empty());
    if script.is_none() && instance_scripts.is_empty() {
        let text = if span.index == frame {
            tr!("actions.no_script")
        } else {
            tr!("actions.no_script_span")
        };
        centered_message(ui, text, None);
        return;
    }

    let rows = code_rows(span, frame, script, &instance_scripts);
    let digits = rows
        .iter()
        .filter_map(|r| match r {
            Row::Line { number, .. } => Some(*number),
            Row::Heading(_) => None,
        })
        .max()
        .unwrap_or(1)
        .to_string()
        .len();
    let mono = egui::TextStyle::Monospace.resolve(ui.style());
    let row_height = ui
        .text_style_height(&egui::TextStyle::Monospace)
        .max(mono.size * 1.35);
    let spacing = ui.spacing().item_spacing.y;

    // Scroll a newly highlighted line (chosen in the Output panel) into
    // view, once.
    let highlight = *highlight_line;
    let scroll_id = egui::Id::new(SCROLLED_TO);
    let mut area = ScrollArea::both()
        .id_salt("actions-code")
        .auto_shrink([false, false]);
    match highlight {
        None => ui.ctx().data_mut(|d| d.remove::<u32>(scroll_id)),
        Some(line) => {
            let done = ui.ctx().data(|d| d.get_temp::<u32>(scroll_id)) == Some(line);
            let index = rows
                .iter()
                .position(|r| matches!(r, Row::Line { number, main: true, .. } if *number == line));
            if let (false, Some(index)) = (done, index) {
                let target = index as f32 * (row_height + spacing);
                let offset = (target - ui.available_height() / 2.0).max(0.0);
                area = area.vertical_scroll_offset(offset);
                ui.ctx().data_mut(|d| d.insert_temp(scroll_id, line));
            }
        }
    }
    let highlight_color = ui.visuals().selection.bg_fill.gamma_multiply(0.5);
    let gutter_color = ui.visuals().weak_text_color();
    area.show_rows(ui, row_height, rows.len(), |ui, range| {
        for row in &rows[range] {
            match row {
                Row::Heading(text) => {
                    ui.add_sized(
                        [ui.available_width(), row_height],
                        egui::Label::new(RichText::new(text).strong()),
                    );
                }
                Row::Line { number, text, main } => {
                    let background = ui.painter().add(Shape::Noop);
                    let response = ui
                        .horizontal(|ui| {
                            ui.set_height(row_height);
                            ui.spacing_mut().item_spacing.x = 10.0;
                            ui.label(
                                RichText::new(format!("{number:>digits$}"))
                                    .font(mono.clone())
                                    .color(gutter_color),
                            );
                            if needs_shaping(text) {
                                let color = ui.visuals().text_color();
                                // Code reads left to right: a leading LRM
                                // keeps the line's base direction LTR,
                                // whatever its first letter.
                                let line = format!("\u{200E}{text}");
                                shaped.label_sized(ui, &line, mono.size, color);
                            } else {
                                let line = text.replace('\t', "    ");
                                ui.label(RichText::new(line).font(mono.clone()));
                            }
                        })
                        .response;
                    if *main && highlight == Some(*number) {
                        ui.painter().set(
                            background,
                            Shape::rect_filled(
                                response.rect.expand2(egui::vec2(2.0, 0.0)),
                                CornerRadius::same(2),
                                highlight_color,
                            ),
                        );
                    }
                }
            }
        }
    });
}

/// One row of the code view.
enum Row<'a> {
    /// A heading: where the following lines come from.
    Heading(String),
    /// A script line.
    Line {
        /// 1-based line number.
        number: u32,
        /// The text, without its line break.
        text: &'a str,
        /// It belongs to the frame script (the one diagnostics point into).
        main: bool,
    },
}

/// The rows: the frame script, then each instance script under a heading.
fn code_rows<'a>(
    span: &Frame,
    frame: u32,
    script: Option<&'a str>,
    instances: &'a [(String, String)],
) -> Vec<Row<'a>> {
    fn lines<'a>(rows: &mut Vec<Row<'a>>, source: &'a str, main: bool) {
        for (i, text) in source.split('\n').enumerate() {
            rows.push(Row::Line {
                number: u32::try_from(i + 1).unwrap_or(u32::MAX),
                text: text.trim_end_matches('\r'),
                main,
            });
        }
    }
    let mut rows = Vec::new();
    if let Some(script) = script {
        if span.index != frame {
            rows.push(Row::Heading(tr!(
                "actions.keyframe_at",
                frame = u64::from(span.index) + 1
            )));
        }
        lines(&mut rows, script, true);
    }
    for (name, source) in instances {
        rows.push(Row::Heading(tr!("actions.instance", name = name)));
        lines(&mut rows, source, false);
    }
    rows
}

/// `(instance name, script)` for every instance with a script, in stacking
/// order.
fn instance_scripts(span: &Frame) -> Vec<(String, String)> {
    span.elements
        .iter()
        .filter_map(|e| match e {
            Element::Instance(i) => i.script.as_ref().map(|s| {
                let name = i
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| tr!("actions.unnamed", item = i.library_item));
                (name, s.clone())
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_number_each_script_from_one() {
        let span = Frame {
            index: 2,
            duration: 5,
            ..Frame::default()
        };
        let instances = vec![("btn".to_owned(), "on (release) {\r\n}".to_owned())];
        let rows = code_rows(&span, 4, Some("a();\nb();"), &instances);
        let shape: Vec<String> = rows
            .iter()
            .map(|r| match r {
                Row::Heading(_) => "H".to_owned(),
                Row::Line { number, text, main } => {
                    format!("{number}{}{text}", if *main { "*" } else { "" })
                }
            })
            .collect();
        assert_eq!(
            shape,
            ["H", "1*a();", "2*b();", "H", "1on (release) {", "2}"]
        );
    }
}
