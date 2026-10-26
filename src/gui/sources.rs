//! Sources tab: template-driven and free-form source editing.

use std::collections::{BTreeMap, BTreeSet};

use crate::db::sources::{SourceDraft, SourceSummary};
use crate::model::source_template::{
    FieldScope, SourceTemplate, TemplateField, render_citation, render_free_form,
};

/// Action requested by the sources tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcesEvent {
    New,
    Select(String),
    Save(Box<SourceDraft>),
    Delete(String),
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
        if let Some(id) = state.selected.clone()
            && ui.button("Delete").clicked()
        {
            *event = Some(SourcesEvent::Delete(id));
        }
    });
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

/// Switches the draft to another template, keeping fields that still exist.
fn set_template(state: &mut SourcesState, key: Option<String>) {
    let Some(key) = key else {
        state.draft.template_key = None;
        state.draft.fields.clear();
        return;
    };
    let Some(template) = state
        .templates
        .iter()
        .find(|template| template.key == key)
        .cloned()
    else {
        return;
    };
    let allowed: BTreeSet<String> = template
        .source_fields()
        .map(|field| field.key.clone())
        .collect();
    state
        .draft
        .fields
        .retain(|field, _| allowed.contains(field));
    state.draft.template_key = Some(key);
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
        assert!(state.draft.fields.is_empty());
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
