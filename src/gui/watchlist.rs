//! Sortable, filterable people grid backed by `egui_extras::TableBuilder`.

use std::cmp::Ordering;
use std::collections::HashMap;

use egui::{Color32, Layout, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::db::{FamilyTree, people::VitalDates};

const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);
const ROW_HEIGHT: f32 = 24.0;

/// One grid row: identity plus denormalised vitals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonRow {
    pub id: String,
    pub given_name: String,
    pub surname: String,
    pub gender: String,
    pub birth: Option<String>,
    pub death: Option<String>,
}

impl PersonRow {
    pub fn display_name(&self) -> String {
        let combined = format!("{} {}", self.given_name, self.surname);
        let combined = combined.trim();
        if combined.is_empty() {
            "(unnamed)".to_string()
        } else {
            combined.to_string()
        }
    }
}

/// Flattens the loaded tree plus vitals into grid rows in rowid order.
pub fn rows(tree: &FamilyTree, dates: &HashMap<String, VitalDates>) -> Vec<PersonRow> {
    let mut rows = Vec::with_capacity(tree.people.len());
    for person in &tree.people {
        let vitals = dates.get(&person.id);
        // The single Given cell shows given + middle so patronymics stay
        // visible and searchable without a separate column.
        let given_name = [person.given_name.as_str(), person.middle_name.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        rows.push(PersonRow {
            id: person.id.clone(),
            given_name,
            surname: person.surname.clone(),
            gender: person.gender.clone(),
            birth: vitals.and_then(|vitals| vitals.birth.clone()),
            death: vitals.and_then(|vitals| vitals.death.clone()),
        });
    }
    rows
}

/// Human-readable gender for the grid and inspector.
pub fn display_gender(code: &str) -> &'static str {
    match code {
        "M" => "Male",
        "F" => "Female",
        "U" => "Unknown",
        _ => "?",
    }
}

/// Columns the header can sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Given,
    Surname,
    Gender,
    Birth,
    Death,
}

impl SortColumn {
    fn label(self) -> &'static str {
        match self {
            Self::Given => "Given Name",
            Self::Surname => "Surname",
            Self::Gender => "Gender",
            Self::Birth => "Birth Date",
            Self::Death => "Death Date",
        }
    }
}

/// What the user asked the shell to do from the grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    Select(String),
    AddPerson,
    Delete(String),
}

/// Filter/sort state plus the cached view of matching row indices.
///
/// The view only rebuilds when [`Self::mark_dirty`] ran or the filter and
/// sort changed, so a 10,000-person table never re-filters per frame.
#[derive(Debug, Clone)]
pub struct WatchState {
    pub filter: String,
    sort: Option<(SortColumn, bool)>,
    view: Vec<usize>,
    dirty: bool,
}

impl Default for WatchState {
    fn default() -> Self {
        Self {
            filter: String::new(),
            sort: None,
            view: Vec::new(),
            dirty: true,
        }
    }
}

impl WatchState {
    /// Marks the cached view stale after the underlying data changed.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Clicks a header: same column flips direction, a new one sorts ascending.
    pub fn toggle_sort(&mut self, column: SortColumn) {
        self.sort = match self.sort {
            Some((active, ascending)) if active == column => Some((active, !ascending)),
            _ => Some((column, true)),
        };
        self.dirty = true;
    }

    fn rebuild(&mut self, rows: &[PersonRow]) {
        let mut view: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| self.matches(row))
            .map(|(index, _)| index)
            .collect();
        if let Some((column, ascending)) = self.sort {
            view.sort_by(|&left, &right| {
                let (Some(left_row), Some(right_row)) = (rows.get(left), rows.get(right)) else {
                    return Ordering::Equal;
                };
                let order = compare(left_row, right_row, column);
                if ascending { order } else { order.reverse() }
            });
        }
        self.view = view;
        self.dirty = false;
    }

    fn matches(&self, row: &PersonRow) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        let needle = self.filter.to_lowercase();
        row.given_name.to_lowercase().contains(&needle)
            || row.surname.to_lowercase().contains(&needle)
    }
}

fn compare(left: &PersonRow, right: &PersonRow, column: SortColumn) -> Ordering {
    match column {
        SortColumn::Given => left.given_name.cmp(&right.given_name),
        SortColumn::Surname => left.surname.cmp(&right.surname),
        SortColumn::Gender => display_gender(&left.gender).cmp(display_gender(&right.gender)),
        SortColumn::Birth => left.birth.cmp(&right.birth),
        SortColumn::Death => left.death.cmp(&right.death),
    }
}

/// Renders the toolbar and virtualized table, returning the events to apply.
pub fn show(
    ui: &mut Ui,
    state: &mut WatchState,
    rows: &[PersonRow],
    selected: Option<&str>,
) -> Vec<WatchEvent> {
    let mut events = Vec::new();

    ui.horizontal(|ui| {
        ui.label("Filter:");
        let response =
            ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text("Filter by name..."));
        if response.changed() {
            state.dirty = true;
        }
        if ui
            .add_enabled(!state.filter.is_empty(), egui::Button::new("Clear"))
            .clicked()
        {
            state.filter.clear();
            state.dirty = true;
        }
    });

    if state.dirty {
        state.rebuild(rows);
    }

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "Showing {} of {} people",
                state.view.len(),
                rows.len()
            ))
            .weak()
            .size(12.0),
        );
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("+ Add Independent Person").strong())
                        .fill(ACCENT),
                )
                .clicked()
            {
                events.push(WatchEvent::AddPerson);
            }
        });
    });

    if rows.is_empty() {
        ui.label(
            RichText::new("No people yet \u{2014} add one with the button above.")
                .weak()
                .size(13.0),
        );
        return events;
    }

    let mut sort_clicked: Option<SortColumn> = None;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::auto().at_least(90.0))
        .column(Column::auto().at_least(90.0))
        .column(Column::auto().at_least(70.0))
        .column(Column::auto().at_least(90.0))
        .column(Column::auto().at_least(90.0))
        .column(Column::auto().at_least(120.0))
        .header(22.0, |mut header| {
            for column in [
                SortColumn::Given,
                SortColumn::Surname,
                SortColumn::Gender,
                SortColumn::Birth,
                SortColumn::Death,
            ] {
                header.col(|ui| {
                    let arrow = match state.sort {
                        Some((active, true)) if active == column => " \u{25b2}",
                        Some((active, false)) if active == column => " \u{25bc}",
                        _ => "",
                    };
                    let text = format!("{}{}", column.label(), arrow);
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
                ui.label(RichText::new("Actions").strong());
            });
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, state.view.len(), |mut row| {
                let Some(&index) = state.view.get(row.index()) else {
                    return;
                };
                let Some(person) = rows.get(index) else {
                    return;
                };
                let is_selected = selected == Some(person.id.as_str());
                row.set_selected(is_selected);

                row.col(|ui| {
                    clickable_cell(ui, &person.given_name, &person.id, &mut events);
                });
                row.col(|ui| {
                    clickable_cell(ui, &person.surname, &person.id, &mut events);
                });
                row.col(|ui| {
                    clickable_cell(ui, display_gender(&person.gender), &person.id, &mut events);
                });
                row.col(|ui| {
                    clickable_cell(
                        ui,
                        person.birth.as_deref().unwrap_or_default(),
                        &person.id,
                        &mut events,
                    );
                });
                row.col(|ui| {
                    clickable_cell(
                        ui,
                        person.death.as_deref().unwrap_or_default(),
                        &person.id,
                        &mut events,
                    );
                });
                row.col(|ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("Edit").clicked() {
                            events.push(WatchEvent::Select(person.id.clone()));
                        }
                        ui.menu_button("Delete", |ui| {
                            ui.label(format!("Delete {}?", person.display_name()));
                            if ui.button("Confirm").clicked() {
                                events.push(WatchEvent::Delete(person.id.clone()));
                                ui.close();
                            }
                            if ui.button("Cancel").clicked() {
                                ui.close();
                            }
                        });
                    });
                });
            });
        });

    if let Some(column) = sort_clicked {
        state.toggle_sort(column);
    }
    events
}

fn clickable_cell(ui: &mut Ui, text: &str, person_id: &str, events: &mut Vec<WatchEvent>) {
    if ui
        .add(egui::Label::new(text).sense(Sense::click()))
        .clicked()
    {
        events.push(WatchEvent::Select(person_id.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Person;
    use egui::{Context, RawInput};

    fn rows_for(names: &[(&str, &str)]) -> Vec<PersonRow> {
        let mut tree = FamilyTree::default();
        for (given, surname) in names {
            tree.people.push(Person::new(*given, *surname, "U"));
        }
        rows(&tree, &HashMap::new())
    }

    #[test]
    fn rows_join_people_with_vital_dates() {
        let mut tree = FamilyTree::default();
        let mut person = Person::new("Ada", "King", "F");
        person.middle_name = "Lynn".to_string();
        let id = person.id.clone();
        tree.people.push(person);
        let mut dates = HashMap::new();
        dates.insert(
            id.clone(),
            VitalDates {
                birth: Some("1815".to_string()),
                death: Some("1852".to_string()),
                ..Default::default()
            },
        );

        let built = rows(&tree, &dates);
        assert_eq!(built.len(), 1);
        let row = built.first().expect("one row");
        assert_eq!(row.id, id);
        assert_eq!(row.birth.as_deref(), Some("1815"));
        assert_eq!(row.death.as_deref(), Some("1852"));
        assert_eq!(row.given_name, "Ada Lynn", "the Given cell shows middle");
        assert_eq!(row.display_name(), "Ada Lynn King");
    }

    #[test]
    fn filter_matches_either_name_case_insensitively() {
        let rows = rows_for(&[("Ada", "King"), ("Bo", "Tester"), ("Carl", "Tester")]);
        let mut state = WatchState {
            filter: "aDa".to_string(),
            ..WatchState::default()
        };
        state.rebuild(&rows);
        assert_eq!(state.view, vec![0], "given name filter");

        state.filter = "tester".to_string();
        state.rebuild(&rows);
        assert_eq!(state.view, vec![1, 2], "surname filter matches both");

        state.filter = "zzz".to_string();
        state.rebuild(&rows);
        assert!(state.view.is_empty(), "no match empties the view");
    }

    #[test]
    fn sorting_toggles_per_column_and_direction() {
        let rows = rows_for(&[("Bo", "Same"), ("Ada", "Same"), ("Carl", "Same")]);
        let mut state = WatchState::default();

        state.toggle_sort(SortColumn::Given);
        state.rebuild(&rows);
        assert_eq!(state.view, vec![1, 0, 2], "ascending by given name");

        state.toggle_sort(SortColumn::Given);
        state.rebuild(&rows);
        assert_eq!(
            state.view,
            vec![2, 0, 1],
            "second click flips to descending"
        );

        state.toggle_sort(SortColumn::Surname);
        state.rebuild(&rows);
        assert_eq!(
            state.view,
            vec![0, 1, 2],
            "a new column starts ascending and equal keys keep rowid order"
        );
    }

    #[test]
    fn watchlist_renders_headless() {
        let rows = rows_for(&[("Ada", "King")]);
        let ctx = Context::default();
        let mut state = WatchState::default();
        let mut captured = Vec::new();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            captured = show(ui, &mut state, &rows, None);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the grid paints");
        assert!(captured.is_empty(), "no pointer interaction picks nothing");
        assert_eq!(state.view, vec![0], "the initial view rebuilt");
    }
}
