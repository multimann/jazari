//! The Timeline panel, painted by hand: layer rows with their kind and
//! hidden/locked state, frame cells, keyframe dots, tween spans with
//! arrows, labels and the playhead. Clicking a cell selects it and moves
//! the playhead (and the stage); dragging scrubs. The layer column and the
//! frame ruler stay pinned while the grid scrolls.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, RichText, ScrollArea, Sense,
    Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use jazari_xfl::model::{Frame, LabelKind, Layer, LayerKind, Timeline, Tween};

use crate::doc::TimelineRef;
use crate::icons::{self, Icon};
use crate::panels::centered_message;
use crate::shaped::{BASELINE_BELOW_CENTER, ShapedText};
use crate::timeline_geom::TimelineGeom;
use crate::workspace::Workspace;

/// What the user did in the panel; applied after painting.
enum Action {
    SelectCell(usize, u32),
    SelectLayer(usize),
    Scrub(u32),
    Scene(usize),
    BackToScene,
    TogglePlay,
    Rewind,
    Step(i64),
}

/// Draws the panel.
pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    if ws.doc.is_none() {
        centered_message(ui, tr!("timeline.no_document"), None);
        return;
    }
    let mut actions = Vec::new();
    toolbar(ui, ws, &mut actions);
    ui.separator();
    grid(ui, ws, &mut actions);
    for action in actions {
        match action {
            Action::SelectCell(layer, frame) => {
                ws.playback = None;
                ws.select_cell(layer, frame);
            }
            Action::SelectLayer(layer) => ws.selected_layer = Some(layer),
            Action::Scrub(frame) => {
                ws.playback = None;
                ws.set_frame(frame);
            }
            Action::Scene(i) => ws.show_timeline(TimelineRef::Scene(i)),
            Action::BackToScene => ws.show_timeline(TimelineRef::Scene(0)),
            Action::TogglePlay => ws.toggle_play(),
            Action::Rewind => ws.rewind(),
            Action::Step(d) => ws.step(d),
        }
    }
}

fn toolbar(ui: &mut Ui, ws: &mut Workspace, actions: &mut Vec<Action>) {
    let Workspace {
        doc, shaped, view, ..
    } = ws;
    let Some(doc) = doc.as_ref() else {
        return;
    };
    ui.horizontal(|ui| match view {
        TimelineRef::Scene(current) => {
            let current = *current;
            let name = doc.timeline_name(&TimelineRef::Scene(current));
            icon_label(ui, Icon::Scene);
            egui::ComboBox::from_id_salt("timeline-scene")
                .selected_text(name)
                .show_ui(ui, |ui| {
                    for (i, scene) in doc.doc.scenes.iter().enumerate() {
                        if ui.selectable_label(i == current, &scene.name).clicked() {
                            actions.push(Action::Scene(i));
                        }
                    }
                })
                .response
                .on_hover_text(tr!("timeline.scene_hover"));
        }
        TimelineRef::Symbol(name) => {
            if ui
                .button(tr!("timeline.back_to_scene"))
                .on_hover_text(tr!("timeline.back_to_scene_hover"))
                .clicked()
            {
                actions.push(Action::BackToScene);
            }
            let kind = doc.doc.library.symbols.get(name.as_str()).map(|s| s.kind);
            icon_label(
                ui,
                match kind {
                    Some(jazari_xfl::model::SymbolKind::Graphic) => Icon::Graphic,
                    Some(jazari_xfl::model::SymbolKind::Button) => Icon::Button,
                    _ => Icon::MovieClip,
                },
            );
            shaped.label(ui, name, None);
        }
    });
    let frame = ws.frame;
    let count = ws.frame_count();
    let fps = ws.fps();
    let playing = ws.playback.is_some();
    ui.horizontal(|ui| {
        if ui.button(tr!("timeline.rewind")).clicked() {
            actions.push(Action::Rewind);
        }
        if ui.button(tr!("timeline.step_back")).clicked() {
            actions.push(Action::Step(-1));
        }
        let play = if playing {
            tr!("timeline.pause")
        } else {
            tr!("timeline.play")
        };
        if ui
            .button(play)
            .on_hover_text(tr!("timeline.play_hover"))
            .clicked()
        {
            actions.push(Action::TogglePlay);
        }
        if ui.button(tr!("timeline.step_forward")).clicked() {
            actions.push(Action::Step(1));
        }
        ui.separator();
        ui.label(tr!("app.frame_of", frame = frame + 1, total = count));
        ui.label(
            RichText::new(tr!(
                "timeline.time",
                fps = format!("{fps:.0}"),
                seconds = format!("{:.2}", f64::from(frame) / fps)
            ))
            .weak(),
        );
    });
}

fn icon_label(ui: &mut Ui, icon: Icon) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    icons::paint(ui.painter(), rect, icon, ui.visuals().text_color());
}

/// Colours of the grid, from the current theme.
struct Palette {
    cell: Color32,
    fifth: Color32,
    grid: Color32,
    row_line: Color32,
    span: Color32,
    motion: Color32,
    shape: Color32,
    other: Color32,
    marker: Color32,
    text: Color32,
    weak: Color32,
    select: Color32,
    layer_select: Color32,
    playhead: Color32,
    chrome: Color32,
    label: Color32,
    comment: Color32,
    anchor: Color32,
}

impl Palette {
    fn new(v: &egui::Visuals) -> Self {
        let dark = v.dark_mode;
        let pick = |d: (u8, u8, u8), l: (u8, u8, u8)| {
            let (r, g, b) = if dark { d } else { l };
            Color32::from_rgb(r, g, b)
        };
        Self {
            cell: pick((40, 40, 40), (255, 255, 255)),
            fifth: pick((50, 50, 50), (238, 238, 238)),
            grid: pick((58, 58, 58), (222, 222, 222)),
            row_line: pick((66, 66, 66), (200, 200, 200)),
            span: pick((92, 92, 92), (208, 208, 208)),
            motion: pick((92, 86, 150), (204, 204, 255)),
            shape: pick((72, 120, 76), (204, 255, 204)),
            other: pick((140, 100, 60), (255, 220, 180)),
            marker: pick((235, 235, 235), (0, 0, 0)),
            text: v.text_color(),
            weak: v.weak_text_color(),
            select: v.selection.bg_fill,
            layer_select: v.selection.bg_fill.gamma_multiply(0.35),
            playhead: Color32::from_rgb(230, 50, 40),
            chrome: v.panel_fill,
            label: Color32::from_rgb(220, 50, 40),
            comment: Color32::from_rgb(60, 170, 70),
            anchor: Color32::from_rgb(210, 160, 30),
        }
    }
}

fn grid(ui: &mut Ui, ws: &mut Workspace, actions: &mut Vec<Action>) {
    let Workspace {
        doc,
        shaped,
        view,
        frame: playhead,
        selected_layer,
        selected_frame,
        ..
    } = ws;
    let Some(doc) = doc.as_ref() else {
        return;
    };
    let Some(timeline) = doc.timeline(view) else {
        centered_message(ui, tr!("timeline.missing"), None);
        return;
    };
    let geom = scaled_geom(ui);
    let rows = timeline.layers.len();
    let frame_count = timeline.frame_count();
    let columns = geom.columns_for(frame_count, ui.available_width());
    let (content_w, content_h) = geom.content_size(columns, rows);
    let palette = Palette::new(ui.visuals());
    let depths = layer_depths(&timeline.layers);
    let id_salt = match view {
        TimelineRef::Scene(i) => format!("timeline-scene-{i}"),
        TimelineRef::Symbol(n) => format!("timeline-symbol-{n}"),
    };
    let state = GridState {
        geom,
        timeline,
        rows,
        columns,
        playhead: *playhead,
        selected_layer: *selected_layer,
        selected_frame: *selected_frame,
        depths: &depths,
        palette: &palette,
    };

    ScrollArea::both()
        .id_salt(id_salt)
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            ui.set_min_size(vec2(content_w, content_h.max(viewport.height())));
            let origin = ui.max_rect().min;
            let visible = viewport.translate(origin.to_vec2());
            let painter = ui.painter_at(visible);
            let ctx = ui.ctx().clone();
            paint_cells(&painter, &state, origin, visible, viewport);
            paint_ruler(&painter, &state, origin, visible, viewport);
            paint_names(&ctx, &painter, shaped, &state, origin, visible, viewport);

            let response = ui.interact(
                visible,
                ui.id().with("timeline-grid"),
                Sense::click_and_drag(),
            );
            if let Some(pos) = response.interact_pointer_pos() {
                let local = pos - origin;
                let in_names = pos.x < visible.left() + geom.names_width;
                let in_ruler = pos.y < visible.top() + geom.ruler_height;
                let frame = geom.frame_at(local.x);
                let row = geom.row_at(local.y, rows);
                if in_names {
                    if response.clicked()
                        && !in_ruler
                        && let Some(r) = row
                    {
                        actions.push(Action::SelectLayer(r));
                    }
                } else if let Some(f) = frame {
                    if in_ruler {
                        if response.clicked() || response.dragged() {
                            actions.push(Action::Scrub(f));
                        }
                    } else if response.clicked() || response.drag_started() {
                        match row {
                            Some(r) => actions.push(Action::SelectCell(r, f)),
                            None => actions.push(Action::Scrub(f)),
                        }
                    } else if response.dragged() {
                        actions.push(Action::Scrub(f));
                    }
                }
            }
        });
}

/// The grid geometry scaled with the body font, so it follows the UI size.
fn scaled_geom(ui: &Ui) -> TimelineGeom {
    let row = ui.text_style_height(&egui::TextStyle::Body).max(14.0) + 4.0;
    TimelineGeom {
        row_height: row.round(),
        ruler_height: row.round(),
        ..TimelineGeom::default()
    }
}

/// Indentation of each layer: how many mask, guide or folder layers it
/// sits under. Cycles in broken files stop at the layer count.
fn layer_depths(layers: &[Layer]) -> Vec<usize> {
    (0..layers.len())
        .map(|i| {
            let mut depth = 0;
            let mut current = layers[i].parent;
            while let Some(p) = current {
                if depth >= layers.len() || p >= layers.len() {
                    break;
                }
                depth += 1;
                current = layers[p].parent;
            }
            depth
        })
        .collect()
}

/// What painting needs to know.
struct GridState<'a> {
    geom: TimelineGeom,
    timeline: &'a Timeline,
    rows: usize,
    columns: u32,
    playhead: u32,
    selected_layer: Option<usize>,
    selected_frame: Option<u32>,
    depths: &'a [usize],
    palette: &'a Palette,
}

fn paint_cells(painter: &Painter, s: &GridState<'_>, origin: Pos2, visible: Rect, viewport: Rect) {
    let g = &s.geom;
    let p = s.palette;
    let cells = Rect::from_min_max(
        pos2(
            visible.left() + g.names_width,
            visible.top() + g.ruler_height,
        ),
        visible.max,
    );
    painter.rect_filled(cells, CornerRadius::ZERO, p.cell);
    let frames = g.visible_frames(viewport.left() + g.names_width, viewport.right(), s.columns);
    let rows = g.visible_rows(viewport.top() + g.ruler_height, viewport.bottom(), s.rows);
    let rows_bottom = (origin.y + g.row_top(s.rows)).min(visible.bottom());
    let x = |f: u32| origin.x + g.frame_left(f);
    let painter = painter.with_clip_rect(cells);

    // Every fifth column is shaded, as in Flash.
    for f in frames.clone() {
        if (f + 1) % 5 == 0 {
            painter.rect_filled(
                Rect::from_min_max(pos2(x(f), cells.top()), pos2(x(f + 1), rows_bottom)),
                CornerRadius::ZERO,
                p.fifth,
            );
        }
    }
    // The selected layer's row.
    if let Some(l) = s.selected_layer.filter(|l| rows.contains(l)) {
        let top = origin.y + g.row_top(l);
        painter.rect_filled(
            Rect::from_min_max(
                pos2(cells.left(), top),
                pos2(cells.right(), top + g.row_height),
            ),
            CornerRadius::ZERO,
            p.layer_select,
        );
    }
    for row in rows.clone() {
        let Some(layer) = s.timeline.layers.get(row) else {
            continue;
        };
        let top = origin.y + g.row_top(row);
        for (k, span) in layer.frames.iter().enumerate() {
            if span.end() <= frames.start || span.index >= frames.end {
                continue;
            }
            let next = layer.frames.get(k + 1);
            paint_span(&painter, s, span, next, top, x);
        }
    }
    // Grid lines.
    for f in frames.clone() {
        let lx = x(f);
        painter.line_segment(
            [pos2(lx, cells.top()), pos2(lx, rows_bottom)],
            Stroke::new(1.0, p.grid),
        );
    }
    for row in rows.start..=rows.end {
        let y = origin.y + g.row_top(row);
        painter.line_segment(
            [pos2(cells.left(), y), pos2(cells.right(), y)],
            Stroke::new(1.0, p.row_line),
        );
    }
    // The selected cell.
    if let (Some(l), Some(f)) = (s.selected_layer, s.selected_frame)
        && rows.contains(&l)
        && frames.contains(&f)
    {
        let top = origin.y + g.row_top(l);
        let r = Rect::from_min_size(pos2(x(f), top), vec2(g.cell_width, g.row_height));
        painter.rect_filled(
            r.shrink(1.0),
            CornerRadius::ZERO,
            p.select.gamma_multiply(0.8),
        );
    }
    // The playhead line.
    if frames.contains(&s.playhead) {
        let cx = origin.x + g.frame_center(s.playhead);
        painter.line_segment(
            [pos2(cx, cells.top()), pos2(cx, rows_bottom)],
            Stroke::new(1.0, p.playhead),
        );
    }
}

fn paint_span(
    painter: &Painter,
    s: &GridState<'_>,
    span: &Frame,
    next: Option<&Frame>,
    top: f32,
    x: impl Fn(u32) -> f32,
) {
    let g = &s.geom;
    let p = s.palette;
    let rect = Rect::from_min_max(
        pos2(x(span.index), top),
        pos2(x(span.end()), top + g.row_height),
    );
    let has_content = !span.elements.is_empty();
    let fill = match &span.tween {
        Tween::Motion(_) => Some(p.motion),
        Tween::Shape(_) => Some(p.shape),
        Tween::Other(_) => Some(p.other),
        Tween::None if has_content => Some(p.span),
        Tween::None => None,
    };
    if let Some(fill) = fill {
        painter.rect_filled(rect.shrink2(vec2(0.0, 1.0)), CornerRadius::ZERO, fill);
    }
    // Span outline, as Flash draws a box around each keyframe span.
    painter.rect_stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(1.0, p.row_line),
        StrokeKind::Inside,
    );
    let cy = rect.center().y;
    let first_cx = x(span.index) + g.cell_width * 0.5;
    let radius = (g.cell_width * 0.3).min(g.row_height * 0.2);
    let dot = pos2(first_cx, top + g.row_height * 0.7);

    // Tween arrow to the next keyframe; dashed when the tween is broken
    // (no keyframe right after the span).
    if !matches!(span.tween, Tween::None) && span.duration > 1 {
        let start = pos2(first_cx + radius + 1.0, cy + g.row_height * 0.2);
        let end = pos2(x(span.end()) - 2.0, start.y);
        let stroke = Stroke::new(1.0, p.marker);
        let broken = next.is_none_or(|n| n.index != span.end());
        if broken {
            painter.extend(Shape::dashed_line(&[start, end], stroke, 3.0, 2.0));
        } else {
            painter.line_segment([start, end], stroke);
            painter.add(Shape::convex_polygon(
                vec![end, end + vec2(-4.0, -3.0), end + vec2(-4.0, 3.0)],
                p.marker,
                Stroke::NONE,
            ));
        }
    } else if span.duration > 1 {
        // End of a static span: a small hollow box in its last cell.
        let last = x(span.end() - 1);
        let r = Rect::from_center_size(
            pos2(last + g.cell_width * 0.5, top + g.row_height * 0.62),
            vec2((g.cell_width * 0.55).max(3.0), g.row_height * 0.45),
        );
        painter.rect_stroke(
            r,
            CornerRadius::ZERO,
            Stroke::new(1.0, p.marker),
            StrokeKind::Inside,
        );
    }

    // Keyframe dot: filled with content, hollow when blank.
    if has_content {
        painter.circle_filled(dot, radius, p.marker);
    } else {
        painter.circle_stroke(dot, radius, Stroke::new(1.0, p.marker));
    }

    // Script mark and label.
    let mut text_x = x(span.index) + g.cell_width + 2.0;
    if span.script.is_some() {
        painter.text(
            pos2(x(span.index) + 1.0, top + 1.0),
            Align2::LEFT_TOP,
            "a",
            FontId::proportional(g.row_height * 0.45),
            p.text,
        );
    }
    if let Some(label) = &span.label {
        let color = match label.kind {
            LabelKind::Name => p.label,
            LabelKind::Comment => p.comment,
            LabelKind::Anchor => p.anchor,
        };
        icons::label_flag(painter, pos2(text_x, top + 3.0), g.row_height * 0.6, color);
        text_x += g.row_height * 0.4;
        let room = Rect::from_min_max(pos2(text_x, top), pos2(rect.right() - 1.0, rect.bottom()));
        if room.width() > 6.0 {
            let shown = match label.kind {
                LabelKind::Comment => format!("// {}", label.name),
                _ => label.name.clone(),
            };
            painter.with_clip_rect(room).text(
                pos2(room.left() + 1.0, room.center().y),
                Align2::LEFT_CENTER,
                shown,
                FontId::proportional(g.row_height * 0.55),
                p.text,
            );
        }
    }
}

fn paint_ruler(painter: &Painter, s: &GridState<'_>, origin: Pos2, visible: Rect, viewport: Rect) {
    let g = &s.geom;
    let p = s.palette;
    let ruler = Rect::from_min_max(
        pos2(visible.left() + g.names_width, visible.top()),
        pos2(visible.right(), visible.top() + g.ruler_height),
    );
    let painter = painter.with_clip_rect(ruler);
    painter.rect_filled(ruler, CornerRadius::ZERO, p.chrome);
    let frames = g.visible_frames(viewport.left() + g.names_width, viewport.right(), s.columns);
    let font = FontId::proportional((g.ruler_height * 0.5).max(9.0));
    for f in frames {
        let lx = origin.x + g.frame_left(f);
        let tick = if (f + 1) % 5 == 0 { 0.45 } else { 0.2 };
        painter.line_segment(
            [
                pos2(lx, ruler.bottom() - g.ruler_height * tick),
                pos2(lx, ruler.bottom()),
            ],
            Stroke::new(1.0, p.weak),
        );
        if f == 0 || (f + 1) % 5 == 0 {
            painter.text(
                pos2(lx + 1.0, ruler.top() + 1.0),
                Align2::LEFT_TOP,
                (f + 1).to_string(),
                font.clone(),
                p.weak,
            );
        }
    }
    painter.line_segment(
        [ruler.left_bottom(), ruler.right_bottom()],
        Stroke::new(1.0, p.row_line),
    );
    // The playhead's box with its frame number.
    let cx = origin.x + g.frame_center(s.playhead);
    let label = (s.playhead + 1).to_string();
    let galley = painter.layout_no_wrap(label, font, Color32::WHITE);
    let w = (galley.size().x + 6.0).max(g.cell_width);
    let r = Rect::from_center_size(
        pos2(cx, ruler.center().y + 1.0),
        vec2(w, g.ruler_height - 4.0),
    );
    painter.rect_filled(r, CornerRadius::same(2), p.playhead);
    painter.galley(r.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

fn paint_names(
    ctx: &egui::Context,
    painter: &Painter,
    shaped: &mut ShapedText,
    s: &GridState<'_>,
    origin: Pos2,
    visible: Rect,
    viewport: Rect,
) {
    let g = &s.geom;
    let p = s.palette;
    let column = Rect::from_min_max(
        visible.min,
        pos2(visible.left() + g.names_width, visible.bottom()),
    );
    let painter = painter.with_clip_rect(column);
    painter.rect_filled(column, CornerRadius::ZERO, p.chrome);
    let icon_size = (g.row_height - 6.0).clamp(10.0, 16.0);
    let flags_w = icon_size + 4.0;
    // Column headers in the corner: eye, lock, outline.
    let header_top = visible.top();
    for (i, icon) in [Icon::Hidden, Icon::Locked, Icon::Outline]
        .into_iter()
        .enumerate()
    {
        let right = column.right() - flags_w * (2 - i) as f32;
        let r = Rect::from_min_size(
            pos2(right - flags_w, header_top),
            vec2(flags_w, g.ruler_height),
        );
        icons::paint(&painter, r.shrink(3.0), icon, p.weak);
    }
    let rows = g.visible_rows(viewport.top() + g.ruler_height, viewport.bottom(), s.rows);
    let body = painter.with_clip_rect(Rect::from_min_max(
        pos2(column.left(), column.top() + g.ruler_height),
        column.max,
    ));
    let font_size = (g.row_height * 0.62).clamp(10.0, 16.0);
    for row in rows {
        let Some(layer) = s.timeline.layers.get(row) else {
            continue;
        };
        let top = origin.y + g.row_top(row);
        let r = Rect::from_min_size(pos2(column.left(), top), vec2(g.names_width, g.row_height));
        if s.selected_layer == Some(row) {
            body.rect_filled(r, CornerRadius::ZERO, p.select);
        }
        body.line_segment(
            [r.left_bottom(), r.right_bottom()],
            Stroke::new(1.0, p.row_line),
        );
        let indent = s.depths.get(row).copied().unwrap_or(0).min(8) as f32 * 12.0;
        let icon_rect = Rect::from_min_size(
            pos2(r.left() + 4.0 + indent, r.center().y - icon_size / 2.0),
            Vec2::splat(icon_size),
        );
        icons::paint(&body, icon_rect, kind_icon(&layer.kind), p.text);
        // Flags: hidden, locked, outline (in the layer's colour).
        let flag = |i: usize| {
            Rect::from_min_size(
                pos2(r.right() - flags_w * (3 - i) as f32, r.top()),
                vec2(flags_w, g.row_height),
            )
            .shrink(3.0)
        };
        icons::paint(
            &body,
            flag(0),
            if layer.visible {
                Icon::Dot
            } else {
                Icon::Hidden
            },
            p.weak,
        );
        icons::paint(
            &body,
            flag(1),
            if layer.locked {
                Icon::Locked
            } else {
                Icon::Dot
            },
            p.text,
        );
        let swatch = Rect::from_center_size(flag(2).center(), Vec2::splat(icon_size * 0.6));
        let c = layer.color;
        let layer_color = Color32::from_rgb(c.r, c.g, c.b);
        if layer.outline {
            body.rect_filled(swatch, CornerRadius::ZERO, layer_color);
        } else {
            body.rect_stroke(
                swatch,
                CornerRadius::ZERO,
                Stroke::new(1.0, layer_color),
                StrokeKind::Inside,
            );
        }
        // The name, shaped when it is right-to-left, clipped before the
        // flags.
        let name_left = icon_rect.right() + 4.0;
        let name_clip = Rect::from_min_max(
            pos2(name_left, r.top()),
            pos2(r.right() - flags_w * 3.0 - 2.0, r.bottom()),
        );
        if name_clip.width() > 4.0 {
            let name_painter = body.with_clip_rect(name_clip.intersect(body.clip_rect()));
            let color = if layer.visible { p.text } else { p.weak };
            let baseline = r.center().y + font_size * BASELINE_BELOW_CENTER;
            shaped.paint(
                ctx,
                &name_painter,
                name_left,
                baseline,
                &layer.name,
                font_size,
                color,
            );
        }
    }
    painter.line_segment(
        [column.right_top(), column.right_bottom()],
        Stroke::new(1.0, p.row_line),
    );
}

fn kind_icon(kind: &LayerKind) -> Icon {
    match kind {
        LayerKind::Normal => Icon::Layer,
        LayerKind::Guide => Icon::Guide,
        LayerKind::Mask => Icon::Mask,
        LayerKind::Folder => Icon::Folder,
        LayerKind::Other(_) => Icon::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depths_follow_parents_and_survive_cycles() {
        let layer = |parent: Option<usize>| Layer {
            parent,
            ..Layer::default()
        };
        let layers = vec![
            layer(None),
            layer(Some(0)),
            layer(Some(1)),
            layer(Some(4)),
            layer(Some(3)),
        ];
        let d = layer_depths(&layers);
        assert_eq!(&d[..3], &[0, 1, 2]);
        assert!(d[3] <= layers.len() && d[4] <= layers.len());
        assert_eq!(layer_depths(&[layer(Some(99))]), vec![0]);
    }
}
