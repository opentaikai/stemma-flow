//! Research workspace: task log and static people groups.

use egui::Ui;

use crate::db::Person;
use crate::gui::groups::{GroupEvent, GroupsState, show as show_groups};
use crate::gui::problems::{self as problems_ui, ProblemsEvent, ProblemsState};
use crate::gui::tasks::{TaskEvent, TasksState, show as show_tasks};

/// The two research panes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResearchTab {
    #[default]
    Tasks,
    Groups,
    Problems,
}

/// State owned by the Research tab.
#[derive(Debug, Default)]
pub struct ResearchState {
    pub tab: ResearchTab,
    pub tasks: TasksState,
    pub groups: GroupsState,
    pub problems: ProblemsState,
}

/// An event from either research pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResearchEvent {
    Task(TaskEvent),
    Group(GroupEvent),
    Problem(ProblemsEvent),
}

/// Draws the Research tab.
pub fn show(
    ui: &mut Ui,
    state: &mut ResearchState,
    people: &[Person],
    surname_first: bool,
) -> Option<ResearchEvent> {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.tab, ResearchTab::Tasks, "Tasks");
        ui.selectable_value(&mut state.tab, ResearchTab::Groups, "Groups");
        ui.selectable_value(
            &mut state.tab,
            ResearchTab::Problems,
            crate::i18n::tr("problems.tab"),
        );
    });
    ui.separator();
    match state.tab {
        ResearchTab::Tasks => show_tasks(ui, &mut state.tasks).map(ResearchEvent::Task),
        ResearchTab::Groups => {
            show_groups(ui, &mut state.groups, people, surname_first).map(ResearchEvent::Group)
        }
        ResearchTab::Problems => problems_ui::show(ui, &mut state.problems, people, surname_first)
            .map(ResearchEvent::Problem),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn research_tab_renders_both_panes() {
        let ctx = Context::default();
        let mut state = ResearchState::default();
        let people = vec![Person::new("Ada", "King", "F")];
        for tab in [
            ResearchTab::Tasks,
            ResearchTab::Groups,
            ResearchTab::Problems,
        ] {
            state.tab = tab;
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                show(ui, &mut state, &people, false);
            });
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty(), "{tab:?} paints");
        }
    }
}
