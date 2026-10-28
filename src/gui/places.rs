//! Places tab: a sortable grid and place editor.

use std::cmp::Ordering;

use egui::{RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::db::places::{PlaceDraft, PlaceRecord, PlaceSummary, PlaceUsage, reverse_name};

/// What the user asked the shell to do from the places tab.
#[derive(Debug, Clone, PartialEq)]
pub enum PlacesEvent {
    New,
    Select(String),
    Save(Box<PlaceDraft>),
    Delete(String),
    SelectPerson(String),
    Notes { place_id: String, title: String },
    Media { place_id: String, title: String },
}

/// Columns the grid header can sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Name,
    Events,
    Latitude,
    Longitude,
    Notes,
}

impl SortColumn {
    fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Events => "Events",
            Self::Latitude => "Latitude",
            Self::Longitude => "Longitude",
            Self::Notes => "Notes",
        }
    }
}

/// A place buffered as text while the editor is open.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaceEdit {
    pub id: Option<String>,
    pub name: String,
    pub latitude: String,
    pub longitude: String,
}

/// State owned by the window while the places tab is open.
#[derive(Debug, Default)]
pub struct PlacesState {
    pub places: Vec<PlaceSummary>,
    pub selected: Option<String>,
    pub draft: PlaceEdit,
    pub usage: Vec<PlaceUsage>,
    pub filter: String,
    pub sort: Option<(SortColumn, bool)>,
    pub editing: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

/// Converts a stored place into its text draft.
pub fn draft_from_record(record: &PlaceRecord) -> PlaceEdit {
    PlaceEdit {
        id: Some(record.id.clone()),
        name: record.name.clone(),
        latitude: format_coordinate(record.latitude),
        longitude: format_coordinate(record.longitude),
    }
}

impl PlaceEdit {
    /// Parses the text fields into a persistable draft.
    pub fn to_draft(&self) -> Result<PlaceDraft, String> {
        Ok(PlaceDraft {
            id: self.id.clone(),
            name: self.name.clone(),
            latitude: parse_coordinate(&self.latitude, 90.0, "Latitude")?,
            longitude: parse_coordinate(&self.longitude, 180.0, "Longitude")?,
        })
    }
}

fn format_coordinate(value: Option<f64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn parse_coordinate(text: &str, limit: f64, label: &str) -> Result<Option<f64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let value = text
        .parse::<f64>()
        .map_err(|_| format!("{label} must be a number"))?;
    if !value.is_finite() || !(-limit..=limit).contains(&value) {
        return Err(format!("{label} must be between -{limit} and {limit}"));
    }
    Ok(Some(value))
}

/// Draws the places tab and returns the action the window should apply.
pub fn show(ui: &mut Ui, state: &mut PlacesState, reverse: bool) -> Option<PlacesEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text("Filter by name or notes…"));
        if ui
            .add_enabled(!state.filter.is_empty(), egui::Button::new("Clear"))
            .clicked()
        {
            state.filter.clear();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("+ New Place").strong()))
                .clicked()
            {
                event = Some(PlacesEvent::New);
            }
        });
    });

    let visible = visible_indices(state, reverse);
    ui.label(
        RichText::new(format!(
            "Showing {} of {} places",
            visible.len(),
            state.places.len()
        ))
        .weak()
        .size(12.0),
    );

    let available = ui.available_height();
    let grid_height = (available * 0.45).max(140.0).min(available.max(0.0));
    ui.allocate_ui(egui::vec2(ui.available_width(), grid_height), |ui| {
        if state.places.is_empty() {
            ui.label(
                RichText::new("No places yet — create one with the button above.")
                    .weak()
                    .size(13.0),
            );
            return;
        }
        let mut sort_clicked = None;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::auto().at_least(160.0))
            .column(Column::auto().at_least(60.0))
            .column(Column::auto().at_least(90.0))
            .column(Column::auto().at_least(90.0))
            .column(Column::remainder().at_least(140.0))
            .column(Column::auto().at_least(120.0))
            .header(22.0, |mut header| {
                for column in [
                    SortColumn::Name,
                    SortColumn::Events,
                    SortColumn::Latitude,
                    SortColumn::Longitude,
                    SortColumn::Notes,
                ] {
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
                header.col(|ui| {
                    ui.label(RichText::new("Actions").strong());
                });
            })
            .body(|body| {
                body.rows(24.0, visible.len(), |mut row| {
                    let Some(&index) = visible.get(row.index()) else {
                        return;
                    };
                    let Some(place) = state.places.get(index) else {
                        return;
                    };
                    let selected = state.selected.as_deref() == Some(place.id.as_str());
                    row.set_selected(selected);
                    row.col(|ui| {
                        let name = display_name(place, reverse);
                        selectable_cell(ui, &name, &place.id, &mut event);
                    });
                    row.col(|ui| {
                        if ui
                            .add(
                                egui::Label::new(place.event_count.to_string())
                                    .sense(Sense::click()),
                            )
                            .clicked()
                        {
                            event = Some(PlacesEvent::Select(place.id.clone()));
                        }
                    });
                    row.col(|ui| {
                        coordinate_cell(ui, place.latitude, &place.id, &mut event);
                    });
                    row.col(|ui| {
                        coordinate_cell(ui, place.longitude, &place.id, &mut event);
                    });
                    row.col(|ui| {
                        let notes = place.notes.as_deref().unwrap_or_default();
                        let first = notes.lines().next().unwrap_or_default();
                        let response =
                            ui.add(egui::Label::new(first).sense(Sense::click()).truncate());
                        let clicked = response.clicked();
                        if !notes.is_empty() {
                            response.on_hover_text(notes);
                        }
                        if clicked {
                            event = Some(PlacesEvent::Select(place.id.clone()));
                        }
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("Edit").clicked() {
                                event = Some(PlacesEvent::Select(place.id.clone()));
                            }
                            ui.menu_button("Delete", |ui| {
                                ui.label(format!("Delete {}?", place.name));
                                ui.label(
                                    RichText::new(format!(
                                        "{} events will lose their place.",
                                        place.event_count
                                    ))
                                    .weak()
                                    .small(),
                                );
                                if ui.button("Confirm").clicked() {
                                    event = Some(PlacesEvent::Delete(place.id.clone()));
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
            toggle_sort(state, column);
        }
    });

    ui.separator();
    let editor_height = ui.available_height().max(0.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), editor_height), |ui| {
        editor(ui, state, &mut event, reverse);
    });
    let remaining = ui.available_height();
    if remaining > 0.0 {
        ui.allocate_space(egui::vec2(ui.available_width(), remaining));
    }
    event
}

fn editor(ui: &mut Ui, state: &mut PlacesState, event: &mut Option<PlacesEvent>, reverse: bool) {
    if !state.editing {
        ui.label(
            RichText::new("Select a place or create a new one.")
                .weak()
                .size(13.0),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("place_editor")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(
                RichText::new(if state.draft.id.is_some() {
                    "Edit place"
                } else {
                    "New place"
                })
                .strong(),
            );
            let event_count = state
                .selected
                .as_deref()
                .and_then(|id| state.places.iter().find(|place| place.id == id))
                .map(|place| place.event_count);
            if let Some(count) = event_count {
                ui.label(
                    RichText::new(format!("{count} events use this place"))
                        .weak()
                        .small(),
                );
            }

            egui::Grid::new("place_fields")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Name *").strong());
                    ui.text_edit_singleline(&mut state.draft.name);
                    ui.end_row();

                    ui.label("Latitude:");
                    ui.text_edit_singleline(&mut state.draft.latitude);
                    ui.end_row();

                    ui.label("Longitude:");
                    ui.text_edit_singleline(&mut state.draft.longitude);
                    ui.end_row();
                });
            if let Some(id) = state.draft.id.clone() {
                if ui.button("Notes\u{2026}").clicked() {
                    *event = Some(PlacesEvent::Notes {
                        place_id: id.clone(),
                        title: format!("Notes \u{2014} {}", state.draft.name.trim()),
                    });
                }
                if ui.button("Media\u{2026}").clicked() {
                    *event = Some(PlacesEvent::Media {
                        place_id: id,
                        title: format!("Media \u{2014} {}", state.draft.name.trim()),
                    });
                }
            }
            ui.label(
                RichText::new("Decimal degrees; leave blank when unknown.")
                    .weak()
                    .small(),
            );
            if reverse && !state.draft.name.trim().is_empty() {
                ui.label(
                    RichText::new(format!(
                        "Reversed name: {}",
                        reverse_name(&state.draft.name)
                    ))
                    .weak()
                    .small(),
                );
            }

            let latitude = parse_coordinate(&state.draft.latitude, 90.0, "Latitude");
            let longitude = parse_coordinate(&state.draft.longitude, 180.0, "Longitude");
            for error in [latitude.as_ref().err(), longitude.as_ref().err()]
                .into_iter()
                .flatten()
            {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }
            if let Some(error) = &state.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }

            let valid =
                !state.draft.name.trim().is_empty() && latitude.is_ok() && longitude.is_ok();
            if ui
                .add_enabled(valid, egui::Button::new(RichText::new("Save").strong()))
                .clicked()
                && let (Ok(latitude), Ok(longitude)) = (latitude, longitude)
            {
                *event = Some(PlacesEvent::Save(Box::new(PlaceDraft {
                    id: state.draft.id.clone(),
                    name: state.draft.name.clone(),
                    latitude,
                    longitude,
                })));
            }

            ui.separator();
            ui.label(RichText::new(format!("Used by {} events", state.usage.len())).strong());
            if state.usage.is_empty() {
                ui.label(RichText::new("No events use this place.").weak().small());
            } else {
                ui.label(
                    RichText::new("Select a person to edit them.")
                        .weak()
                        .small(),
                );
                for usage in &state.usage {
                    usage_row(ui, usage, event);
                }
            }
        });
}

fn usage_row(ui: &mut Ui, usage: &PlaceUsage, event: &mut Option<PlacesEvent>) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&usage.event_label).strong());
        if let Some(date) = usage
            .date
            .as_deref()
            .map(str::trim)
            .filter(|date| !date.is_empty())
        {
            ui.label(RichText::new(date).weak().small());
        }
        if let Some(person_id) = &usage.person_id {
            let name = usage.person_name.as_deref().unwrap_or("(unnamed)");
            if ui.button(name).on_hover_text("Edit this person").clicked() {
                *event = Some(PlacesEvent::SelectPerson(person_id.clone()));
            }
            return;
        }
        if usage.family_id.is_none() {
            ui.label(RichText::new("Unlinked event").weak().small());
            return;
        }
        let mut shown = false;
        if let Some(husband_id) = &usage.husband_id {
            let name = usage.husband_name.as_deref().unwrap_or("(unnamed)");
            if ui.button(name).on_hover_text("Edit this person").clicked() {
                *event = Some(PlacesEvent::SelectPerson(husband_id.clone()));
            }
            shown = true;
        }
        if usage.husband_id.is_some() && usage.wife_id.is_some() {
            ui.label("&");
        }
        if let Some(wife_id) = &usage.wife_id {
            let name = usage.wife_name.as_deref().unwrap_or("(unnamed)");
            if ui.button(name).on_hover_text("Edit this person").clicked() {
                *event = Some(PlacesEvent::SelectPerson(wife_id.clone()));
            }
            shown = true;
        }
        if !shown {
            ui.label(RichText::new("Unnamed couple").weak().small());
        }
    });
}

fn coordinate_cell(ui: &mut Ui, value: Option<f64>, id: &str, event: &mut Option<PlacesEvent>) {
    let text = value.map(|value| value.to_string()).unwrap_or_default();
    if ui
        .add(egui::Label::new(text).sense(Sense::click()))
        .clicked()
    {
        *event = Some(PlacesEvent::Select(id.to_string()));
    }
}

fn visible_indices(state: &PlacesState, reverse: bool) -> Vec<usize> {
    let needle = state.filter.trim().to_lowercase();
    let mut indices: Vec<usize> = state
        .places
        .iter()
        .enumerate()
        .filter(|(_, place)| {
            if needle.is_empty() {
                return true;
            }
            display_name(place, reverse)
                .to_lowercase()
                .contains(&needle)
                || place
                    .notes
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&needle)
        })
        .map(|(index, _)| index)
        .collect();
    if let Some((column, ascending)) = state.sort {
        indices.sort_by(|&left, &right| {
            let (Some(left), Some(right)) = (state.places.get(left), state.places.get(right))
            else {
                return Ordering::Equal;
            };
            let order = match column {
                SortColumn::Name => display_name(left, reverse)
                    .to_lowercase()
                    .cmp(&display_name(right, reverse).to_lowercase()),
                SortColumn::Events => left.event_count.cmp(&right.event_count),
                SortColumn::Latitude => compare_coordinate(left.latitude, right.latitude),
                SortColumn::Longitude => compare_coordinate(left.longitude, right.longitude),
                SortColumn::Notes => left
                    .notes
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .cmp(&right.notes.as_deref().unwrap_or_default().to_lowercase()),
            };
            if ascending { order } else { order.reverse() }
        });
    }
    indices
}

fn display_name(place: &PlaceSummary, reverse: bool) -> String {
    if reverse {
        reverse_name(&place.name)
    } else {
        place.name.clone()
    }
}

fn compare_coordinate(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.partial_cmp(&right).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

fn toggle_sort(state: &mut PlacesState, column: SortColumn) {
    state.sort = match state.sort {
        Some((active, ascending)) if active == column => Some((active, !ascending)),
        _ => Some((column, true)),
    };
}

fn selectable_cell(ui: &mut Ui, text: &str, id: &str, event: &mut Option<PlacesEvent>) {
    if ui
        .add(egui::Label::new(text).sense(Sense::click()))
        .clicked()
    {
        *event = Some(PlacesEvent::Select(id.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn record() -> PlaceRecord {
        PlaceRecord {
            id: "p1".to_string(),
            name: "Uppsala, Sweden".to_string(),
            latitude: Some(59.8586),
            longitude: Some(17.6389),
        }
    }

    fn summary(id: &str, name: &str, events: i64) -> PlaceSummary {
        PlaceSummary {
            id: id.to_string(),
            name: name.to_string(),
            notes: None,
            latitude: None,
            longitude: None,
            event_count: events,
        }
    }

    #[test]
    fn drafts_round_trip_coordinates() {
        let draft = draft_from_record(&record());
        assert_eq!(draft.latitude, "59.8586");
        assert_eq!(draft.longitude, "17.6389");
        let parsed = draft.to_draft().expect("draft parses");
        assert_eq!(parsed.id.as_deref(), Some("p1"));
        assert_eq!(parsed.latitude, Some(59.8586));
        assert_eq!(parsed.longitude, Some(17.6389));
    }

    #[test]
    fn rejects_bad_coordinate_text() {
        let mut draft = draft_from_record(&record());
        draft.latitude = "north".to_string();
        assert!(draft.to_draft().is_err());
        draft.latitude = "91".to_string();
        assert!(draft.to_draft().is_err());
        draft.latitude.clear();
        draft.longitude = "181".to_string();
        assert!(draft.to_draft().is_err());
        draft.longitude.clear();
        assert!(draft.to_draft().is_ok());
    }

    #[test]
    fn reversed_names_drive_display_and_filtering() {
        let mut state = PlacesState {
            places: vec![summary("p1", "Uppsala, Sweden", 0)],
            ..PlacesState::default()
        };
        assert_eq!(display_name(&state.places[0], true), "Sweden, Uppsala");
        state.filter = "sweden, uppsala".to_string();
        assert_eq!(visible_indices(&state, true), vec![0]);
        assert!(visible_indices(&state, false).is_empty());
    }

    #[test]
    fn filters_and_sorts_the_grid() {
        let mut state = PlacesState {
            places: vec![summary("p1", "Uppsala", 1), summary("p2", "Lund", 3)],
            ..PlacesState::default()
        };
        assert_eq!(visible_indices(&state, false), vec![0, 1]);
        state.filter = "lund".to_string();
        assert_eq!(visible_indices(&state, false), vec![1]);
        state.filter.clear();
        toggle_sort(&mut state, SortColumn::Events);
        assert_eq!(visible_indices(&state, false), vec![0, 1]);
        toggle_sort(&mut state, SortColumn::Events);
        assert_eq!(visible_indices(&state, false), vec![1, 0]);
    }

    #[test]
    fn places_tab_fills_the_available_height() {
        let ctx = Context::default();
        let mut state = PlacesState {
            places: vec![summary("p1", "Uppsala", 0)],
            ..PlacesState::default()
        };
        let mut fills = false;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let available = ui.available_height();
            show(ui, &mut state, false);
            fills = ui.min_rect().height() >= available - 1.0;
        });
        output.textures_delta.clear();
        assert!(fills, "the places tab must fill the available height");
    }

    #[test]
    fn usage_rows_render_person_and_couple_buttons() {
        let ctx = Context::default();
        let mut state = PlacesState {
            places: vec![summary("p1", "Uppsala", 2)],
            selected: Some("p1".to_string()),
            draft: draft_from_record(&record()),
            usage: vec![
                PlaceUsage {
                    event_id: "e1".to_string(),
                    event_type: "BIRTH".to_string(),
                    event_label: "Birth".to_string(),
                    date: Some("1850".to_string()),
                    person_id: Some("s1".to_string()),
                    person_name: Some("Ada King".to_string()),
                    family_id: None,
                    husband_id: None,
                    husband_name: None,
                    wife_id: None,
                    wife_name: None,
                },
                PlaceUsage {
                    event_id: "e2".to_string(),
                    event_type: "MARRIAGE".to_string(),
                    event_label: "Marriage".to_string(),
                    date: Some("1875".to_string()),
                    person_id: None,
                    person_name: None,
                    family_id: Some("f1".to_string()),
                    husband_id: Some("s2".to_string()),
                    husband_name: Some("Otto King".to_string()),
                    wife_id: Some("s1".to_string()),
                    wife_name: Some("Ada King".to_string()),
                },
            ],
            editing: true,
            ..PlacesState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "usage rows paint");
        assert_eq!(state.usage.len(), 2);
    }

    #[test]
    fn places_tab_renders_headless() {
        let ctx = Context::default();
        let mut state = PlacesState {
            places: vec![summary("p1", "Uppsala", 1)],
            selected: Some("p1".to_string()),
            draft: draft_from_record(&record()),
            editing: true,
            ..PlacesState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the places tab paints");
        assert!(!state.loaded, "the test drives loading separately");
    }
}
