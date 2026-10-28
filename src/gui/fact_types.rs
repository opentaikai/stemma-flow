//! Fact Types tab: custom facts, GEDCOM tags and per-type roles.

use std::cmp::Ordering;

use crate::i18n::fact_label;
use egui::{RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::db::facts::{
    FactRoleDraft, FactTypeDraft, FactTypeRecord, FactTypeSummary, suggested_key,
};
use crate::model::fact::{FactKind, FactScope};

/// What the user asked the shell to do from the Fact Types tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactTypesEvent {
    New,
    Select(String),
    Save(Box<FactTypeDraft>),
    Delete(String),
}

/// Columns the grid header can sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Label,
    Category,
    Events,
}

impl SortColumn {
    fn label(self) -> &'static str {
        match self {
            Self::Label => "Label",
            Self::Category => "Category",
            Self::Events => "Events",
        }
    }
}

/// State owned by the window while the Fact Types tab is open.
#[derive(Debug, Default)]
pub struct FactTypesState {
    pub types: Vec<FactTypeSummary>,
    pub selected: Option<String>,
    pub draft: FactTypeDraft,
    pub builtin: bool,
    pub auto_tag: bool,
    pub add_role: String,
    pub filter: String,
    pub sort: Option<(SortColumn, bool)>,
    pub editing: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

/// Converts a stored fact type into an editable draft.
pub fn draft_from_record(record: &FactTypeRecord) -> FactTypeDraft {
    FactTypeDraft {
        key: Some(record.key.clone()),
        label: record.label.clone(),
        category: record.category.clone().unwrap_or_default(),
        gedcom_tag: record.gedcom_tag.clone().unwrap_or_default(),
        scope: record.scope,
        kind: record.kind,
        roles: record
            .roles
            .iter()
            .map(|role| FactRoleDraft {
                key: Some(role.role_key.clone()),
                label: role.label.clone(),
            })
            .collect(),
    }
}

/// Draws the Fact Types tab and returns the action the window should apply.
pub fn show(ui: &mut Ui, state: &mut FactTypesState) -> Option<FactTypesEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text("Filter label, category or tag…"),
        );
        if ui
            .add_enabled(!state.filter.is_empty(), egui::Button::new("Clear"))
            .clicked()
        {
            state.filter.clear();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("+ New Fact Type").strong()))
                .clicked()
            {
                event = Some(FactTypesEvent::New);
            }
        });
    });

    let visible = visible_indices(state);
    ui.label(
        RichText::new(format!(
            "Showing {} of {} fact types",
            visible.len(),
            state.types.len()
        ))
        .weak()
        .size(12.0),
    );

    let available = ui.available_height();
    let grid_height = (available * 0.45).max(140.0).min(available.max(0.0));
    ui.allocate_ui(egui::vec2(ui.available_width(), grid_height), |ui| {
        if state.types.is_empty() {
            ui.label(
                RichText::new("No fact types yet — create one with the button above.")
                    .weak()
                    .size(13.0),
            );
            return;
        }
        let mut sort_clicked = None;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::auto().at_least(170.0))
            .column(Column::auto().at_least(100.0))
            .column(Column::auto().at_least(70.0))
            .column(Column::auto().at_least(80.0))
            .column(Column::auto().at_least(100.0))
            .column(Column::auto().at_least(60.0))
            .column(Column::auto().at_least(60.0))
            .column(Column::auto().at_least(110.0))
            .header(22.0, |mut header| {
                for column in [SortColumn::Label, SortColumn::Category] {
                    header.col(|ui| {
                        let arrow = match state.sort {
                            Some((active, true)) if active == column => " ▲",
                            Some((active, false)) if active == column => " ▼",
                            _ => "",
                        };
                        let strong = state.sort.map(|(active, _)| active) == Some(column);
                        let text = format!("{}{arrow}", column.label());
                        let label = egui::Label::new(if strong {
                            RichText::new(text).strong()
                        } else {
                            RichText::new(text)
                        })
                        .sense(Sense::click());
                        if ui.add(label).clicked() {
                            sort_clicked = Some(column);
                        }
                    });
                }
                for title in ["Scope", "Kind", "GEDCOM tag", "Roles"] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).strong());
                    });
                }
                header.col(|ui| {
                    let arrow = match state.sort {
                        Some((SortColumn::Events, true)) => " ▲",
                        Some((SortColumn::Events, false)) => " ▼",
                        _ => "",
                    };
                    let strong = state.sort.map(|(active, _)| active) == Some(SortColumn::Events);
                    let text = format!("Events{arrow}");
                    let label = egui::Label::new(if strong {
                        RichText::new(text).strong()
                    } else {
                        RichText::new(text)
                    })
                    .sense(Sense::click());
                    if ui.add(label).clicked() {
                        sort_clicked = Some(SortColumn::Events);
                    }
                });
                header.col(|ui| {
                    ui.label(RichText::new("Actions").strong());
                });
            })
            .body(|body| {
                body.rows(24.0, visible.len(), |mut row| {
                    let Some(&index) = visible.get(row.index()) else {
                        return;
                    };
                    let Some(fact) = state.types.get(index) else {
                        return;
                    };
                    let selected = state.selected.as_deref() == Some(fact.key.as_str());
                    row.set_selected(selected);
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            let label = fact_label(&fact.key, &fact.label);
                            selectable_cell(ui, &label, &fact.key, &mut event);
                            if fact.is_builtin {
                                ui.label(RichText::new("built-in").weak().small());
                            }
                        });
                    });
                    row.col(|ui| {
                        selectable_cell(
                            ui,
                            fact.category.as_deref().unwrap_or_default(),
                            &fact.key,
                            &mut event,
                        );
                    });
                    row.col(|ui| {
                        if ui
                            .add(egui::Label::new(scope_label(fact.scope)).sense(Sense::click()))
                            .clicked()
                        {
                            event = Some(FactTypesEvent::Select(fact.key.clone()));
                        }
                    });
                    row.col(|ui| {
                        if ui
                            .add(egui::Label::new(kind_label(fact.kind)).sense(Sense::click()))
                            .clicked()
                        {
                            event = Some(FactTypesEvent::Select(fact.key.clone()));
                        }
                    });
                    row.col(|ui| {
                        selectable_cell(
                            ui,
                            fact.gedcom_tag.as_deref().unwrap_or_default(),
                            &fact.key,
                            &mut event,
                        );
                    });
                    row.col(|ui| {
                        if ui
                            .add(
                                egui::Label::new(fact.role_count.to_string()).sense(Sense::click()),
                            )
                            .clicked()
                        {
                            event = Some(FactTypesEvent::Select(fact.key.clone()));
                        }
                    });
                    row.col(|ui| {
                        if ui
                            .add(
                                egui::Label::new(fact.event_count.to_string())
                                    .sense(Sense::click()),
                            )
                            .clicked()
                        {
                            event = Some(FactTypesEvent::Select(fact.key.clone()));
                        }
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("Edit").clicked() {
                                event = Some(FactTypesEvent::Select(fact.key.clone()));
                            }
                            if !fact.is_builtin {
                                if fact.event_count > 0 {
                                    ui.add_enabled(false, egui::Button::new("Delete"))
                                        .on_hover_text(format!(
                                            "{} events still use this fact type",
                                            fact.event_count
                                        ));
                                } else {
                                    ui.menu_button("Delete", |ui| {
                                        ui.label(format!(
                                            "Delete {}?",
                                            fact_label(&fact.key, &fact.label)
                                        ));
                                        if ui.button("Confirm").clicked() {
                                            event = Some(FactTypesEvent::Delete(fact.key.clone()));
                                            ui.close();
                                        }
                                        if ui.button("Cancel").clicked() {
                                            ui.close();
                                        }
                                    });
                                }
                            }
                        });
                    });
                });
            });
        if let Some(column) = sort_clicked {
            toggle_sort(state, column);
        }
    });

    ui.separator();
    let editor_height = ui.available_height().max(0.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), editor_height), |ui| {
        editor(ui, state, &mut event);
    });
    let remaining = ui.available_height();
    if remaining > 0.0 {
        ui.allocate_space(egui::vec2(ui.available_width(), remaining));
    }
    event
}

fn editor(ui: &mut Ui, state: &mut FactTypesState, event: &mut Option<FactTypesEvent>) {
    if !state.editing {
        ui.label(
            RichText::new("Select a fact type or create a new one.")
                .weak()
                .size(13.0),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("fact_type_editor")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(
                RichText::new(if state.draft.key.is_some() {
                    "Edit fact type"
                } else {
                    "New fact type"
                })
                .strong(),
            );

            let mut remove_role = None;
            let mut label_changed = false;
            egui::Grid::new("fact_type_fields")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Label *").strong());
                    if state.builtin {
                        ui.label(&state.draft.label);
                    } else {
                        label_changed = ui.text_edit_singleline(&mut state.draft.label).changed();
                    }
                    ui.end_row();

                    if label_changed && state.auto_tag {
                        state.draft.gedcom_tag = auto_tag(&state.draft.label);
                    }

                    ui.label("Key:");
                    match state.draft.key.as_deref() {
                        Some(key) => ui.label(key),
                        None => ui.label(format!(
                            "{} (generated on save)",
                            suggested_key(&state.draft.label)
                        )),
                    };
                    ui.end_row();

                    ui.label("Category:");
                    if state.builtin {
                        ui.label(state.draft.category.as_str());
                    } else {
                        ui.text_edit_singleline(&mut state.draft.category);
                    }
                    ui.end_row();

                    ui.label("Scope:");
                    if state.builtin {
                        ui.label(scope_label(state.draft.scope));
                    } else {
                        egui::ComboBox::from_id_salt("fact_scope")
                            .selected_text(scope_label(state.draft.scope))
                            .show_ui(ui, |ui| {
                                for scope in [FactScope::Person, FactScope::Family, FactScope::Both] {
                                    ui.selectable_value(
                                        &mut state.draft.scope,
                                        scope,
                                        scope_label(scope),
                                    );
                                }
                            });
                    }
                    ui.end_row();

                    ui.label("Kind:");
                    if state.builtin {
                        ui.label(kind_label(state.draft.kind));
                    } else {
                        egui::ComboBox::from_id_salt("fact_kind")
                            .selected_text(kind_label(state.draft.kind))
                            .show_ui(ui, |ui| {
                                for kind in [FactKind::Event, FactKind::Attribute] {
                                    ui.selectable_value(
                                        &mut state.draft.kind,
                                        kind,
                                        kind_label(kind),
                                    );
                                }
                            });
                    }
                    ui.end_row();

                    ui.label("GEDCOM tag:");
                    if state.builtin {
                        ui.label(state.draft.gedcom_tag.as_str());
                    } else {
                        if ui
                            .text_edit_singleline(&mut state.draft.gedcom_tag)
                            .changed()
                        {
                            state.auto_tag = false;
                        }
                    }
                    ui.end_row();
                });
            if !state.builtin {
                ui.label(
                    RichText::new(
                        "Custom tags are uppercase and unique; an underscore prefix avoids standard tags.",
                    )
                    .weak()
                    .small(),
                );
            }

            ui.separator();
            ui.label(RichText::new("Roles").strong());
            if state.draft.roles.is_empty() {
                ui.label(RichText::new("No roles defined.").weak().small());
            }
            for (index, role) in state.draft.roles.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut role.label);
                        match role.key.as_deref() {
                            Some(key) => {
                                ui.label(RichText::new(key).weak().small());
                            }
                            None => {
                                ui.label(RichText::new("new key").weak().small());
                            }
                        }
                        if ui.small_button("Remove").clicked() {
                            remove_role = Some(index);
                        }
                    });
                });
            }
            if let Some(index) = remove_role {
                state.draft.roles.remove(index);
            }
            ui.horizontal(|ui| {
                ui.label("Add role:");
                ui.text_edit_singleline(&mut state.add_role);
                if ui
                    .add_enabled(
                        !state.add_role.trim().is_empty(),
                        egui::Button::new("Add"),
                    )
                    .clicked()
                {
                    add_role(state);
                }
            });

            if let Some(error) = &state.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }

            let valid = !state.draft.label.trim().is_empty()
                && (state.builtin || !state.draft.gedcom_tag.trim().is_empty());
            if ui
                .add_enabled(valid, egui::Button::new(RichText::new("Save").strong()))
                .clicked()
            {
                *event = Some(FactTypesEvent::Save(Box::new(state.draft.clone())));
            }
        });
}

fn add_role(state: &mut FactTypesState) {
    let label = state.add_role.trim();
    if label.is_empty() {
        return;
    }
    state.draft.roles.push(FactRoleDraft {
        key: None,
        label: label.to_string(),
    });
    state.add_role.clear();
}

fn auto_tag(label: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        return String::new();
    }
    let key = suggested_key(label);
    let mut tag = format!("_{key}");
    tag.truncate(31);
    tag
}

fn scope_label(scope: FactScope) -> &'static str {
    match scope {
        FactScope::Person => "Person",
        FactScope::Family => "Family",
        FactScope::Both => "Both",
    }
}

fn kind_label(kind: FactKind) -> &'static str {
    match kind {
        FactKind::Event => "Event",
        FactKind::Attribute => "Attribute",
    }
}

fn visible_indices(state: &FactTypesState) -> Vec<usize> {
    let needle = state.filter.trim().to_lowercase();
    let mut indices: Vec<usize> = state
        .types
        .iter()
        .enumerate()
        .filter(|(_, fact)| {
            if needle.is_empty() {
                return true;
            }
            fact.label.to_lowercase().contains(&needle)
                || fact
                    .category
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&needle)
                || fact
                    .gedcom_tag
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&needle)
        })
        .map(|(index, _)| index)
        .collect();
    if let Some((column, ascending)) = state.sort {
        indices.sort_by(|&left, &right| {
            let (Some(left), Some(right)) = (state.types.get(left), state.types.get(right)) else {
                return Ordering::Equal;
            };
            let order = match column {
                SortColumn::Label => left.label.to_lowercase().cmp(&right.label.to_lowercase()),
                SortColumn::Category => left
                    .category
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .cmp(&right.category.as_deref().unwrap_or_default().to_lowercase()),
                SortColumn::Events => left.event_count.cmp(&right.event_count),
            };
            if ascending { order } else { order.reverse() }
        });
    }
    indices
}

fn toggle_sort(state: &mut FactTypesState, column: SortColumn) {
    state.sort = match state.sort {
        Some((active, ascending)) if active == column => Some((active, !ascending)),
        _ => Some((column, true)),
    };
}

fn selectable_cell(ui: &mut Ui, text: &str, key: &str, event: &mut Option<FactTypesEvent>) {
    if ui
        .add(egui::Label::new(text).sense(Sense::click()))
        .clicked()
    {
        *event = Some(FactTypesEvent::Select(key.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::facts::FactTypeRole;
    use egui::{Context, RawInput};

    fn record() -> FactTypeRecord {
        FactTypeRecord {
            key: "HEIRLOOM".to_string(),
            label: "Heirloom".to_string(),
            category: Some("Other".to_string()),
            gedcom_tag: Some("_HEIR".to_string()),
            scope: FactScope::Person,
            kind: FactKind::Attribute,
            is_builtin: false,
            roles: vec![FactTypeRole {
                role_key: "OWNER".to_string(),
                label: "Owner".to_string(),
                sort_order: 0,
            }],
        }
    }

    fn summary(key: &str, label: &str, events: i64) -> FactTypeSummary {
        FactTypeSummary {
            key: key.to_string(),
            label: label.to_string(),
            category: Some("Other".to_string()),
            gedcom_tag: Some(format!("_{key}")),
            scope: FactScope::Person,
            kind: FactKind::Event,
            is_builtin: false,
            role_count: 0,
            event_count: events,
        }
    }

    #[test]
    fn drafts_keep_existing_roles() {
        let draft = draft_from_record(&record());
        assert_eq!(draft.key.as_deref(), Some("HEIRLOOM"));
        assert_eq!(draft.gedcom_tag, "_HEIR");
        assert_eq!(draft.roles.len(), 1);
        assert_eq!(draft.roles[0].key.as_deref(), Some("OWNER"));
    }

    #[test]
    fn adds_roles_and_auto_tags_new_types() {
        let mut state = FactTypesState {
            add_role: "Owner".to_string(),
            ..FactTypesState::default()
        };
        add_role(&mut state);
        assert_eq!(state.draft.roles.len(), 1);
        assert_eq!(state.draft.roles[0].label, "Owner");
        assert!(state.add_role.is_empty());

        assert_eq!(auto_tag("Heirloom"), "_HEIRLOOM");
        assert_eq!(auto_tag("Family Heirloom"), "_FAMILY_HEIRLOOM");
        assert!(auto_tag("   ").is_empty());
    }

    #[test]
    fn filters_and_sorts_the_grid() {
        let mut state = FactTypesState {
            types: vec![
                summary("BIRTH", "Birth", 3),
                summary("HEIRLOOM", "Heirloom", 1),
            ],
            ..FactTypesState::default()
        };
        assert_eq!(visible_indices(&state), vec![0, 1]);
        state.filter = "heir".to_string();
        assert_eq!(visible_indices(&state), vec![1]);
        state.filter.clear();
        toggle_sort(&mut state, SortColumn::Events);
        assert_eq!(visible_indices(&state), vec![1, 0]);
    }

    #[test]
    fn fact_types_tab_fills_the_available_height() {
        let ctx = Context::default();
        let mut state = FactTypesState {
            types: vec![summary("HEIRLOOM", "Heirloom", 0)],
            ..FactTypesState::default()
        };
        let mut fills = false;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let available = ui.available_height();
            show(ui, &mut state);
            fills = ui.min_rect().height() >= available - 1.0;
        });
        output.textures_delta.clear();
        assert!(fills, "the fact types tab must fill the available height");
    }

    #[test]
    fn fact_types_tab_renders_headless() {
        let ctx = Context::default();
        let mut state = FactTypesState {
            types: vec![summary("HEIRLOOM", "Heirloom", 0)],
            selected: Some("HEIRLOOM".to_string()),
            draft: draft_from_record(&record()),
            editing: true,
            ..FactTypesState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the fact types tab paints");
        assert!(!state.loaded, "the test drives loading separately");
    }
}
