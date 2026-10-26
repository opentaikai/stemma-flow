//! Repositories tab: a sortable grid and repository editor.

use std::cmp::Ordering;
use std::collections::HashSet;

use egui::{RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::db::repositories::{
    RepositoryDraft, RepositoryLinkDraft, RepositoryRecord, RepositorySummary,
};
use crate::db::sources::SourceSummary;

/// What the user asked the shell to do from the repositories tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoriesEvent {
    New,
    Select(String),
    Save(Box<RepositoryDraft>),
    Delete(String),
}

/// Columns the grid header can sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Name,
    Address,
    Sources,
}

impl SortColumn {
    fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Address => "Address",
            Self::Sources => "Sources",
        }
    }
}

/// State owned by the window while the repositories tab is open.
#[derive(Debug, Default)]
pub struct RepositoriesState {
    pub repositories: Vec<RepositorySummary>,
    pub sources: Vec<SourceSummary>,
    pub selected: Option<String>,
    pub draft: RepositoryDraft,
    pub filter: String,
    pub sort: Option<(SortColumn, bool)>,
    pub add_source: Option<String>,
    pub add_call_number: String,
    pub add_media_type: String,
    pub editing: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

/// Converts a stored repository into its editable draft, keeping links.
pub fn draft_from_record(record: &RepositoryRecord) -> RepositoryDraft {
    RepositoryDraft {
        id: Some(record.id.clone()),
        name: record.name.clone(),
        address: record.address.clone().unwrap_or_default(),
        notes: record.notes.clone().unwrap_or_default(),
        links: record
            .links
            .iter()
            .map(|link| RepositoryLinkDraft {
                source_id: link.source_id.clone(),
                call_number: link.call_number.clone().unwrap_or_default(),
                media_type: link.media_type.clone().unwrap_or_default(),
            })
            .collect(),
    }
}

/// Draws the repositories tab and returns the action the window should apply.
pub fn show(ui: &mut Ui, state: &mut RepositoriesState) -> Option<RepositoriesEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.add(
            egui::TextEdit::singleline(&mut state.filter).hint_text("Filter by name or address…"),
        );
        if ui
            .add_enabled(!state.filter.is_empty(), egui::Button::new("Clear"))
            .clicked()
        {
            state.filter.clear();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(
                    RichText::new("+ New Repository").strong(),
                ))
                .clicked()
            {
                event = Some(RepositoriesEvent::New);
            }
        });
    });

    let visible = visible_indices(state);
    ui.label(
        RichText::new(format!(
            "Showing {} of {} repositories",
            visible.len(),
            state.repositories.len()
        ))
        .weak()
        .size(12.0),
    );

    let grid_height = (ui.available_height() * 0.45).max(140.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), grid_height), |ui| {
        if state.repositories.is_empty() {
            ui.label(
                RichText::new("No repositories yet — create one with the button above.")
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
            .column(Column::remainder().at_least(160.0))
            .column(Column::auto().at_least(70.0))
            .column(Column::auto().at_least(120.0))
            .header(22.0, |mut header| {
                for column in [SortColumn::Name, SortColumn::Address, SortColumn::Sources] {
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
                    let Some(repository) = state.repositories.get(index) else {
                        return;
                    };
                    let selected = state.selected.as_deref() == Some(repository.id.as_str());
                    row.set_selected(selected);
                    row.col(|ui| {
                        selectable_cell(ui, &repository.name, &repository.id, &mut event);
                    });
                    row.col(|ui| {
                        let address = repository.address.as_deref().unwrap_or_default();
                        let first = address.lines().next().unwrap_or_default();
                        let response =
                            ui.add(egui::Label::new(first).sense(Sense::click()).truncate());
                        let clicked = response.clicked();
                        if !address.is_empty() {
                            response.on_hover_text(address);
                        }
                        if clicked {
                            event = Some(RepositoriesEvent::Select(repository.id.clone()));
                        }
                    });
                    row.col(|ui| {
                        if ui
                            .add(
                                egui::Label::new(repository.source_count.to_string())
                                    .sense(Sense::click()),
                            )
                            .clicked()
                        {
                            event = Some(RepositoriesEvent::Select(repository.id.clone()));
                        }
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("Edit").clicked() {
                                event = Some(RepositoriesEvent::Select(repository.id.clone()));
                            }
                            ui.menu_button("Delete", |ui| {
                                ui.label(format!("Delete {}?", repository.name));
                                ui.label(
                                    RichText::new(format!(
                                        "{} source links will be removed.",
                                        repository.source_count
                                    ))
                                    .weak()
                                    .small(),
                                );
                                if ui.button("Confirm").clicked() {
                                    event = Some(RepositoriesEvent::Delete(repository.id.clone()));
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
    let editor_height = ui.available_height();
    ui.allocate_ui(egui::vec2(ui.available_width(), editor_height), |ui| {
        editor(ui, state, &mut event);
    });
    let remaining = ui.available_height();
    if remaining > 0.0 {
        ui.allocate_space(egui::vec2(ui.available_width(), remaining));
    }
    event
}

fn editor(ui: &mut Ui, state: &mut RepositoriesState, event: &mut Option<RepositoriesEvent>) {
    if !state.editing {
        ui.label(
            RichText::new("Select a repository or create a new one.")
                .weak()
                .size(13.0),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("repository_editor")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(
                RichText::new(if state.draft.id.is_some() {
                    "Edit repository"
                } else {
                    "New repository"
                })
                .strong(),
            );
            egui::Grid::new("repository_fields")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Name *").strong());
                    ui.text_edit_singleline(&mut state.draft.name);
                    ui.end_row();

                    ui.label("Address:");
                    ui.add(
                        egui::TextEdit::multiline(&mut state.draft.address)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();

                    ui.label("Notes:");
                    ui.add(
                        egui::TextEdit::multiline(&mut state.draft.notes)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                });

            ui.separator();
            ui.label(RichText::new("Linked sources").strong());
            if state.draft.links.is_empty() {
                ui.label(RichText::new("No sources linked.").weak().small());
            }
            let mut remove = None;
            let titles: std::collections::BTreeMap<String, String> = state
                .sources
                .iter()
                .map(|source| (source.id.clone(), source.title.clone()))
                .collect();
            for (index, link) in state.draft.links.iter_mut().enumerate() {
                ui.push_id(("repository-link", &link.source_id), |ui| {
                    ui.horizontal(|ui| {
                        let title = titles
                            .get(&link.source_id)
                            .map(String::as_str)
                            .unwrap_or("(missing source)");
                        ui.label(title);
                        ui.label("Call:");
                        ui.text_edit_singleline(&mut link.call_number);
                        ui.label("Media:");
                        ui.text_edit_singleline(&mut link.media_type);
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                });
            }
            if let Some(index) = remove {
                state.draft.links.remove(index);
            }

            let linked: HashSet<&str> = state
                .draft
                .links
                .iter()
                .map(|link| link.source_id.as_str())
                .collect();
            if state.sources.is_empty() {
                ui.label(
                    RichText::new("Create a source in the Sources tab to link it here.")
                        .weak()
                        .small(),
                );
            } else {
                if state
                    .add_source
                    .as_deref()
                    .is_some_and(|id| linked.contains(id))
                {
                    state.add_source = None;
                }
                let choices: Vec<(String, String)> = state
                    .sources
                    .iter()
                    .filter(|source| !linked.contains(source.id.as_str()))
                    .map(|source| (source.id.clone(), source.title.clone()))
                    .collect();
                let current = state
                    .add_source
                    .as_deref()
                    .and_then(|id| choices.iter().find(|(source_id, _)| source_id == id))
                    .map(|(_, title)| title.clone())
                    .unwrap_or_else(|| "Select a source".to_string());
                ui.horizontal(|ui| {
                    ui.label("Add source:");
                    egui::ComboBox::from_id_salt("repository_add_source")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (source_id, title) in &choices {
                                if ui
                                    .selectable_label(
                                        state.add_source.as_deref() == Some(source_id.as_str()),
                                        title,
                                    )
                                    .clicked()
                                {
                                    state.add_source = Some(source_id.clone());
                                }
                            }
                        });
                    ui.label("Call:");
                    ui.text_edit_singleline(&mut state.add_call_number);
                    ui.label("Media:");
                    ui.text_edit_singleline(&mut state.add_media_type);
                    let enabled = state.add_source.is_some();
                    if ui
                        .add_enabled(enabled, egui::Button::new("Add link"))
                        .clicked()
                    {
                        add_link(state);
                    }
                });
            }

            if let Some(error) = &state.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }

            let valid = !state.draft.name.trim().is_empty();
            if ui
                .add_enabled(valid, egui::Button::new(RichText::new("Save").strong()))
                .clicked()
            {
                *event = Some(RepositoriesEvent::Save(Box::new(state.draft.clone())));
            }
        });
}

fn add_link(state: &mut RepositoriesState) {
    let Some(source_id) = state.add_source.clone() else {
        return;
    };
    if state
        .draft
        .links
        .iter()
        .any(|link| link.source_id == source_id)
    {
        return;
    }
    state.draft.links.push(RepositoryLinkDraft {
        source_id,
        call_number: std::mem::take(&mut state.add_call_number),
        media_type: std::mem::take(&mut state.add_media_type),
    });
    state.add_source = None;
}

fn visible_indices(state: &RepositoriesState) -> Vec<usize> {
    let needle = state.filter.trim().to_lowercase();
    let mut indices: Vec<usize> = state
        .repositories
        .iter()
        .enumerate()
        .filter(|(_, repository)| {
            if needle.is_empty() {
                return true;
            }
            repository.name.to_lowercase().contains(&needle)
                || repository
                    .address
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&needle)
        })
        .map(|(index, _)| index)
        .collect();
    if let Some((column, ascending)) = state.sort {
        indices.sort_by(|&left, &right| {
            let (Some(left), Some(right)) =
                (state.repositories.get(left), state.repositories.get(right))
            else {
                return Ordering::Equal;
            };
            let order = match column {
                SortColumn::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                SortColumn::Address => left
                    .address
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .cmp(&right.address.as_deref().unwrap_or_default().to_lowercase()),
                SortColumn::Sources => left.source_count.cmp(&right.source_count),
            };
            if ascending { order } else { order.reverse() }
        });
    }
    indices
}

fn toggle_sort(state: &mut RepositoriesState, column: SortColumn) {
    state.sort = match state.sort {
        Some((active, ascending)) if active == column => Some((active, !ascending)),
        _ => Some((column, true)),
    };
}

fn selectable_cell(ui: &mut Ui, text: &str, id: &str, event: &mut Option<RepositoriesEvent>) {
    if ui
        .add(egui::Label::new(text).sense(Sense::click()))
        .clicked()
    {
        *event = Some(RepositoriesEvent::Select(id.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn record() -> RepositoryRecord {
        RepositoryRecord {
            id: "r1".to_string(),
            name: "Uppsala Archive".to_string(),
            address: Some("Main Street\nUppsala".to_string()),
            notes: Some("Reading room".to_string()),
            links: vec![crate::db::repositories::RepositorySourceLink {
                source_id: "s1".to_string(),
                source_title: "Parish Book".to_string(),
                call_number: Some("12/34".to_string()),
                media_type: Some("film".to_string()),
            }],
        }
    }

    fn summary(
        id: &str,
        name: &str,
        address: Option<&str>,
        source_count: i64,
    ) -> RepositorySummary {
        RepositorySummary {
            id: id.to_string(),
            name: name.to_string(),
            address: address.map(str::to_string),
            source_count,
        }
    }

    #[test]
    fn drafts_keep_existing_links() {
        let draft = draft_from_record(&record());
        assert_eq!(draft.id.as_deref(), Some("r1"));
        assert_eq!(draft.address, "Main Street\nUppsala");
        assert_eq!(draft.links.len(), 1);
        assert_eq!(draft.links[0].source_id, "s1");
        assert_eq!(draft.links[0].call_number, "12/34");
        assert_eq!(draft.links[0].media_type, "film");
    }

    #[test]
    fn filters_and_sorts_the_grid() {
        let mut state = RepositoriesState {
            repositories: vec![
                summary("r1", "Uppsala Archive", Some("Main Street"), 2),
                summary("r2", "Lund Library", Some("Second Street"), 5),
            ],
            ..RepositoriesState::default()
        };
        assert_eq!(visible_indices(&state), vec![0, 1]);

        state.filter = "lund".to_string();
        assert_eq!(visible_indices(&state), vec![1]);
        state.filter.clear();

        toggle_sort(&mut state, SortColumn::Sources);
        assert_eq!(visible_indices(&state), vec![0, 1]);
        toggle_sort(&mut state, SortColumn::Sources);
        assert_eq!(visible_indices(&state), vec![1, 0]);
    }

    #[test]
    fn adds_and_removes_source_links() {
        let mut state = RepositoriesState {
            sources: vec![SourceSummary {
                id: "s1".to_string(),
                title: "Parish Book".to_string(),
                template_key: None,
                citation_count: 0,
            }],
            ..RepositoriesState::default()
        };
        state.add_source = Some("s1".to_string());
        state.add_call_number = "12/34".to_string();
        state.add_media_type = "film".to_string();

        add_link(&mut state);

        assert_eq!(state.draft.links.len(), 1);
        assert_eq!(state.draft.links[0].source_id, "s1");
        assert_eq!(state.draft.links[0].call_number, "12/34");
        assert_eq!(state.draft.links[0].media_type, "film");
        assert!(state.add_source.is_none());
        assert!(state.add_call_number.is_empty());

        state.draft.links.clear();
        assert!(state.draft.links.is_empty());
    }

    #[test]
    fn repositories_tab_fills_the_available_height() {
        let ctx = Context::default();
        let mut state = RepositoriesState {
            repositories: vec![summary("r1", "Uppsala Archive", Some("Main Street"), 0)],
            ..RepositoriesState::default()
        };
        let mut fills = false;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let available = ui.available_height();
            show(ui, &mut state);
            fills = ui.min_rect().height() >= available - 1.0;
        });
        output.textures_delta.clear();
        assert!(fills, "the repositories tab must fill the available height");
    }

    #[test]
    fn repositories_tab_renders_headless() {
        let ctx = Context::default();
        let mut state = RepositoriesState {
            repositories: vec![summary("r1", "Uppsala Archive", Some("Main Street"), 1)],
            sources: vec![SourceSummary {
                id: "s1".to_string(),
                title: "Parish Book".to_string(),
                template_key: None,
                citation_count: 1,
            }],
            selected: Some("r1".to_string()),
            draft: draft_from_record(&record()),
            editing: true,
            ..RepositoriesState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the repositories tab paints");
        assert!(!state.loaded, "the test drives loading separately");
    }
}
