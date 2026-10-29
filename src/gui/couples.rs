//! Sortable, filterable list of family couples.

use std::cmp::Ordering;

use egui::{RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::i18n::tr;
use crate::model::couples::Couple;

const ROW_HEIGHT: f32 = 24.0;

/// Columns the couple list can sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoupleSort {
    Partner1,
    Partner2,
    Married,
    Place,
    Children,
}

impl CoupleSort {
    fn label(self) -> &'static str {
        match self {
            Self::Partner1 => "couples.partner1",
            Self::Partner2 => "couples.partner2",
            Self::Married => "couples.married",
            Self::Place => "couples.place",
            Self::Children => "couples.children",
        }
    }
}

/// What the couple view asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CouplesEvent {
    SelectPerson(String),
    EditPerson(String),
    OpenFamily(String),
}

/// Filter/sort state for the couple list.
#[derive(Debug, Clone, Default)]
pub struct CouplesState {
    pub filter: String,
    sort: Option<(CoupleSort, bool)>,
    view: Vec<usize>,
    dirty: bool,
}

impl CouplesState {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn toggle_sort(&mut self, column: CoupleSort) {
        self.sort = match self.sort {
            Some((active, ascending)) if active == column => Some((active, !ascending)),
            _ => Some((column, true)),
        };
        self.dirty = true;
    }

    fn rebuild(&mut self, rows: &[Couple]) {
        let needle = self.filter.trim().to_lowercase();
        let mut view: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, couple)| {
                needle.is_empty()
                    || couple.partner1_name.to_lowercase().contains(&needle)
                    || couple.partner2_name.to_lowercase().contains(&needle)
                    || couple
                        .marriage_place
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&needle)
            })
            .map(|(index, _)| index)
            .collect();
        if let Some((column, ascending)) = self.sort {
            view.sort_by(|&left, &right| {
                let (Some(left), Some(right)) = (rows.get(left), rows.get(right)) else {
                    return Ordering::Equal;
                };
                let order = compare(left, right, column);
                if ascending { order } else { order.reverse() }
            });
        }
        self.view = view;
        self.dirty = false;
    }
}

fn compare(left: &Couple, right: &Couple, column: CoupleSort) -> Ordering {
    match column {
        CoupleSort::Children => left.child_count.cmp(&right.child_count),
        CoupleSort::Partner1 => text(
            Some(left.partner1_name.clone()),
            Some(right.partner1_name.clone()),
        ),
        CoupleSort::Partner2 => text(
            Some(left.partner2_name.clone()),
            Some(right.partner2_name.clone()),
        ),
        CoupleSort::Married => text(left.marriage_date.clone(), right.marriage_date.clone()),
        CoupleSort::Place => text(left.marriage_place.clone(), right.marriage_place.clone()),
    }
}

fn text(left: Option<String>, right: Option<String>) -> Ordering {
    match (
        left.filter(|value| !value.is_empty()),
        right.filter(|value| !value.is_empty()),
    ) {
        (Some(left), Some(right)) => left.to_lowercase().cmp(&right.to_lowercase()),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Draws the Couples tab and returns requested actions.
pub fn show(ui: &mut Ui, state: &mut CouplesState, rows: &[Couple]) -> Vec<CouplesEvent> {
    let mut events = Vec::new();
    ui.horizontal(|ui| {
        ui.label(tr("common.filter"));
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.filter).hint_text(tr("couples.filter_hint")),
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
                "couples.showing",
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
        ui.label(RichText::new(tr("couples.empty")).weak().size(13.0));
        return events;
    }

    let mut sort_clicked = None;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::auto().at_least(130.0))
        .column(Column::auto().at_least(130.0))
        .column(Column::auto().at_least(90.0))
        .column(Column::auto().at_least(110.0))
        .column(Column::auto().at_least(70.0))
        .column(Column::auto().at_least(110.0))
        .header(22.0, |mut header| {
            for column in [
                CoupleSort::Partner1,
                CoupleSort::Partner2,
                CoupleSort::Married,
                CoupleSort::Place,
                CoupleSort::Children,
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
                let Some(couple) = rows.get(index) else {
                    return;
                };
                row.col(|ui| {
                    if let Some(id) = &couple.partner1_id {
                        clickable_person(ui, &couple.partner1_name, id, &mut events);
                    }
                });
                row.col(|ui| {
                    if let Some(id) = &couple.partner2_id {
                        clickable_person(ui, &couple.partner2_name, id, &mut events);
                    }
                });
                row.col(|ui| {
                    ui.label(couple.marriage_date.as_deref().unwrap_or(""));
                });
                row.col(|ui| {
                    ui.label(couple.marriage_place.as_deref().unwrap_or(""));
                });
                row.col(|ui| {
                    ui.label(couple.child_count.to_string());
                });
                row.col(|ui| {
                    if ui.button(tr("couples.open_family")).clicked() {
                        events.push(CouplesEvent::OpenFamily(couple.family_id.clone()));
                    }
                });
            });
        });
    if let Some(column) = sort_clicked {
        state.toggle_sort(column);
    }
    events
}

fn clickable_person(ui: &mut Ui, name: &str, id: &str, events: &mut Vec<CouplesEvent>) {
    let response = ui.add(egui::Label::new(name).sense(Sense::click()));
    if response.double_clicked() {
        events.push(CouplesEvent::EditPerson(id.to_string()));
    } else if response.clicked() {
        events.push(CouplesEvent::SelectPerson(id.to_string()));
    }
    response.on_hover_text(tr("watchlist.double_click_edit"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn couple(id: &str, one: &str, two: &str, date: &str, place: &str, children: i64) -> Couple {
        Couple {
            family_id: id.to_string(),
            partner1_id: Some(format!("{id}-1")),
            partner1_name: one.to_string(),
            partner2_id: Some(format!("{id}-2")),
            partner2_name: two.to_string(),
            marriage_date: (!date.is_empty()).then(|| date.to_string()),
            marriage_place: (!place.is_empty()).then(|| place.to_string()),
            child_count: children,
            marriage_event_id: None,
        }
    }

    #[test]
    fn filters_and_sorts_couples() {
        let rows = vec![
            couple("f1", "Bo", "Zed", "1900", "London", 2),
            couple("f2", "Ada", "King", "1850", "", 1),
            couple("f3", "Carl", "Able", "", "Paris", 3),
        ];
        let mut state = CouplesState {
            filter: "ada".to_string(),
            ..CouplesState::default()
        };
        state.rebuild(&rows);
        assert_eq!(state.view, vec![1]);
        state.filter = "paris".to_string();
        state.rebuild(&rows);
        assert_eq!(state.view, vec![2]);
        state.filter.clear();
        state.toggle_sort(CoupleSort::Partner1);
        state.rebuild(&rows);
        assert_eq!(state.view, vec![1, 0, 2]);
        state.toggle_sort(CoupleSort::Children);
        state.rebuild(&rows);
        assert_eq!(state.view, vec![1, 0, 2]);
    }

    #[test]
    fn missing_values_sort_last() {
        let rows = vec![
            couple("f1", "Bo", "", "", "", 0),
            couple("f2", "Ada", "", "1850", "London", 0),
        ];
        let mut state = CouplesState::default();
        state.toggle_sort(CoupleSort::Married);
        state.rebuild(&rows);
        assert_eq!(state.view, vec![1, 0]);
    }

    #[test]
    fn couple_view_renders_headless() {
        let rows = vec![couple("f1", "Ada", "Charles", "1850", "London", 2)];
        let ctx = Context::default();
        let mut state = CouplesState::default();
        let mut events = Vec::new();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            events = show(ui, &mut state, &rows);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(events.is_empty());
    }
}
