//! Graph mode controls for the ancestor-focused pedigree chart.

use egui::Ui;

use crate::i18n::tr;

/// Which graph is currently displayed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GraphMode {
    #[default]
    All,
    Pedigree,
}

/// Ancestor levels including the root person.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PedigreeGenerations {
    #[default]
    Five,
    Six,
}

impl PedigreeGenerations {
    pub fn levels(self) -> usize {
        match self {
            Self::Five => 5,
            Self::Six => 6,
        }
    }
}

/// Persisted-in-session pedigree state.
#[derive(Debug, Default)]
pub struct PedigreeState {
    pub mode: GraphMode,
    pub root_id: Option<String>,
    pub generations: PedigreeGenerations,
}

/// Draws the graph-mode toolbar; returns true when the scene must rebuild.
pub fn show_toolbar(
    ui: &mut Ui,
    state: &mut PedigreeState,
    selected: Option<&str>,
    root_label: Option<&str>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(tr("pedigree.view"));
        let mut mode = state.mode;
        ui.selectable_value(&mut mode, GraphMode::All, tr("pedigree.all"));
        ui.selectable_value(&mut mode, GraphMode::Pedigree, tr("pedigree.pedigree"));
        if mode != state.mode {
            state.mode = mode;
            if mode == GraphMode::Pedigree && state.root_id.is_none() {
                state.root_id = selected.map(str::to_string);
            }
            changed = true;
        }

        if state.mode == GraphMode::Pedigree {
            ui.separator();
            ui.label(tr("pedigree.generations"));
            let mut generations = state.generations;
            ui.selectable_value(&mut generations, PedigreeGenerations::Five, "5");
            ui.selectable_value(&mut generations, PedigreeGenerations::Six, "6");
            if generations != state.generations {
                state.generations = generations;
                changed = true;
            }
            ui.separator();
            ui.label(tr("pedigree.root"));
            match root_label {
                Some(label) => ui.label(label),
                None => ui.label(tr("pedigree.no_root")),
            };
            if ui
                .add_enabled(
                    selected.is_some(),
                    egui::Button::new(tr("pedigree.set_root")),
                )
                .clicked()
                && let Some(selected) = selected
                && state.root_id.as_deref() != Some(selected)
            {
                state.root_id = Some(selected.to_string());
                changed = true;
            }
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn generation_counts_include_the_root() {
        assert_eq!(PedigreeGenerations::Five.levels(), 5);
        assert_eq!(PedigreeGenerations::Six.levels(), 6);
        assert_eq!(PedigreeGenerations::default(), PedigreeGenerations::Five);
    }

    #[test]
    fn toolbar_renders_headless() {
        let ctx = Context::default();
        let mut state = PedigreeState::default();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show_toolbar(ui, &mut state, Some("p1"), Some("Ada King"));
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
