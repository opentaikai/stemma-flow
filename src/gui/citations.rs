//! Citations card: source attachment and Evidence Explained quality.

use crate::db::citations::{CitationEdit, CitationRecord};
use crate::db::sources::SourceSummary;
use crate::model::quality::{Certainty, EvidenceQuality, InformationQuality, SourceQuality};

/// One citation buffered for editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationDraft {
    pub id: String,
    pub source_title: String,
    pub event_type: String,
    pub event_date: Option<String>,
    pub page: String,
    pub quality_source: Option<SourceQuality>,
    pub quality_information: Option<InformationQuality>,
    pub quality_evidence: Option<EvidenceQuality>,
    pub quality_certainty: Option<Certainty>,
}

impl CitationDraft {
    pub fn from_record(record: &CitationRecord) -> Self {
        Self {
            id: record.id.clone(),
            source_title: record.source_title.clone(),
            event_type: record.event_type.clone(),
            event_date: record.event_date.clone(),
            page: record.page.clone().unwrap_or_default(),
            quality_source: record.quality_source,
            quality_information: record.quality_information,
            quality_evidence: record.quality_evidence,
            quality_certainty: record.quality_certainty,
        }
    }

    pub fn to_edit(&self) -> CitationEdit {
        CitationEdit {
            id: self.id.clone(),
            page: clean(&self.page),
            quality_source: self.quality_source,
            quality_information: self.quality_information,
            quality_evidence: self.quality_evidence,
            quality_certainty: self.quality_certainty,
        }
    }
}

fn clean(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// State owned by the window while the inspector shows citations.
#[derive(Debug)]
pub struct CitationsState {
    pub person_id: Option<String>,
    pub citations: Vec<CitationDraft>,
    pub sources: Vec<SourceSummary>,
    pub add_event: String,
    pub add_source: Option<String>,
    pub add_page: String,
    pub error: Option<String>,
    pub dirty: bool,
    pub loaded: bool,
}

impl Default for CitationsState {
    fn default() -> Self {
        Self {
            person_id: None,
            citations: Vec::new(),
            sources: Vec::new(),
            add_event: "BIRTH".to_string(),
            add_source: None,
            add_page: String::new(),
            error: None,
            dirty: false,
            loaded: false,
        }
    }
}

/// Action requested by the citations card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CitationsEvent {
    Save(Vec<CitationEdit>),
    Add {
        source_id: String,
        event_type: String,
        page: Option<String>,
    },
    Delete(String),
}

/// Draws the citations card content.
pub fn show(ui: &mut egui::Ui, state: &mut CitationsState) -> Option<CitationsEvent> {
    let mut event = None;
    let CitationsState {
        citations,
        sources,
        add_event,
        add_source,
        add_page,
        error,
        dirty,
        ..
    } = state;

    if citations.is_empty() {
        ui.label(egui::RichText::new("No citations yet.").weak());
    }
    let mut delete = None;
    for (event_type, title) in [("BIRTH", "Birth"), ("DEATH", "Death")] {
        let indices: Vec<usize> = citations
            .iter()
            .enumerate()
            .filter(|(_, citation)| citation.event_type == event_type)
            .map(|(index, _)| index)
            .collect();
        if indices.is_empty() {
            continue;
        }
        ui.label(egui::RichText::new(title).strong());
        for index in indices {
            let Some(citation) = citations.get_mut(index) else {
                continue;
            };
            ui.push_id(&citation.id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&citation.source_title).strong());
                    if let Some(date) = &citation.event_date {
                        ui.label(egui::RichText::new(date).weak().small());
                    }
                    if ui.small_button("Delete").clicked() {
                        delete = Some(citation.id.clone());
                    }
                });
                egui::Grid::new("citation_quality_fields")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Page:");
                        if ui.text_edit_singleline(&mut citation.page).changed() {
                            *dirty = true;
                        }
                        ui.end_row();

                        ui.label("Source:");
                        if source_combo(ui, &mut citation.quality_source) {
                            *dirty = true;
                        }
                        ui.end_row();

                        ui.label("Information:");
                        if information_combo(ui, &mut citation.quality_information) {
                            *dirty = true;
                        }
                        ui.end_row();

                        ui.label("Evidence:");
                        if evidence_combo(ui, &mut citation.quality_evidence) {
                            *dirty = true;
                        }
                        ui.end_row();

                        ui.label("Certainty:");
                        if certainty_combo(ui, &mut citation.quality_certainty) {
                            *dirty = true;
                        }
                        ui.end_row();
                    });
                ui.separator();
            });
        }
    }

    ui.separator();
    ui.label(egui::RichText::new("Add citation").strong());
    if sources.is_empty() {
        ui.label(
            egui::RichText::new("Create a source in the Sources tab first.")
                .weak()
                .small(),
        );
    } else {
        let current_source = add_source
            .as_deref()
            .and_then(|id| sources.iter().find(|source| source.id == id))
            .map(|source| source.title.clone())
            .unwrap_or_else(|| "Select a source".to_string());
        ui.horizontal(|ui| {
            ui.label("Event:");
            egui::ComboBox::from_id_salt("citation_add_event")
                .selected_text(event_label(add_event))
                .show_ui(ui, |ui| {
                    for value in ["BIRTH", "DEATH"] {
                        if ui
                            .selectable_label(add_event == value, event_label(value))
                            .clicked()
                        {
                            *add_event = value.to_string();
                        }
                    }
                });
            ui.label("Source:");
            egui::ComboBox::from_id_salt("citation_add_source")
                .selected_text(current_source)
                .show_ui(ui, |ui| {
                    for source in sources {
                        if ui
                            .selectable_label(
                                add_source.as_deref() == Some(source.id.as_str()),
                                &source.title,
                            )
                            .clicked()
                        {
                            *add_source = Some(source.id.clone());
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Page:");
            ui.text_edit_singleline(add_page);
            if ui
                .add_enabled(add_source.is_some(), egui::Button::new("Add"))
                .clicked()
                && let Some(source_id) = add_source.clone()
            {
                event = Some(CitationsEvent::Add {
                    source_id,
                    event_type: add_event.clone(),
                    page: clean(add_page),
                });
            }
        });
    }

    if let Some(error) = error {
        ui.label(
            egui::RichText::new(error.as_str())
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
    if *dirty && ui.button("Save citations").clicked() {
        event = Some(CitationsEvent::Save(
            citations.iter().map(CitationDraft::to_edit).collect(),
        ));
    }
    if let Some(id) = delete {
        event = Some(CitationsEvent::Delete(id));
    }
    event
}

fn event_label(event_type: &str) -> &str {
    if event_type == "DEATH" {
        "Death"
    } else {
        "Birth"
    }
}

fn source_combo(ui: &mut egui::Ui, value: &mut Option<SourceQuality>) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt("citation_quality_source")
        .selected_text(value.map(SourceQuality::label).unwrap_or("Don't know"))
        .show_ui(ui, |ui| {
            if ui.selectable_label(value.is_none(), "Don't know").clicked() {
                *value = None;
                changed = true;
            }
            for option in SourceQuality::ALL {
                if ui
                    .selectable_label(*value == Some(option), option.label())
                    .clicked()
                {
                    *value = Some(option);
                    changed = true;
                }
            }
        });
    changed
}

fn information_combo(ui: &mut egui::Ui, value: &mut Option<InformationQuality>) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt("citation_quality_information")
        .selected_text(value.map(InformationQuality::label).unwrap_or("Don't know"))
        .show_ui(ui, |ui| {
            if ui.selectable_label(value.is_none(), "Don't know").clicked() {
                *value = None;
                changed = true;
            }
            for option in InformationQuality::ALL {
                if ui
                    .selectable_label(*value == Some(option), option.label())
                    .clicked()
                {
                    *value = Some(option);
                    changed = true;
                }
            }
        });
    changed
}

fn evidence_combo(ui: &mut egui::Ui, value: &mut Option<EvidenceQuality>) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt("citation_quality_evidence")
        .selected_text(value.map(EvidenceQuality::label).unwrap_or("Don't know"))
        .show_ui(ui, |ui| {
            if ui.selectable_label(value.is_none(), "Don't know").clicked() {
                *value = None;
                changed = true;
            }
            for option in EvidenceQuality::ALL {
                if ui
                    .selectable_label(*value == Some(option), option.label())
                    .clicked()
                {
                    *value = Some(option);
                    changed = true;
                }
            }
        });
    changed
}

fn certainty_combo(ui: &mut egui::Ui, value: &mut Option<Certainty>) -> bool {
    let mut changed = false;
    let selected = value
        .map(|certainty| format!("{} ({})", certainty.label(), certainty.value()))
        .unwrap_or_else(|| "Don't know".to_string());
    egui::ComboBox::from_id_salt("citation_quality_certainty")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            if ui.selectable_label(value.is_none(), "Don't know").clicked() {
                *value = None;
                changed = true;
            }
            for option in Certainty::ALL {
                let response = ui
                    .selectable_label(
                        *value == Some(option),
                        format!("{} ({})", option.label(), option.value()),
                    )
                    .on_hover_text(option.description());
                if response.clicked() {
                    *value = Some(option);
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

    fn record() -> CitationRecord {
        CitationRecord {
            id: "c1".to_string(),
            source_id: "s1".to_string(),
            source_title: "Parish Book".to_string(),
            event_id: "e1".to_string(),
            event_type: "BIRTH".to_string(),
            event_date: Some("1850".to_string()),
            page: Some("12".to_string()),
            quality_source: Some(SourceQuality::Original),
            quality_information: Some(InformationQuality::Primary),
            quality_evidence: Some(EvidenceQuality::Direct),
            quality_certainty: Some(Certainty::Primary),
            usage: 1,
        }
    }

    #[test]
    fn draft_round_trips_through_the_edit() {
        let draft = CitationDraft::from_record(&record());
        assert_eq!(draft.page, "12");
        assert_eq!(draft.quality_source, Some(SourceQuality::Original));
        let edit = draft.to_edit();
        assert_eq!(edit.id, "c1");
        assert_eq!(edit.page.as_deref(), Some("12"));
        assert_eq!(edit.quality_information, Some(InformationQuality::Primary));
        assert_eq!(edit.quality_certainty, Some(Certainty::Primary));
    }

    #[test]
    fn blank_pages_clear() {
        let mut draft = CitationDraft::from_record(&record());
        draft.page = "   ".to_string();
        assert_eq!(draft.to_edit().page, None);
    }

    #[test]
    fn citations_card_renders_headless() {
        let ctx = Context::default();
        let mut state = CitationsState {
            citations: vec![CitationDraft::from_record(&record())],
            sources: vec![SourceSummary {
                id: "s1".to_string(),
                title: "Parish Book".to_string(),
                template_key: None,
                citation_count: 1,
            }],
            ..CitationsState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the citations card paints");
        assert!(!state.dirty);
    }
}
