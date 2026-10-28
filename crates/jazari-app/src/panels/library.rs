//! The Library panel: folders and items with their kinds. Clicking selects
//! an item (its details show in Properties); double-clicking a symbol, or
//! its Edit button, opens its timeline in the Timeline panel.

use eframe::egui::{self, CornerRadius, RichText, ScrollArea, Sense, Ui, Vec2, pos2};

use crate::doc::TimelineRef;
use crate::dock::Tab;
use crate::icons::{self, Icon};
use crate::library::{LibKind, LibNode, item_count};
use crate::panels::centered_message;
use crate::shaped::{BASELINE_BELOW_CENTER, ShapedText};
use crate::workspace::Workspace;

enum Action {
    Select(String),
    Open(String),
}

/// Draws the panel.
pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    if ws.doc.is_none() {
        centered_message(ui, tr!("library.no_document"), None);
        return;
    }
    let mut action = None;
    {
        let Workspace {
            library,
            shaped,
            selected_item,
            ..
        } = ws;
        ui.horizontal(|ui| {
            let count = item_count(library);
            let text = if count == 1 {
                tr!("library.count_one").to_owned()
            } else {
                tr!("library.count", count = count)
            };
            ui.label(RichText::new(text).weak());
            let can_edit = selected_item
                .as_deref()
                .is_some_and(|s| is_symbol(library, s));
            if ui
                .add_enabled(can_edit, egui::Button::new(tr!("library.edit")))
                .on_hover_text(tr!("library.edit_hover"))
                .clicked()
                && let Some(item) = selected_item.clone()
            {
                action = Some(Action::Open(item));
            }
        });
        ui.separator();
        if library.is_empty() {
            centered_message(ui, tr!("library.empty"), None);
        } else {
            ScrollArea::vertical()
                .id_salt("library-tree")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for node in library.iter() {
                        node_ui(ui, shaped, node, selected_item.as_deref(), &mut action);
                    }
                });
        }
    }
    match action {
        Some(Action::Select(path)) => ws.selected_item = Some(path),
        Some(Action::Open(path)) => {
            ws.selected_item = Some(path.clone());
            ws.show_timeline(TimelineRef::Symbol(path));
            ws.focus_request = Some(Tab::Timeline);
        }
        None => {}
    }
}

fn is_symbol(nodes: &[LibNode], path: &str) -> bool {
    nodes
        .iter()
        .any(|n| (n.path == path && n.kind.is_symbol()) || is_symbol(&n.children, path))
}

fn node_ui(
    ui: &mut Ui,
    shaped: &mut ShapedText,
    node: &LibNode,
    selected: Option<&str>,
    action: &mut Option<Action>,
) {
    if node.kind == LibKind::Folder {
        let id = ui.make_persistent_id(("library-folder", &node.path));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| {
                item_row(ui, shaped, Icon::Folder, &node.name, false);
            })
            .body(|ui| {
                for child in &node.children {
                    node_ui(ui, shaped, child, selected, action);
                }
            });
        return;
    }
    let icon = match node.kind {
        LibKind::MovieClip => Icon::MovieClip,
        LibKind::Graphic => Icon::Graphic,
        LibKind::Button => Icon::Button,
        LibKind::Bitmap => Icon::Bitmap,
        LibKind::Font => Icon::Font,
        LibKind::Folder => Icon::Folder,
    };
    let response = item_row(
        ui,
        shaped,
        icon,
        &node.name,
        selected == Some(node.path.as_str()),
    );
    let response = response.on_hover_text(kind_name(node.kind));
    if response.double_clicked() && node.kind.is_symbol() {
        *action = Some(Action::Open(node.path.clone()));
    } else if response.clicked() {
        *action = Some(Action::Select(node.path.clone()));
    }
}

/// The kind's name.
pub fn kind_name(kind: LibKind) -> &'static str {
    match kind {
        LibKind::Folder => tr!("library.folder"),
        LibKind::MovieClip => tr!("library.movie_clip"),
        LibKind::Graphic => tr!("library.graphic"),
        LibKind::Button => tr!("library.button"),
        LibKind::Bitmap => tr!("library.bitmap"),
        LibKind::Font => tr!("library.font"),
    }
}

/// One selectable row: icon and (shaped) name.
fn item_row(
    ui: &mut Ui,
    shaped: &mut ShapedText,
    icon: Icon,
    name: &str,
    selected: bool,
) -> egui::Response {
    let size = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .map_or(14.0, |f| f.size);
    let height = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
    let width = ui.available_width().max(40.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        if selected {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(2), visuals.selection.bg_fill);
        } else if response.hovered() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(2),
                visuals.widgets.hovered.weak_bg_fill,
            );
        }
        let text_color = if selected {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        };
        let icon_rect = egui::Rect::from_min_size(
            pos2(rect.left() + 2.0, rect.center().y - 8.0),
            Vec2::splat(16.0),
        );
        icons::paint(ui.painter(), icon_rect, icon, visuals.text_color());
        let painter = ui.painter_at(rect);
        let ctx = ui.ctx().clone();
        let baseline = rect.center().y + size * BASELINE_BELOW_CENTER;
        shaped.paint(
            &ctx,
            &painter,
            icon_rect.right() + 5.0,
            baseline,
            name,
            size,
            text_color,
        );
    }
    response
}
