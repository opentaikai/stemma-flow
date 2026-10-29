//! Sortable, filterable list of directed person associations.

use std::cmp::Ordering;

use egui::{RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::db::associations::AssociationViewRow;
use crate::i18n::tr;

const ROW_HEIGHT: f32 = 24.0;

/// Sortable association columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociationSort {
    From,
    Associate,
    Relationship,
}

impl AssociationSort {
    fn label(self) -> &'static str {
        match self {
            Self::From => "associations.from",
            Self::Associate => "associations.associate",
            Self::Relationship => "associations.relationship",
        }
    }
}

/// Actions requested by the association list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssociationViewEvent {
    SelectPerson(String),
    EditPerson(String),
    EditAssociation(String),
    Delete(String),
}

/// Filter/sort/delete-confirmation state.
#[derive(Debug, Clone, Default)]
pub struct AssociationViewState {
    pub filter: String,
    pub pending_delete: Option<String>,
    sort: Option<(AssociationSort, bool)>,
    view: Vec<usize>,
    dirty: bool,
    pub loaded: bool,
}

impl AssociationViewState {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn toggle_sort(&mut self, column: AssociationSort) {
        self.sort = match self.sort {
            Some((active, ascending)) if active == column => Some((active, !ascending)),
            _ => Some((column, true)),
        };
        self.dirty = true;
    }

    fn rebuild(&mut self, rows: &[AssociationViewRow]) {
        let needle = self.filter.trim().to_lowercase();
        let mut view: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                needle.is_empty()
                    || row.person_name.to_lowercase().contains(&needle)
                    || row.associate_name.to_lowercase().contains(&needle)
                    || row.relationship.to_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect();
        if let Some((column, ascending)) = self.sort {
            view.sort_by(|&left, &right| {
                let (Some(left), Some(right)) = (rows.get(left), rows.get(right)) else {
                    return Ordering::Equal;
                };
                let order = match column {
                    AssociationSort::From => text(&left.person_name, &right.person_name),
                    AssociationSort::Associate => text(&left.associate_name, &right.associate_name),
                    AssociationSort::Relationship => text(&left.relationship, &right.relationship),
                };
                if ascending { order } else { order.reverse() }
            });
        }
        self.view = view;
        self.dirty = false;
    }
}

fn text(left: &str, right: &str) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (false, false) => left.to_lowercase().cmp(&right.to_lowercase()),
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        (true, true) => Ordering::Equal,
    }
}

/// Draws the Associations tab.
pub fn show(
    ui: &mut Ui,
    state: &mut AssociationViewState,
    rows: &[AssociationViewRow],
) -> Vec<AssociationViewEvent> {
    let mut events = Vec::new();
    ui.horizontal(|ui| {
        ui.label(tr("common.filter"));
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.filter).hint_text(tr("associations.filter_hint")),
        );
        if response.changed() {
            state.dirty = true;
        }
        if ui
            .add_enabled(
                !state.filter.is_empty(),
                egui::Button::new(tr("common.clear")),
            )
            .clicked()
        {
            state.filter.clear();
            state.dirty = true;
        }
        ui.label(
            RichText::new(crate::i18n::tr_args(
                "associations.showing",
                &[
                    ("shown", state.view.len().to_string()),
                    ("total", rows.len().to_string()),
                ],
            ))
            .weak()
            .size(12.0),
        );
    });
    if state.dirty {
        state.rebuild(rows);
    }
    if rows.is_empty() {
        ui.label(RichText::new(tr("associations.empty")).weak().size(13.0));
        return events;
    }

    let mut sort_clicked = None;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::auto().at_least(130.0))
        .column(Column::auto().at_least(130.0))
        .column(Column::auto().at_least(110.0))
        .column(Column::auto().at_least(150.0))
        .header(22.0, |mut header| {
            for column in [
                AssociationSort::From,
                AssociationSort::Associate,
                AssociationSort::Relationship,
            ] {
                header.col(|ui| {
                    let arrow = match state.sort {
                        Some((active, true)) if active == column => " \u{25b2}",
                        Some((active, false)) if active == column => " \u{25bc}",
                        _ => "",
                    };
                    let text = format!("{}{}", tr(column.label()), arrow);
                    let active = state.sort.map(|(active, _)| active) == Some(column);
                    let label = egui::Label::new(if active {
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
            header.col(|ui| {
                ui.label(RichText::new(tr("common.actions")).strong());
            });
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, state.view.len(), |mut row| {
                let Some(&index) = state.view.get(row.index()) else {
                    return;
                };
                let Some(association) = rows.get(index) else {
                    return;
                };
                row.col(|ui| {
                    clickable_person(
                        ui,
                        &association.person_name,
                        &association.person_id,
                        &mut events,
                    );
                });
                row.col(|ui| {
                    clickable_person(
                        ui,
                        &association.associate_name,
                        &association.associate_id,
                        &mut events,
                    );
                });
                row.col(|ui| {
                    ui.label(&association.relationship);
                });
                row.col(|ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button(tr("common.edit")).clicked() {
                            events.push(AssociationViewEvent::EditAssociation(
                                association.id.clone(),
                            ));
                        }
                        if state.pending_delete.as_deref() == Some(association.id.as_str()) {
                            if ui.small_button(tr("people.confirm")).clicked() {
                                state.pending_delete = None;
                                events.push(AssociationViewEvent::Delete(association.id.clone()));
                            }
                            if ui.small_button(tr("common.cancel")).clicked() {
                                state.pending_delete = None;
                            }
                        } else if ui.small_button(tr("common.delete")).clicked() {
                            state.pending_delete = Some(association.id.clone());
                        }
                    });
                });
            });
        });
    if let Some(column) = sort_clicked {
        state.toggle_sort(column);
    }
    events
}

fn clickable_person(ui: &mut Ui, name: &str, id: &str, events: &mut Vec<AssociationViewEvent>) {
    let response = ui.add(egui::Label::new(name).sense(Sense::click()));
    if response.double_clicked() {
        events.push(AssociationViewEvent::EditPerson(id.to_string()));
    } else if response.clicked() {
        events.push(AssociationViewEvent::SelectPerson(id.to_string()));
    }
    response.on_hover_text(tr("watchlist.double_click_edit"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn row(id: &str, from: &str, to: &str, label: &str) -> AssociationViewRow {
        AssociationViewRow {
            id: id.to_string(),
            person_id: format!("{id}-from"),
            person_name: from.to_string(),
            associate_id: format!("{id}-to"),
            associate_name: to.to_string(),
            relationship: label.to_string(),
        }
    }

    #[test]
    fn filters_and_sorts_associations() {
        let rows = vec![
            row("a1", "Bo", "Zed", "Friend"),
            row("a2", "Ada", "King", "Godparent"),
            row("a3", "Carl", "Able", "Neighbour"),
        ];
        let mut state = AssociationViewState {
            filter: "ada".to_string(),
            ..AssociationViewState::default()
        };
        state.rebuild(&rows);
        assert_eq!(state.view, vec![1]);
        state.filter = "neighbour".to_string();
        state.rebuild(&rows);
        assert_eq!(state.view, vec![2]);
        state.filter.clear();
        state.toggle_sort(AssociationSort::From);
        state.rebuild(&rows);
        assert_eq!(state.view, vec![1, 0, 2]);
        state.toggle_sort(AssociationSort::Relationship);
        state.rebuild(&rows);
        assert_eq!(state.view, vec![0, 1, 2]);
    }

    #[test]
    fn association_view_renders_headless() {
        let rows = vec![row("a1", "Ada", "Charles", "Friend")];
        let ctx = Context::default();
        let mut state = AssociationViewState::default();
        let mut events = Vec::new();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            events = show(ui, &mut state, &rows);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(events.is_empty());
    }
}
