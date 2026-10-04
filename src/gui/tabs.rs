//! Top tab bar switching between the graph canvas and the watch list.

use egui::Ui;

/// The main views available once a tree is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTab {
    #[default]
    Graph,
    WatchList,
}

/// Renders the tab row and updates `active` when a tab is clicked.
pub fn show(ui: &mut Ui, active: &mut AppTab) {
    ui.horizontal(|ui| {
        ui.selectable_value(active, AppTab::Graph, "Graph View");
        ui.selectable_value(active, AppTab::WatchList, "Watch List");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn default_tab_is_the_graph() {
        assert_eq!(AppTab::default(), AppTab::Graph);
    }

    #[test]
    fn tab_bar_renders_headless() {
        let ctx = Context::default();
        let mut active = AppTab::Graph;
        let mut output = ctx.run_ui(RawInput::default(), |ui| show(ui, &mut active));
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the tab row paints");
        assert_eq!(active, AppTab::Graph, "no input keeps the active tab");
    }
}
