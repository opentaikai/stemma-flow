//! Top tab bar switching between the graph canvas and the watch list.

use egui::Ui;

use crate::i18n::tr;

/// The main views available once a tree is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTab {
    #[default]
    Graph,
    WatchList,
    Couples,
    Associations,
    Places,
    Media,
    Research,
    FactTypes,
    Sources,
    Repositories,
}

/// Renders the tab row and updates `active` when a tab is clicked.
pub fn show(ui: &mut Ui, active: &mut AppTab) {
    ui.horizontal(|ui| {
        ui.selectable_value(active, AppTab::Graph, tr("tabs.graph"));
        ui.selectable_value(active, AppTab::WatchList, tr("tabs.people"));
        ui.selectable_value(active, AppTab::Couples, tr("tabs.couples"));
        ui.selectable_value(active, AppTab::Associations, tr("tabs.associations"));
        ui.selectable_value(active, AppTab::Places, tr("tabs.places"));
        ui.selectable_value(active, AppTab::Media, tr("tabs.media"));
        ui.selectable_value(active, AppTab::Research, tr("tabs.research"));
        ui.selectable_value(active, AppTab::FactTypes, tr("tabs.fact_types"));
        ui.selectable_value(active, AppTab::Sources, tr("tabs.sources"));
        ui.selectable_value(active, AppTab::Repositories, tr("tabs.repositories"));
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
