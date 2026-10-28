//! The dock: which panels exist, the default layout, and the tab viewer that
//! draws each panel.

use eframe::{egui, egui_wgpu};
use egui_dock::{DockState, NodeIndex, TabViewer, Translations};
use serde::{Deserialize, Serialize};

use crate::panels;
use crate::workspace::Workspace;

/// A panel of the dock. The names are stored in the saved layout, so keep
/// them stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    /// The stage view.
    Stage,
    /// The timeline.
    Timeline,
    /// The library.
    Library,
    /// Document, layer and frame properties.
    Properties,
    /// The selected frame's script.
    Actions,
    /// Traces and diagnostics.
    Output,
    /// The Test Movie player.
    TestMovie,
}

impl Tab {
    /// Every panel, in Window-menu order.
    pub const ALL: [Tab; 7] = [
        Tab::Stage,
        Tab::Timeline,
        Tab::Library,
        Tab::Properties,
        Tab::Actions,
        Tab::Output,
        Tab::TestMovie,
    ];

    /// The panel's title.
    pub fn title(self) -> &'static str {
        match self {
            Tab::Stage => tr!("tab.stage"),
            Tab::Timeline => tr!("tab.timeline"),
            Tab::Library => tr!("tab.library"),
            Tab::Properties => tr!("tab.properties"),
            Tab::Actions => tr!("tab.actions"),
            Tab::Output => tr!("tab.output"),
            Tab::TestMovie => tr!("tab.test_movie"),
        }
    }
}

/// The default layout: the timeline on top; the stage (with Test Movie
/// behind it) in the middle with the library and properties at its end;
/// actions and output along the bottom.
pub fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Stage, Tab::TestMovie]);
    let surface = dock.main_surface_mut();
    // egui_dock gives `fraction` to the first child (the top or left one),
    // whichever is new: here the timeline gets the top third.
    let [center, _timeline] = surface.split_above(NodeIndex::root(), 0.33, vec![Tab::Timeline]);
    let [center, bottom] = surface.split_below(center, 0.66, vec![Tab::Actions]);
    let [_actions, _output] = surface.split_right(bottom, 0.45, vec![Tab::Output]);
    let [_stage, side] = surface.split_right(center, 0.70, vec![Tab::Library]);
    let [_library, _properties] = surface.split_below(side, 0.5, vec![Tab::Properties]);
    dock.with_translations(translations())
}

/// egui_dock's own labels, through the string table.
pub fn translations() -> Translations {
    let mut t = Translations::english();
    t.tab_context_menu.close_button = tr!("dock.close").to_owned();
    t.tab_context_menu.eject_button = tr!("dock.eject").to_owned();
    t.tab_context_menu.hide_tab_bar_button = tr!("dock.hide_tab_bar").to_owned();
    t.tab_context_menu.show_tab_bar_button = tr!("dock.show_tab_bar").to_owned();
    t.leaf.close_button_disabled_tooltip = tr!("dock.close_disabled").to_owned();
    t.leaf.close_all_button = tr!("dock.close_window").to_owned();
    t.leaf.close_all_button_menu_hint = tr!("dock.close_window_menu_hint").to_owned();
    t.leaf.close_all_button_modifier_hint = tr!("dock.close_window_modifier_hint").to_owned();
    t.leaf.close_all_button_modifier_menu_hint =
        tr!("dock.close_window_modifier_menu_hint").to_owned();
    t.leaf.close_all_button_disabled_tooltip = tr!("dock.close_window_disabled").to_owned();
    t.leaf.minimize_button = tr!("dock.minimize").to_owned();
    t.leaf.minimize_button_menu_hint = tr!("dock.minimize_menu_hint").to_owned();
    t.leaf.minimize_button_modifier_hint = tr!("dock.minimize_modifier_hint").to_owned();
    t.leaf.minimize_button_modifier_menu_hint = tr!("dock.minimize_modifier_menu_hint").to_owned();
    t
}

/// A saved layout, or the default one when there is none or it lacks the
/// panels that cannot be closed.
pub fn restore(saved: Option<DockState<Tab>>) -> DockState<Tab> {
    match saved {
        Some(dock)
            if dock.find_tab(&Tab::Stage).is_some() && dock.find_tab(&Tab::Timeline).is_some() =>
        {
            dock.with_translations(translations())
        }
        _ => default_layout(),
    }
}

/// Brings `tab` to the front, adding it back if it was closed (Test Movie
/// next to the stage, the rest in the focused leaf).
pub fn focus(dock: &mut DockState<Tab>, tab: Tab) {
    if dock.find_tab(&tab).is_none() {
        add(dock, tab);
    }
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
        dock.set_focused_node_and_surface(path.node_path());
    }
}

/// Shows or closes a panel (Window menu).
pub fn toggle(dock: &mut DockState<Tab>, tab: Tab) {
    match dock.find_tab(&tab) {
        Some(path) if !matches!(tab, Tab::Stage | Tab::Timeline) => {
            dock.remove_tab(path);
        }
        Some(_) => focus(dock, tab),
        None => focus(dock, tab),
    }
}

fn add(dock: &mut DockState<Tab>, tab: Tab) {
    let anchor = match tab {
        Tab::TestMovie => Tab::Stage,
        Tab::Actions => Tab::Output,
        Tab::Output => Tab::Actions,
        Tab::Library => Tab::Properties,
        Tab::Properties => Tab::Library,
        Tab::Stage | Tab::Timeline => Tab::Stage,
    };
    match dock.find_tab(&anchor) {
        Some(path) => {
            if let Ok(leaf) = dock.leaf_mut(path.node_path()) {
                leaf.append_tab(tab);
            }
        }
        None => dock.push_to_first_leaf(tab),
    }
}

/// Draws the panels.
pub struct Viewer<'a> {
    /// The application state.
    pub ws: &'a mut Workspace,
    /// eframe's wgpu state, for the players' textures.
    pub rs: Option<&'a egui_wgpu::RenderState>,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(("jazari-tab", *tab))
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Stage => panels::stage::show(ui, self.ws, self.rs),
            Tab::Timeline => panels::timeline::show(ui, self.ws),
            Tab::Library => panels::library::show(ui, self.ws),
            Tab::Properties => panels::properties::show(ui, self.ws),
            Tab::Actions => panels::actions::show(ui, self.ws),
            Tab::Output => panels::output::show(ui, self.ws),
            Tab::TestMovie => panels::test_movie::show(ui, self.ws, self.rs),
        }
    }

    fn is_closeable(&self, tab: &Tab) -> bool {
        !matches!(tab, Tab::Stage | Tab::Timeline)
    }

    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        // Every panel manages its own scrolling.
        [false, false]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_has_every_panel_once() {
        let dock = default_layout();
        for tab in Tab::ALL {
            let count = dock.iter_all_tabs().filter(|(_, t)| **t == tab).count();
            assert_eq!(count, 1, "{tab:?}");
        }
    }

    #[test]
    fn closed_panels_come_back_and_fixed_ones_stay() {
        let mut dock = default_layout();
        toggle(&mut dock, Tab::Library);
        assert!(dock.find_tab(&Tab::Library).is_none());
        toggle(&mut dock, Tab::Stage);
        assert!(dock.find_tab(&Tab::Stage).is_some());
        focus(&mut dock, Tab::Library);
        assert!(dock.find_tab(&Tab::Library).is_some());
        let mut dock = default_layout();
        if let Some(p) = dock.find_tab(&Tab::TestMovie) {
            dock.remove_tab(p);
        }
        focus(&mut dock, Tab::TestMovie);
        let movie = dock.find_tab(&Tab::TestMovie).expect("added back");
        let stage = dock.find_tab(&Tab::Stage).expect("stage");
        assert_eq!(movie.node_path(), stage.node_path(), "next to the stage");
    }

    #[test]
    fn bad_saved_layouts_are_replaced() {
        assert!(restore(None).find_tab(&Tab::Stage).is_some());
        let only_output = DockState::new(vec![Tab::Output]);
        let restored = restore(Some(only_output));
        assert!(restored.find_tab(&Tab::Timeline).is_some());
        let good = restore(Some(default_layout()));
        assert_eq!(good.iter_all_tabs().count(), Tab::ALL.len());
        assert_eq!(
            good.translations.tab_context_menu.close_button,
            tr!("dock.close")
        );
    }
}
