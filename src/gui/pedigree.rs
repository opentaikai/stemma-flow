//! Graph mode controls for the ancestor-focused pedigree chart.

use egui::Ui;

use crate::i18n::tr;

/// Which graph is currently displayed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GraphMode {
    #[default]
    All,
    Pedigree,
    Family,
    Descendants,
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
    pub family_root_id: Option<String>,
    pub family_id: Option<String>,
    pub descendant_root_id: Option<String>,
    pub descendant_generations: DescendantGenerations,
}

/// Descendant levels including the root.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DescendantGenerations {
    Two,
    Three,
    #[default]
    Four,
    Five,
    Six,
    Seven,
}

impl DescendantGenerations {
    pub fn levels(self) -> usize {
        match self {
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
        }
    }
}

/// Draws the graph-mode toolbar; returns true when the scene must rebuild.
pub fn show_toolbar(
    ui: &mut Ui,
    state: &mut PedigreeState,
    selected: Option<&str>,
    pedigree_root_label: Option<&str>,
    family_root_label: Option<&str>,
    family_options: &[(String, String)],
    descendant_root_label: Option<&str>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(tr("pedigree.view"));
        let mut mode = state.mode;
        ui.selectable_value(&mut mode, GraphMode::All, tr("pedigree.all"));
        ui.selectable_value(&mut mode, GraphMode::Pedigree, tr("pedigree.pedigree"));
        ui.selectable_value(&mut mode, GraphMode::Family, tr("family.mode"));
        ui.selectable_value(&mut mode, GraphMode::Descendants, tr("descendants.mode"));
        if mode != state.mode {
            state.mode = mode;
            if mode == GraphMode::Pedigree && state.root_id.is_none() {
                state.root_id = selected.map(str::to_string);
            }
            if mode == GraphMode::Family && state.family_root_id.is_none() {
                state.family_root_id = selected.map(str::to_string);
            }
            if mode == GraphMode::Descendants && state.descendant_root_id.is_none() {
                state.descendant_root_id = selected.map(str::to_string);
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
            match pedigree_root_label {
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
        } else if state.mode == GraphMode::Family {
            ui.separator();
            ui.label(tr("family.root"));
            match family_root_label {
                Some(label) => ui.label(label),
                None => ui.label(tr("family.no_root")),
            };
            if ui
                .add_enabled(selected.is_some(), egui::Button::new(tr("family.set_root")))
                .clicked()
                && let Some(selected) = selected
                && state.family_root_id.as_deref() != Some(selected)
            {
                state.family_root_id = Some(selected.to_string());
                state.family_id = None;
                changed = true;
            }
            if !family_options.is_empty() {
                ui.separator();
                ui.label(tr("family.family"));
                let current = family_options
                    .iter()
                    .find(|(id, _)| state.family_id.as_deref() == Some(id.as_str()))
                    .map(|(_, label)| label.clone())
                    .unwrap_or_else(|| family_options[0].1.clone());
                egui::ComboBox::from_id_salt("family_view_family")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (id, label) in family_options {
                            if ui
                                .selectable_label(
                                    state.family_id.as_deref() == Some(id.as_str()),
                                    label,
                                )
                                .clicked()
                            {
                                state.family_id = Some(id.clone());
                                changed = true;
                            }
                        }
                    });
            }
        } else if state.mode == GraphMode::Descendants {
            ui.separator();
            ui.label(tr("descendants.generations"));
            egui::ComboBox::from_id_salt("descendant_generations")
                .selected_text(state.descendant_generations.label())
                .show_ui(ui, |ui| {
                    for generations in [
                        DescendantGenerations::Two,
                        DescendantGenerations::Three,
                        DescendantGenerations::Four,
                        DescendantGenerations::Five,
                        DescendantGenerations::Six,
                        DescendantGenerations::Seven,
                    ] {
                        if ui
                            .selectable_value(
                                &mut state.descendant_generations,
                                generations,
                                generations.label(),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            ui.separator();
            ui.label(tr("descendants.root"));
            match descendant_root_label {
                Some(label) => ui.label(label),
                None => ui.label(tr("descendants.no_root")),
            };
            if ui
                .add_enabled(
                    selected.is_some(),
                    egui::Button::new(tr("descendants.set_root")),
                )
                .clicked()
                && let Some(selected) = selected
                && state.descendant_root_id.as_deref() != Some(selected)
            {
                state.descendant_root_id = Some(selected.to_string());
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
    fn descendant_generations_cover_two_to_seven_and_default_to_four() {
        assert_eq!(
            DescendantGenerations::default(),
            DescendantGenerations::Four
        );
        for (generations, levels) in [
            (DescendantGenerations::Two, 2),
            (DescendantGenerations::Three, 3),
            (DescendantGenerations::Four, 4),
            (DescendantGenerations::Five, 5),
            (DescendantGenerations::Six, 6),
            (DescendantGenerations::Seven, 7),
        ] {
            assert_eq!(generations.levels(), levels);
        }
    }

    #[test]
    fn toolbar_renders_headless() {
        let ctx = Context::default();
        let mut state = PedigreeState::default();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show_toolbar(
                ui,
                &mut state,
                Some("p1"),
                Some("Ada King"),
                None,
                &[],
                None,
            );
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
