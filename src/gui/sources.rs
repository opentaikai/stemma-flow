//! Sources tab: template-driven and free-form source editing.

use std::collections::{BTreeMap, BTreeSet};

use crate::db::sources::{SourceDraft, SourceSummary};
use crate::model::source_template::{
    FieldScope, SourceTemplate, TemplateField, gedcom_columns, render_citation, render_free_form,
};

/// Action requested by the sources tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcesEvent {
    New,
    Select(String),
    Save(Box<SourceDraft>),
    Delete(String),
    Merge {
        source_id: String,
        target_id: String,
    },
}

/// State owned by the window while the sources tab is open.
#[derive(Debug, Default)]
pub struct SourcesState {
    pub templates: Vec<SourceTemplate>,
    pub list: Vec<SourceSummary>,
    pub selected: Option<String>,
    pub draft: SourceDraft,
    pub search: String,
    /// Citation-scoped fields used only for the live preview.
    pub preview: BTreeMap<String, String>,
    pub merge_open: bool,
    pub merge_target: Option<String>,
    pub loaded: bool,
    pub error: Option<String>,
}

/// Draws the sources tab and returns the action the window should apply.
pub fn show(ui: &mut egui::Ui, state: &mut SourcesState) -> Option<SourcesEvent> {
    let mut event = None;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(250.0);
            ui.label(egui::RichText::new("Sources").strong());
            ui.add(
                egui::TextEdit::singleline(&mut state.search).hint_text("Filter sources\u{2026}"),
            );
            if ui.button("+ New Source").clicked() {
                event = Some(SourcesEvent::New);
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("source_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if state.list.is_empty() {
                        ui.label(egui::RichText::new("No sources yet.").weak());
                    }
                    let needle = state.search.trim().to_lowercase();
                    for summary in &state.list {
                        if !needle.is_empty() && !summary.title.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let selected = state.selected.as_deref() == Some(summary.id.as_str());
                        let label =
                            format!("{}  ({} citations)", summary.title, summary.citation_count);
                        if ui.selectable_label(selected, label).clicked() {
                            event = Some(SourcesEvent::Select(summary.id.clone()));
                        }
                    }
                });
        });
        ui.separator();
        ui.vertical(|ui| {
            egui::ScrollArea::vertical()
                .id_salt("source_editor")
                .auto_shrink([false, false])
                .show(ui, |ui| editor(ui, state, &mut event));
        });
    });
    event
}

fn editor(ui: &mut egui::Ui, state: &mut SourcesState, event: &mut Option<SourcesEvent>) {
    merge_panel(ui, state, event);
    let current_key = state.draft.template_key.clone();
    let current_name = current_key
        .as_deref()
        .and_then(|key| state.templates.iter().find(|template| template.key == key))
        .map(|template| format!("{} - {}", template.category, template.name))
        .unwrap_or_else(|| "Free-form".to_string());
    let choices: Vec<(String, String)> = state
        .templates
        .iter()
        .map(|template| {
            (
                template.key.clone(),
                format!("{} - {}", template.category, template.name),
            )
        })
        .collect();

    ui.horizontal(|ui| {
        ui.label("Template:");
        egui::ComboBox::from_id_salt("source_template_picker")
            .selected_text(current_name)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(current_key.is_none(), "Free-form")
                    .clicked()
                {
                    set_template(state, None);
                }
                for (key, label) in &choices {
                    let selected = current_key.as_deref() == Some(key.as_str());
                    if ui.selectable_label(selected, label).clicked() {
                        set_template(state, Some(key.clone()));
                    }
                }
            });
    });
    ui.label(
        egui::RichText::new(
            "Switching templates carries matching values; free-form citation text is replaced by the template rendering.",
        )
        .weak()
        .small(),
    );

    let template = state
        .draft
        .template_key
        .as_deref()
        .and_then(|key| state.templates.iter().find(|template| template.key == key))
        .cloned();

    match &template {
        Some(template) => {
            egui::Grid::new("source_template_fields")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    for field in template.source_fields() {
                        field_label(ui, field);
                        let value = state.draft.fields.entry(field.key.clone()).or_default();
                        let response = ui.add(egui::TextEdit::singleline(value));
                        if let Some(hint) = &field.hint {
                            response.on_hover_text(hint);
                        }
                        ui.end_row();
                    }
                });
            if template.citation_fields().next().is_some() {
                ui.separator();
                ui.label(egui::RichText::new("Citation detail (preview)").strong());
                egui::Grid::new("source_citation_fields")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        for field in template.citation_fields() {
                            field_label(ui, field);
                            let value = state.preview.entry(field.key.clone()).or_default();
                            ui.text_edit_singleline(value);
                            ui.end_row();
                        }
                    });
            }
        }
        None => {
            ui.label(egui::RichText::new("Source name *").strong());
            ui.text_edit_singleline(&mut state.draft.title);
            ui.label("Footnote:");
            ui.add(
                egui::TextEdit::multiline(&mut state.draft.footnote)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.label("Short footnote:");
            ui.add(
                egui::TextEdit::multiline(&mut state.draft.short_footnote)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.label("Bibliography:");
            ui.add(
                egui::TextEdit::multiline(&mut state.draft.bibliography)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                ui.label("Specific item (page):");
                ui.text_edit_singleline(state.preview.entry("page".to_string()).or_default());
            });

            ui.separator();
            ui.label(egui::RichText::new("GEDCOM fields").strong());
            egui::Grid::new("source_free_fields")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Author:");
                    ui.text_edit_singleline(&mut state.draft.author);
                    ui.end_row();

                    ui.label("Publication:");
                    ui.text_edit_singleline(&mut state.draft.publication);
                    ui.end_row();

                    ui.label("Abbreviation:");
                    ui.text_edit_singleline(&mut state.draft.abbreviation);
                    ui.end_row();

                    ui.label("Data date:");
                    ui.text_edit_singleline(&mut state.draft.data_date);
                    ui.end_row();

                    ui.label("Data text:");
                    ui.text_edit_singleline(&mut state.draft.data_text);
                    ui.end_row();
                });
            ui.label("Notes:");
            ui.add(
                egui::TextEdit::multiline(&mut state.draft.notes)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
        }
    }

    let rendered = match &template {
        Some(template) => render_citation(template, &state.draft.fields, &state.preview),
        None => render_free_form(
            &state.draft.footnote,
            &state.draft.short_footnote,
            &state.draft.bibliography,
            state.preview.get("page").map(String::as_str).unwrap_or(""),
        ),
    };
    ui.separator();
    ui.label(egui::RichText::new("Preview").strong());
    preview_line(ui, "Footnote", &rendered.footnote);
    preview_line(ui, "Short footnote", &rendered.short_footnote);
    preview_line(ui, "Bibliography", &rendered.bibliography);

    if let Some(error) = &state.error {
        ui.label(
            egui::RichText::new(error)
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }

    let valid = match &template {
        Some(template) => template
            .fields
            .iter()
            .filter(|field| field.scope == FieldScope::Source && field.required)
            .all(|field| {
                state
                    .draft
                    .fields
                    .get(&field.key)
                    .is_some_and(|value| !value.trim().is_empty())
            }),
        None => !state.draft.title.trim().is_empty(),
    };

    ui.separator();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                valid,
                egui::Button::new(egui::RichText::new("Save").strong()),
            )
            .clicked()
        {
            *event = Some(SourcesEvent::Save(Box::new(state.draft.clone())));
        }
        if state.selected.is_some() && state.list.len() > 1 && ui.button("Merge\u{2026}").clicked()
        {
            state.merge_open = true;
            state.merge_target = None;
        }
        if let Some(id) = state.selected.clone()
            && ui.button("Delete").clicked()
        {
            *event = Some(SourcesEvent::Delete(id));
        }
    });
}

fn merge_panel(ui: &mut egui::Ui, state: &mut SourcesState, event: &mut Option<SourcesEvent>) {
    if !state.merge_open {
        return;
    }
    let Some(source) = state
        .selected
        .as_deref()
        .and_then(|id| state.list.iter().find(|source| source.id == id).cloned())
    else {
        state.merge_open = false;
        state.merge_target = None;
        return;
    };
    let choices: Vec<SourceSummary> = state
        .list
        .iter()
        .filter(|candidate| candidate.id != source.id)
        .cloned()
        .collect();
    let target = state
        .merge_target
        .as_deref()
        .and_then(|id| choices.iter().find(|candidate| candidate.id == id).cloned())
        .or_else(|| choices.first().cloned());
    let Some(target) = target else {
        state.merge_open = false;
        state.merge_target = None;
        return;
    };
    state.merge_target = Some(target.id.clone());

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Merge source").strong());
            if ui.small_button("Cancel").clicked() {
                state.merge_open = false;
                state.merge_target = None;
            }
        });
        ui.label(format!("Merge \"{}\" into:", source.title));
        egui::ComboBox::from_id_salt("source_merge_target")
            .selected_text(&target.title)
            .show_ui(ui, |ui| {
                for candidate in &choices {
                    if ui
                        .selectable_label(candidate.id == target.id, &candidate.title)
                        .clicked()
                    {
                        state.merge_target = Some(candidate.id.clone());
                    }
                }
            });
        ui.label(
            egui::RichText::new(format!(
                "{}: {} citations",
                template_name(state, target.template_key.as_deref()),
                target.citation_count
            ))
            .weak()
            .small(),
        );
        ui.label(
            egui::RichText::new(format!(
                "{}: {} citations will move",
                template_name(state, source.template_key.as_deref()),
                source.citation_count
            ))
            .weak()
            .small(),
        );
        if source.template_key != target.template_key {
            ui.label(
                egui::RichText::new(
                    "The survivor's template and citation style will be kept.",
                )
                .color(ui.visuals().warn_fg_color)
                .small(),
            );
        }
        ui.label(
            egui::RichText::new(
                "Citations, repositories, fields and media/task links move; the absorbed source is deleted. Saved versions are merged.",
            )
            .weak()
            .small(),
        );
        if ui.button("Merge").clicked() {
            *event = Some(SourcesEvent::Merge {
                source_id: source.id.clone(),
                target_id: target.id.clone(),
            });
        }
    });
    ui.separator();
}

fn template_name(state: &SourcesState, key: Option<&str>) -> String {
    key.and_then(|key| state.templates.iter().find(|template| template.key == key))
        .map(|template| format!("{} - {}", template.category, template.name))
        .unwrap_or_else(|| "Free-form".to_string())
}

fn field_label(ui: &mut egui::Ui, field: &TemplateField) {
    if field.required {
        ui.label(egui::RichText::new(format!("{} *", field.label)).strong());
    } else {
        ui.label(&field.label);
    }
}

fn preview_line(ui: &mut egui::Ui, title: &str, text: &str) {
    ui.label(egui::RichText::new(title).weak().small());
    if text.is_empty() {
        ui.label(egui::RichText::new("(empty)").weak());
    } else {
        ui.label(text);
    }
}

/// Switches the draft to another template, carrying the values it can.
fn set_template(state: &mut SourcesState, key: Option<String>) {
    let current_key = state.draft.template_key.clone();
    if current_key == key {
        return;
    }
    let current = current_key
        .as_deref()
        .and_then(|key| state.templates.iter().find(|template| template.key == key))
        .cloned();
    let target = key
        .as_deref()
        .and_then(|key| state.templates.iter().find(|template| template.key == key))
        .cloned();
    if key.is_some() && target.is_none() {
        return;
    }

    if let Some(template) = target {
        let title = if current.is_some() {
            state
                .draft
                .fields
                .get("title")
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| state.draft.title.clone())
        } else {
            state.draft.title.clone()
        };
        let author = if current.is_some() {
            state
                .draft
                .fields
                .get("author")
                .or_else(|| state.draft.fields.get("creator"))
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| state.draft.author.clone())
        } else {
            state.draft.author.clone()
        };
        let allowed: BTreeSet<String> = template
            .source_fields()
            .map(|field| field.key.clone())
            .collect();
        state
            .draft
            .fields
            .retain(|field, _| allowed.contains(field));
        if template.source_fields().any(|field| field.key == "title") {
            seed_field(&mut state.draft.fields, "title", &title);
        }
        if template.source_fields().any(|field| field.key == "author") {
            seed_field(&mut state.draft.fields, "author", &author);
        }
        if template.source_fields().any(|field| field.key == "creator") {
            seed_field(&mut state.draft.fields, "creator", &author);
        }
        state.draft.template_key = Some(template.key.clone());
        return;
    }

    if let Some(template) = current {
        let rendered = render_citation(&template, &state.draft.fields, &BTreeMap::new());
        state.draft.footnote = rendered.footnote;
        state.draft.short_footnote = rendered.short_footnote;
        state.draft.bibliography = rendered.bibliography;
        let columns = gedcom_columns(&template, &state.draft.fields);
        replace_if_present(
            &mut state.draft.title,
            columns.get("title").map(String::as_str),
        );
        replace_if_present(
            &mut state.draft.author,
            columns.get("author").map(String::as_str),
        );
        replace_if_present(
            &mut state.draft.publication,
            columns.get("publication").map(String::as_str),
        );
        replace_if_present(
            &mut state.draft.abbreviation,
            columns.get("abbreviation").map(String::as_str),
        );
        replace_if_present(
            &mut state.draft.notes,
            columns.get("notes").map(String::as_str),
        );
        replace_if_present(
            &mut state.draft.data_date,
            columns.get("data_date").map(String::as_str),
        );
        replace_if_present(
            &mut state.draft.data_text,
            columns.get("data_text").map(String::as_str),
        );
    }
    state.draft.template_key = None;
}

fn seed_field(fields: &mut BTreeMap<String, String>, key: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    let slot = fields.entry(key.to_string()).or_default();
    if slot.trim().is_empty() {
        *slot = value.trim().to_string();
    }
}

fn replace_if_present(target: &mut String, source: Option<&str>) {
    if let Some(value) = source.filter(|value| !value.trim().is_empty()) {
        *target = value.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::source_template::builtin_templates;
    use egui::{Context, RawInput};

    #[test]
    fn switching_templates_keeps_matching_fields() {
        let mut state = SourcesState {
            templates: builtin_templates(),
            ..SourcesState::default()
        };
        state
            .draft
            .fields
            .insert("title".to_string(), "Family History".to_string());
        state
            .draft
            .fields
            .insert("page".to_string(), "42".to_string());

        set_template(&mut state, Some("book".to_string()));

        assert_eq!(state.draft.template_key.as_deref(), Some("book"));
        assert!(state.draft.fields.contains_key("title"));
        assert!(!state.draft.fields.contains_key("page"));

        set_template(&mut state, None);
        assert!(state.draft.template_key.is_none());
        assert!(
            state.draft.fields.contains_key("title"),
            "template values stay hidden so switching back does not lose them"
        );
        assert!(state.draft.footnote.contains("Family History"));
        assert_eq!(state.draft.title, "Family History");
    }

    #[test]
    fn unknown_template_keys_are_ignored() {
        let mut state = SourcesState {
            templates: builtin_templates(),
            ..SourcesState::default()
        };
        state.draft.template_key = Some("book".to_string());
        state
            .draft
            .fields
            .insert("title".to_string(), "Family History".to_string());

        set_template(&mut state, Some("missing".to_string()));

        assert_eq!(state.draft.template_key.as_deref(), Some("book"));
        assert!(state.draft.fields.contains_key("title"));
    }

    #[test]
    fn converting_free_form_to_a_template_seeds_values() {
        let mut state = SourcesState {
            templates: builtin_templates(),
            ..SourcesState::default()
        };
        state.draft.title = "Family History".to_string();
        state.draft.author = "Ada King".to_string();
        state.draft.footnote = "Old footnote.".to_string();

        set_template(&mut state, Some("book".to_string()));

        assert_eq!(state.draft.template_key.as_deref(), Some("book"));
        assert_eq!(
            state.draft.fields.get("title").map(String::as_str),
            Some("Family History")
        );
        assert_eq!(
            state.draft.fields.get("author").map(String::as_str),
            Some("Ada King")
        );
        assert_eq!(
            state.draft.footnote, "Old footnote.",
            "free-form text stays in the draft until the converted source is saved"
        );
    }

    #[test]
    fn converting_a_template_to_free_form_renders_its_text() {
        let mut state = SourcesState {
            templates: builtin_templates(),
            ..SourcesState::default()
        };
        state.draft.template_key = Some("book".to_string());
        for (key, value) in [
            ("title", "Family History"),
            ("author", "Ada King"),
            ("place", "London"),
            ("publisher", "Archive Press"),
            ("year", "1900"),
        ] {
            state
                .draft
                .fields
                .insert(key.to_string(), value.to_string());
        }

        set_template(&mut state, None);

        assert!(state.draft.template_key.is_none());
        assert!(state.draft.footnote.contains("Ada King"));
        assert!(state.draft.footnote.contains("Family History"));
        assert!(!state.draft.footnote.contains("{page}"));
        assert!(state.draft.short_footnote.contains("Family History"));
        assert!(state.draft.bibliography.contains("Archive Press"));
        assert_eq!(state.draft.title, "Family History");
        assert_eq!(state.draft.author, "Ada King");
        assert_eq!(
            state.draft.publication, "London: Archive Press, 1900",
            "cached GEDCOM columns become free-form fields"
        );
        assert!(
            state.draft.fields.contains_key("title"),
            "the template values stay available for switching back"
        );
    }

    #[test]
    fn converting_between_templates_keeps_matching_values() {
        let mut state = SourcesState {
            templates: builtin_templates(),
            ..SourcesState::default()
        };
        state.draft.template_key = Some("book".to_string());
        state.draft.author = "Ada King".to_string();
        for (key, value) in [
            ("title", "Family History"),
            ("author", "Ada King"),
            ("place", "London"),
        ] {
            state
                .draft
                .fields
                .insert(key.to_string(), value.to_string());
        }

        set_template(&mut state, Some("website".to_string()));

        assert_eq!(state.draft.template_key.as_deref(), Some("website"));
        assert_eq!(
            state.draft.fields.get("title").map(String::as_str),
            Some("Family History")
        );
        assert_eq!(
            state.draft.fields.get("creator").map(String::as_str),
            Some("Ada King"),
            "the author seeds the website creator"
        );
        assert!(!state.draft.fields.contains_key("author"));
        assert!(!state.draft.fields.contains_key("place"));
    }

    #[test]
    fn sources_tab_fills_the_available_height() {
        let ctx = Context::default();
        let mut state = SourcesState {
            templates: builtin_templates(),
            ..SourcesState::default()
        };
        set_template(&mut state, Some("book".to_string()));
        let mut fills = false;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let available = ui.available_height();
            show(ui, &mut state);
            fills = ui.min_rect().height() >= available - 1.0;
        });
        output.textures_delta.clear();
        assert!(fills, "the sources tab must fill the available height");
    }

    #[test]
    fn free_form_editor_renders_and_previews() {
        let ctx = Context::default();
        let mut state = SourcesState {
            templates: builtin_templates(),
            ..SourcesState::default()
        };
        state.draft.title = "Parish register".to_string();
        state.draft.footnote = "Parish register.".to_string();
        state.preview.insert("page".to_string(), "42".to_string());
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the free-form editor paints");
        assert!(state.draft.template_key.is_none());
    }

    #[test]
    fn merge_panel_targets_another_source() {
        let ctx = Context::default();
        let mut state = SourcesState {
            templates: builtin_templates(),
            list: vec![
                SourceSummary {
                    id: "target".to_string(),
                    title: "Target".to_string(),
                    template_key: Some("book".to_string()),
                    citation_count: 1,
                },
                SourceSummary {
                    id: "source".to_string(),
                    title: "Source".to_string(),
                    template_key: None,
                    citation_count: 2,
                },
            ],
            selected: Some("source".to_string()),
            merge_open: true,
            ..SourcesState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the merge panel paints");
        assert_eq!(state.merge_target.as_deref(), Some("target"));
        assert!(state.merge_open);
    }

    #[test]
    fn converted_sources_round_trip_through_the_database() {
        use crate::db;
        use crate::db::sources::{load_source, save_source};

        let mut conn = rusqlite::Connection::open_in_memory().expect("database opens");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma applies");
        db::init_db(&conn).expect("schema initialises");
        let id = save_source(
            &mut conn,
            &SourceDraft {
                title: "Family History".to_string(),
                author: "Ada King".to_string(),
                footnote: "Old footnote.".to_string(),
                ..SourceDraft::default()
            },
        )
        .expect("free-form source saves");
        let record = load_source(&conn, &id)
            .expect("load")
            .expect("source exists");
        let mut state = SourcesState {
            templates: builtin_templates(),
            draft: SourceDraft::from_record(&record),
            ..SourcesState::default()
        };

        set_template(&mut state, Some("book".to_string()));
        save_source(&mut conn, &state.draft).expect("templated source saves");
        let saved = load_source(&conn, &id)
            .expect("load")
            .expect("source exists");
        assert_eq!(saved.template_key.as_deref(), Some("book"));
        assert_eq!(
            saved.fields.get("title").map(String::as_str),
            Some("Family History")
        );
        assert_eq!(
            saved.fields.get("author").map(String::as_str),
            Some("Ada King")
        );
        assert_eq!(saved.footnote, None);

        state.draft = SourceDraft::from_record(&saved);
        set_template(&mut state, None);
        save_source(&mut conn, &state.draft).expect("free-form source saves");
        let free = load_source(&conn, &id)
            .expect("load")
            .expect("source exists");
        assert_eq!(free.template_key, None);
        assert!(
            free.footnote
                .as_deref()
                .is_some_and(|text| text.contains("Family History"))
        );
        assert!(
            free.fields.is_empty(),
            "free-form saves drop template fields"
        );
    }

    #[test]
    fn sources_tab_renders_headless() {
        let ctx = Context::default();
        let mut state = SourcesState {
            templates: builtin_templates(),
            ..SourcesState::default()
        };
        set_template(&mut state, Some("book".to_string()));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the sources tab paints");
        assert_eq!(state.draft.template_key.as_deref(), Some("book"));
    }
}
