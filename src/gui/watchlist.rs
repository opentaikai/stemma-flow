//! Configurable, sortable, filterable People grid.

use std::cmp::Ordering;
use std::collections::HashMap;

use egui::{Color32, Layout, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::db::FamilyTree;
use crate::db::people::{PersonExtras, VitalDates};
use crate::i18n::tr;
use crate::model::people_columns::{
    PeopleColumn, PeopleColumnConfig, default_columns, sanitize_columns,
};

const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);
const ROW_HEIGHT: f32 = 24.0;

/// One grid row: identity plus denormalised People-list values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonRow {
    pub id: String,
    pub prefix: String,
    pub given_name: String,
    pub middle_name: String,
    pub surname: String,
    pub suffix: String,
    pub nickname: String,
    pub gender: String,
    pub birth: Option<String>,
    pub birth_place: Option<String>,
    pub death: Option<String>,
    pub death_place: Option<String>,
    pub age_at_death: Option<String>,
    pub notes_preview: String,
    pub event_count: i64,
    pub citation_count: i64,
    pub media_count: i64,
    pub task_count: i64,
    pub group_count: i64,
    pub association_count: i64,
    pub updated_at: String,
    pub display_name: String,
}

impl PersonRow {
    pub fn display_name(&self) -> String {
        self.display_name.clone()
    }
}

/// Flattens the loaded tree plus vitals and extras into grid rows.
pub fn rows(
    tree: &FamilyTree,
    dates: &HashMap<String, VitalDates>,
    extras: &HashMap<String, PersonExtras>,
    surname_first: bool,
) -> Vec<PersonRow> {
    let mut rows = Vec::with_capacity(tree.people.len());
    for person in &tree.people {
        let vitals = dates.get(&person.id);
        let extra = extras.get(&person.id);
        let display_name = crate::model::name::format_name(
            &person.name_prefix,
            &person.given_name,
            &person.middle_name,
            &person.surname,
            &person.name_suffix,
            surname_first,
        );
        let notes_preview = extra
            .and_then(|extra| extra.notes_preview.as_deref())
            .and_then(|notes| notes.lines().next())
            .unwrap_or_default()
            .to_string();
        rows.push(PersonRow {
            id: person.id.clone(),
            prefix: person.name_prefix.clone(),
            given_name: person.given_name.clone(),
            middle_name: person.middle_name.clone(),
            surname: person.surname.clone(),
            suffix: person.name_suffix.clone(),
            nickname: person.nickname.clone(),
            gender: person.gender.clone(),
            birth: vitals.and_then(|vitals| vitals.birth.clone()),
            birth_place: vitals.and_then(|vitals| vitals.birth_place.clone()),
            death: vitals.and_then(|vitals| vitals.death.clone()),
            death_place: vitals.and_then(|vitals| vitals.death_place.clone()),
            age_at_death: crate::model::age::age_at(
                vitals.and_then(|vitals| vitals.birth.as_deref()),
                vitals.and_then(|vitals| vitals.death.as_deref()),
            )
            .map(|age| age.to_string()),
            notes_preview,
            event_count: extra.map_or(0, |extra| extra.event_count),
            citation_count: extra.map_or(0, |extra| extra.citation_count),
            media_count: extra.map_or(0, |extra| extra.media_count),
            task_count: extra.map_or(0, |extra| extra.task_count),
            group_count: extra.map_or(0, |extra| extra.group_count),
            association_count: extra.map_or(0, |extra| extra.association_count),
            updated_at: person.updated_at.clone(),
            display_name: if display_name.is_empty() {
                "(unnamed)".to_string()
            } else {
                display_name
            },
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

/// What the user asked the shell to do from the grid.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchEvent {
    Select(String),
    Edit(String),
    AddPerson,
    Delete(String),
    ColumnsChanged(Vec<PeopleColumnConfig>),
}

/// Filter/sort/column-dialog state.
#[derive(Debug, Clone)]
pub struct WatchState {
    pub filter: String,
    sort: Option<(PeopleColumn, bool)>,
    view: Vec<usize>,
    dirty: bool,
    show_columns: bool,
    column_draft: Vec<PeopleColumnConfig>,
    column_error: Option<String>,
}

impl Default for WatchState {
    fn default() -> Self {
        Self {
            filter: String::new(),
            sort: None,
            view: Vec::new(),
            dirty: true,
            show_columns: false,
            column_draft: Vec::new(),
            column_error: None,
        }
    }
}

impl WatchState {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn toggle_sort(&mut self, column: PeopleColumn) {
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
        [
            &row.prefix,
            &row.given_name,
            &row.middle_name,
            &row.surname,
            &row.suffix,
            &row.nickname,
            &row.notes_preview,
        ]
        .into_iter()
        .any(|value| value.to_lowercase().contains(&needle))
    }
}

fn compare(left: &PersonRow, right: &PersonRow, column: PeopleColumn) -> Ordering {
    match column {
        PeopleColumn::Events
        | PeopleColumn::Citations
        | PeopleColumn::Media
        | PeopleColumn::Tasks
        | PeopleColumn::Groups
        | PeopleColumn::Associations => numeric(
            Some(numeric_value(left, column)),
            Some(numeric_value(right, column)),
        ),
        _ => text(text_value(left, column), text_value(right, column)),
    }
}

fn text(left: Option<String>, right: Option<String>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.to_lowercase().cmp(&right.to_lowercase()),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn numeric(left: Option<i64>, right: Option<i64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn numeric_value(row: &PersonRow, column: PeopleColumn) -> i64 {
    match column {
        PeopleColumn::Events => row.event_count,
        PeopleColumn::Citations => row.citation_count,
        PeopleColumn::Media => row.media_count,
        PeopleColumn::Tasks => row.task_count,
        PeopleColumn::Groups => row.group_count,
        PeopleColumn::Associations => row.association_count,
        _ => 0,
    }
}

fn text_value(row: &PersonRow, column: PeopleColumn) -> Option<String> {
    let value = match column {
        PeopleColumn::Prefix => row.prefix.clone(),
        PeopleColumn::GivenName => row.given_name.clone(),
        PeopleColumn::MiddleName => row.middle_name.clone(),
        PeopleColumn::Surname => row.surname.clone(),
        PeopleColumn::Suffix => row.suffix.clone(),
        PeopleColumn::Nickname => row.nickname.clone(),
        PeopleColumn::Gender => display_gender(&row.gender).to_string(),
        PeopleColumn::BirthDate => row.birth.clone().unwrap_or_default(),
        PeopleColumn::BirthPlace => row.birth_place.clone().unwrap_or_default(),
        PeopleColumn::DeathDate => row.death.clone().unwrap_or_default(),
        PeopleColumn::DeathPlace => row.death_place.clone().unwrap_or_default(),
        PeopleColumn::AgeAtDeath => row.age_at_death.clone().unwrap_or_default(),
        PeopleColumn::Notes => row.notes_preview.clone(),
        PeopleColumn::UpdatedAt => row.updated_at.clone(),
        _ => return None,
    };
    (!value.is_empty()).then_some(value)
}

fn cell_value(row: &PersonRow, column: PeopleColumn) -> String {
    match column {
        PeopleColumn::Events
        | PeopleColumn::Citations
        | PeopleColumn::Media
        | PeopleColumn::Tasks
        | PeopleColumn::Groups
        | PeopleColumn::Associations => numeric_value(row, column).to_string(),
        _ => text_value(row, column).unwrap_or_default(),
    }
}

/// Renders the toolbar and configurable virtualized table.
pub fn show(
    ui: &mut Ui,
    state: &mut WatchState,
    rows: &[PersonRow],
    selected: Option<&str>,
    columns: &[PeopleColumnConfig],
) -> Vec<WatchEvent> {
    let mut events = Vec::new();
    ui.horizontal(|ui| {
        ui.label(tr("common.filter"));
        let response = ui
            .add(egui::TextEdit::singleline(&mut state.filter).hint_text(tr("people.filter_hint")));
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
        if ui.button(tr("people.columns")).clicked() {
            state.column_draft = columns.to_vec();
            state.column_error = None;
            state.show_columns = true;
        }
    });
    if state.show_columns {
        columns_dialog(ui, state, &mut events);
    }

    if state.dirty {
        state.rebuild(rows);
    }

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::i18n::tr_args(
                "people.showing",
                &[
                    ("shown", state.view.len().to_string()),
                    ("total", rows.len().to_string()),
                ],
            ))
            .weak()
            .size(12.0),
        );
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new(tr("people.add")).strong()).fill(ACCENT))
                .clicked()
            {
                events.push(WatchEvent::AddPerson);
            }
        });
    });

    if rows.is_empty() {
        ui.label(RichText::new(tr("people.empty")).weak().size(13.0));
        return events;
    }

    let visible: Vec<PeopleColumn> = columns
        .iter()
        .filter(|config| config.visible)
        .map(|config| config.column)
        .collect();
    if let Some((active, _)) = state.sort
        && !visible.contains(&active)
    {
        state.sort = None;
        state.dirty = true;
    }

    let mut sort_clicked: Option<PeopleColumn> = None;
    let mut builder = TableBuilder::new(ui).striped(true).resizable(true);
    for config in columns.iter().filter(|config| config.visible) {
        builder = builder.column(Column::initial(config.width).at_least(60.0));
    }
    builder = builder.column(Column::initial(140.0).at_least(120.0));
    builder
        .header(22.0, |mut header| {
            for column in &visible {
                header.col(|ui| {
                    let arrow = match state.sort {
                        Some((active, true)) if active == *column => " \u{25b2}",
                        Some((active, false)) if active == *column => " \u{25bc}",
                        _ => "",
                    };
                    let text = format!("{}{}", tr(column.label_key()), arrow);
                    let active = state.sort.map(|(active, _)| active) == Some(*column);
                    let label = egui::Label::new(if active {
                        RichText::new(text).strong()
                    } else {
                        RichText::new(text)
                    })
                    .sense(Sense::click());
                    if ui.add(label).clicked() {
                        sort_clicked = Some(*column);
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
                let Some(person) = rows.get(index) else {
                    return;
                };
                row.set_selected(selected == Some(person.id.as_str()));
                for column in &visible {
                    row.col(|ui| {
                        clickable_cell(ui, &cell_value(person, *column), &person.id, &mut events);
                    });
                }
                row.col(|ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button(tr("common.edit")).clicked() {
                            events.push(WatchEvent::Edit(person.id.clone()));
                        }
                        ui.menu_button(tr("common.delete"), |ui| {
                            ui.label(crate::i18n::tr_args(
                                "people.delete_confirm",
                                &[("name", person.display_name())],
                            ));
                            if ui.button(tr("people.confirm")).clicked() {
                                events.push(WatchEvent::Delete(person.id.clone()));
                                ui.close();
                            }
                            if ui.button(tr("common.cancel")).clicked() {
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

fn columns_dialog(ui: &mut Ui, state: &mut WatchState, events: &mut Vec<WatchEvent>) {
    let mut open = true;
    let mut apply = false;
    let mut reset = false;
    let mut cancel = false;
    let mut move_up = None;
    let mut move_down = None;
    egui::Window::new(tr("people.columns"))
        .id(egui::Id::new("people_columns_window"))
        .collapsible(false)
        .resizable(true)
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            let len = state.column_draft.len();
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    for (index, config) in state.column_draft.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut config.visible, tr(config.column.label_key()));
                            if ui.small_button("\u{25b2}").clicked() && index > 0 {
                                move_up = Some(index);
                            }
                            if ui.small_button("\u{25bc}").clicked() && index + 1 < len {
                                move_down = Some(index);
                            }
                            ui.add(
                                egui::DragValue::new(&mut config.width)
                                    .range(60.0..=500.0)
                                    .suffix(" px"),
                            );
                        });
                    }
                });
            if let Some(error) = &state.column_error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }
            ui.horizontal(|ui| {
                if ui.button(tr("people.reset_columns")).clicked() {
                    reset = true;
                }
                if ui.button(tr("people.apply")).clicked() {
                    apply = true;
                }
                if ui.button(tr("common.cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if let Some(index) = move_up {
        state.column_draft.swap(index, index - 1);
    }
    if let Some(index) = move_down {
        state.column_draft.swap(index, index + 1);
    }
    if reset {
        state.column_draft = default_columns();
    }
    if apply {
        let cleaned = sanitize_columns(&state.column_draft);
        events.push(WatchEvent::ColumnsChanged(cleaned));
        state.show_columns = false;
    } else if !open || cancel {
        state.show_columns = false;
    }
}

fn clickable_cell(ui: &mut Ui, text: &str, person_id: &str, events: &mut Vec<WatchEvent>) {
    let response = ui.add(egui::Label::new(text).sense(Sense::click()));
    if let Some(event) = watch_click(person_id, response.double_clicked(), response.clicked()) {
        events.push(event);
    }
    response.on_hover_text(tr("watchlist.double_click_edit"));
}

fn watch_click(person_id: &str, double_clicked: bool, clicked: bool) -> Option<WatchEvent> {
    if double_clicked {
        Some(WatchEvent::Edit(person_id.to_string()))
    } else if clicked {
        Some(WatchEvent::Select(person_id.to_string()))
    } else {
        None
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
        rows(&tree, &HashMap::new(), &HashMap::new(), false)
    }

    #[test]
    fn filter_matches_name_and_notes_case_insensitively() {
        let mut rows = rows_for(&[("Ada", "King"), ("Bo", "Tester"), ("Carl", "Tester")]);
        rows[0].notes_preview = "Correspondent".to_string();
        let mut state = WatchState {
            filter: "aDa".to_string(),
            ..WatchState::default()
        };
        state.rebuild(&rows);
        assert_eq!(state.view, vec![0]);
        state.filter = "correspondent".to_string();
        state.rebuild(&rows);
        assert_eq!(state.view, vec![0]);
        state.filter = "zzz".to_string();
        state.rebuild(&rows);
        assert!(state.view.is_empty());
    }

    #[test]
    fn sorting_supports_text_and_numeric_columns() {
        let mut rows = rows_for(&[("Bo", "Same"), ("Ada", "Same"), ("Carl", "Same")]);
        rows[0].event_count = 1;
        rows[1].event_count = 3;
        rows[2].event_count = 2;
        let mut state = WatchState::default();
        state.toggle_sort(PeopleColumn::GivenName);
        state.rebuild(&rows);
        assert_eq!(state.view, vec![1, 0, 2]);
        state.toggle_sort(PeopleColumn::GivenName);
        state.rebuild(&rows);
        assert_eq!(state.view, vec![2, 0, 1]);
        state.toggle_sort(PeopleColumn::Events);
        state.rebuild(&rows);
        assert_eq!(state.view, vec![0, 2, 1]);
    }

    #[test]
    fn visible_columns_drive_cell_order_and_values() {
        let mut rows = rows_for(&[("Ada", "King")]);
        rows[0].birth_place = Some("London".to_string());
        let columns = [
            PeopleColumnConfig {
                column: PeopleColumn::Surname,
                visible: true,
                width: 100.0,
            },
            PeopleColumnConfig {
                column: PeopleColumn::BirthPlace,
                visible: true,
                width: 100.0,
            },
        ];
        let visible: Vec<String> = columns
            .iter()
            .filter(|column| column.visible)
            .map(|column| cell_value(&rows[0], column.column))
            .collect();
        assert_eq!(visible, vec!["King".to_string(), "London".to_string()]);
    }

    #[test]
    fn click_events_prefer_double_click() {
        assert_eq!(
            watch_click("p1", true, true),
            Some(WatchEvent::Edit("p1".to_string()))
        );
        assert_eq!(
            watch_click("p1", false, true),
            Some(WatchEvent::Select("p1".to_string()))
        );
        assert_eq!(watch_click("p1", false, false), None);
    }

    #[test]
    fn people_view_renders_headless() {
        let rows = rows_for(&[("Ada", "King")]);
        let ctx = Context::default();
        let mut state = WatchState::default();
        let mut captured = Vec::new();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            captured = show(ui, &mut state, &rows, None, &default_columns());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the grid paints");
        assert!(captured.is_empty());
    }
}
