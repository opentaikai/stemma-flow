//! Citations card: reusable citations, source attachment and quality.

use crate::db::citations::{CitationEdit, CitationRecord, CitationTarget, SourceCitation};
use crate::db::sources::SourceSummary;
use crate::i18n::{tr, tr_args, trn};
use crate::model::quality::{Certainty, EvidenceQuality, InformationQuality, SourceQuality};

/// One event a citation is attached to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationUse {
    pub event_id: String,
    pub event_type: String,
    pub event_date: Option<String>,
}

/// One canonical citation buffered for editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationDraft {
    pub id: String,
    pub source_title: String,
    pub uses: Vec<CitationUse>,
    pub usage: i64,
    pub page: String,
    pub quality_source: Option<SourceQuality>,
    pub quality_information: Option<InformationQuality>,
    pub quality_evidence: Option<EvidenceQuality>,
    pub quality_certainty: Option<Certainty>,
}

impl CitationDraft {
    /// Groups link records by citation so a shared citation is edited once.
    pub fn from_records(records: &[CitationRecord]) -> Vec<Self> {
        let mut drafts: Vec<Self> = Vec::new();
        for record in records {
            if let Some(draft) = drafts.iter_mut().find(|draft| draft.id == record.id) {
                if !draft
                    .uses
                    .iter()
                    .any(|use_| use_.event_id == record.event_id)
                {
                    draft.uses.push(CitationUse {
                        event_id: record.event_id.clone(),
                        event_type: record.event_type.clone(),
                        event_date: record.event_date.clone(),
                    });
                }
                draft.usage = record.usage;
            } else {
                drafts.push(Self::from_record(record));
            }
        }
        drafts
    }

    pub fn from_record(record: &CitationRecord) -> Self {
        Self {
            id: record.id.clone(),
            source_title: record.source_title.clone(),
            uses: vec![CitationUse {
                event_id: record.event_id.clone(),
                event_type: record.event_type.clone(),
                event_date: record.event_date.clone(),
            }],
            usage: record.usage,
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
    pub targets: Vec<CitationTarget>,
    pub add_target: String,
    pub add_source: Option<String>,
    pub add_page: String,
    pub choices: Vec<SourceCitation>,
    pub choices_source: Option<String>,
    pub pending_delete: Option<String>,
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
            targets: Vec::new(),
            add_target: "BIRTH".to_string(),
            add_source: None,
            add_page: String::new(),
            choices: Vec::new(),
            choices_source: None,
            pending_delete: None,
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
        target: String,
        page: Option<String>,
    },
    Attach {
        citation_id: String,
        target: String,
    },
    Detach {
        citation_id: String,
        event_id: String,
    },
    Delete(String),
    LoadChoices(String),
    Notes {
        citation_id: String,
        title: String,
    },
    Tasks {
        citation_id: String,
        title: String,
    },
}

/// Draws the citations card content.
pub fn show(ui: &mut egui::Ui, state: &mut CitationsState) -> Option<CitationsEvent> {
    let mut requested = None;
    let mut delete = None;
    let mut detach = None;
    let mut attach = None;
    let mut load_choices = None;
    let mut cancel_delete = false;
    let CitationsState {
        citations,
        sources,
        targets,
        add_target,
        add_source,
        add_page,
        choices,
        choices_source,
        pending_delete,
        error,
        dirty,
        ..
    } = state;

    if citations.is_empty() {
        ui.label(egui::RichText::new(tr("citations.empty")).weak());
    }
    for citation in citations.iter_mut() {
        ui.push_id(&citation.id, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&citation.source_title).strong());
                if ui.small_button(tr("citations.notes")).clicked() {
                    requested = Some(CitationsEvent::Notes {
                        citation_id: citation.id.clone(),
                        title: tr_args(
                            "citations.notes_title",
                            &[("title", citation.source_title.clone())],
                        ),
                    });
                }
                if ui.small_button(tr("citations.tasks")).clicked() {
                    requested = Some(CitationsEvent::Tasks {
                        citation_id: citation.id.clone(),
                        title: tr_args(
                            "citations.tasks_title",
                            &[("title", citation.source_title.clone())],
                        ),
                    });
                }
                ui.label(
                    egui::RichText::new(trn("citations.used_by", citation.usage))
                        .weak()
                        .small(),
                );
                if citation.usage > 1 {
                    if pending_delete.as_deref() == Some(citation.id.as_str()) {
                        if ui.small_button(tr("citations.confirm_delete")).clicked() {
                            delete = Some(citation.id.clone());
                        }
                        if ui.small_button(tr("common.cancel")).clicked() {
                            cancel_delete = true;
                        }
                    } else if ui
                        .small_button(tr_args(
                            "citations.delete_count",
                            &[("count", citation.usage.to_string())],
                        ))
                        .clicked()
                    {
                        *pending_delete = Some(citation.id.clone());
                    }
                } else if ui.small_button(tr("common.delete")).clicked() {
                    delete = Some(citation.id.clone());
                }
            });
            for use_ in &citation.uses {
                ui.push_id(("citation-use", &use_.event_id), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(use_label(&use_.event_type, use_.event_date.as_deref()));
                        if ui.small_button(tr("citations.detach")).clicked() {
                            detach = Some((citation.id.clone(), use_.event_id.clone()));
                        }
                    });
                });
            }
            egui::Grid::new("citation_quality_fields")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label(tr("citations.page"));
                    if ui.text_edit_singleline(&mut citation.page).changed() {
                        *dirty = true;
                    }
                    ui.end_row();

                    ui.label(tr("citations.source"));
                    if source_combo(ui, &mut citation.quality_source) {
                        *dirty = true;
                    }
                    ui.end_row();

                    ui.label(tr("citations.information"));
                    if information_combo(ui, &mut citation.quality_information) {
                        *dirty = true;
                    }
                    ui.end_row();

                    ui.label(tr("citations.evidence"));
                    if evidence_combo(ui, &mut citation.quality_evidence) {
                        *dirty = true;
                    }
                    ui.end_row();

                    ui.label(tr("citations.certainty"));
                    if certainty_combo(ui, &mut citation.quality_certainty) {
                        *dirty = true;
                    }
                    ui.end_row();
                });
            ui.separator();
        });
    }

    ui.label(egui::RichText::new(tr("citations.add")).strong());
    if sources.is_empty() {
        ui.label(
            egui::RichText::new(tr("citations.create_source_first"))
                .weak()
                .small(),
        );
    } else {
        if !targets
            .iter()
            .any(|target| target.key.as_str() == add_target.as_str())
        {
            *add_target = targets
                .first()
                .map(|target| target.key.clone())
                .unwrap_or_else(|| "BIRTH".to_string());
        }
        let selected_target = targets
            .iter()
            .find(|target| target.key.as_str() == add_target.as_str())
            .map(|target| target.label.clone())
            .unwrap_or_else(|| event_label(add_target).to_string());
        let current_source = add_source
            .as_deref()
            .and_then(|id| sources.iter().find(|source| source.id == id))
            .map(|source| source.title.clone())
            .unwrap_or_else(|| tr("citations.select_source").to_string());
        ui.horizontal(|ui| {
            ui.label(tr("citations.event"));
            egui::ComboBox::from_id_salt("citation_add_target")
                .selected_text(selected_target)
                .show_ui(ui, |ui| {
                    for target in targets.iter() {
                        if ui
                            .selectable_label(
                                add_target.as_str() == target.key.as_str(),
                                &target.label,
                            )
                            .clicked()
                        {
                            *add_target = target.key.clone();
                        }
                    }
                });
            ui.label(tr("citations.source"));
            egui::ComboBox::from_id_salt("citation_add_source")
                .selected_text(current_source)
                .show_ui(ui, |ui| {
                    for source in sources.iter() {
                        if ui
                            .selectable_label(
                                add_source.as_deref() == Some(source.id.as_str()),
                                &source.title,
                            )
                            .clicked()
                        {
                            *add_source = Some(source.id.clone());
                            choices.clear();
                            *choices_source = None;
                            load_choices = Some(source.id.clone());
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(tr("citations.page"));
            ui.text_edit_singleline(add_page);
            if ui
                .add_enabled(
                    add_source.is_some(),
                    egui::Button::new(tr("citations.add_new")),
                )
                .clicked()
                && let Some(source_id) = add_source.clone()
            {
                requested = Some(CitationsEvent::Add {
                    source_id,
                    target: add_target.clone(),
                    page: clean(add_page),
                });
            }
        });
        if let Some(source_id) = add_source.as_deref() {
            if choices_source.as_deref() != Some(source_id) {
                ui.label(egui::RichText::new(tr("citations.loading")).weak().small());
            } else if choices.is_empty() {
                ui.label(
                    egui::RichText::new(tr("citations.none_for_source"))
                        .weak()
                        .small(),
                );
            } else {
                ui.label(
                    egui::RichText::new(tr("citations.reuse_existing"))
                        .weak()
                        .small(),
                );
                for choice in choices.iter() {
                    ui.push_id(("citation-choice", &choice.id), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let page = choice
                                .page
                                .as_deref()
                                .map(str::trim)
                                .filter(|page| !page.is_empty())
                                .unwrap_or("(no page)");
                            ui.label(page);
                            ui.label(egui::RichText::new(quality_summary(choice)).weak().small());
                            ui.label(
                                egui::RichText::new(trn("citations.used_by", choice.usage))
                                    .weak()
                                    .small(),
                            );
                            if ui.small_button(tr("citations.attach")).clicked() {
                                attach = Some((choice.id.clone(), add_target.clone()));
                            }
                        });
                    });
                }
            }
        }
    }

    if let Some(error) = error {
        ui.label(
            egui::RichText::new(error.as_str())
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
    if requested.is_none() && *dirty && ui.button(tr("citations.save")).clicked() {
        requested = Some(CitationsEvent::Save(
            citations.iter().map(CitationDraft::to_edit).collect(),
        ));
    }
    let deleted = delete.is_some();
    if let Some(id) = delete {
        *pending_delete = None;
        requested = Some(CitationsEvent::Delete(id));
    } else if let Some((citation_id, event_id)) = detach {
        requested = Some(CitationsEvent::Detach {
            citation_id,
            event_id,
        });
    } else if let Some((citation_id, target)) = attach {
        requested = Some(CitationsEvent::Attach {
            citation_id,
            target,
        });
    } else if let Some(source_id) = load_choices
        && requested.is_none()
    {
        requested = Some(CitationsEvent::LoadChoices(source_id));
    }
    if cancel_delete && !deleted {
        *pending_delete = None;
    }
    requested
}

fn event_label(event_type: &str) -> String {
    match event_type {
        "DEATH" => tr("citations.death").to_string(),
        "MARRIAGE" => tr("citations.marriage").to_string(),
        "BIRTH" => tr("citations.birth").to_string(),
        other => other.to_string(),
    }
}

fn use_label(event_type: &str, date: Option<&str>) -> String {
    let label = event_label(event_type);
    match date.map(str::trim).filter(|date| !date.is_empty()) {
        Some(date) => format!("{label} ({date})"),
        None => label,
    }
}

fn quality_source_label(value: SourceQuality) -> &'static str {
    tr(match value {
        SourceQuality::Original => "citations.quality.original",
        SourceQuality::Derivative => "citations.quality.derivative",
        SourceQuality::Authored => "citations.quality.authored",
    })
}

fn quality_information_label(value: InformationQuality) -> &'static str {
    tr(match value {
        InformationQuality::Primary => "citations.information_quality.primary",
        InformationQuality::Secondary => "citations.information_quality.secondary",
        InformationQuality::Undetermined => "citations.information_quality.undetermined",
    })
}

fn quality_evidence_label(value: EvidenceQuality) -> &'static str {
    tr(match value {
        EvidenceQuality::Direct => "citations.evidence_quality.direct",
        EvidenceQuality::Indirect => "citations.evidence_quality.indirect",
        EvidenceQuality::Negative => "citations.evidence_quality.negative",
    })
}

fn certainty_label(value: Certainty) -> &'static str {
    tr(match value {
        Certainty::Unreliable => "citations.certainty_level_unreliable",
        Certainty::Questionable => "citations.certainty_level_questionable",
        Certainty::Secondary => "citations.certainty_level_secondary",
        Certainty::Primary => "citations.certainty_level_primary",
    })
}

fn certainty_description(value: Certainty) -> &'static str {
    tr(match value {
        Certainty::Unreliable => "citations.certainty_hint_unreliable",
        Certainty::Questionable => "citations.certainty_hint_questionable",
        Certainty::Secondary => "citations.certainty_hint_secondary",
        Certainty::Primary => "citations.certainty_hint_primary",
    })
}

fn quality_summary(citation: &SourceCitation) -> String {
    let mut parts: Vec<String> = [
        citation.quality_source.map(quality_source_label),
        citation.quality_information.map(quality_information_label),
        citation.quality_evidence.map(quality_evidence_label),
    ]
    .into_iter()
    .flatten()
    .map(str::to_string)
    .collect();
    if let Some(certainty) = citation.quality_certainty {
        parts.push(tr_args(
            "citations.certainty_prefix",
            &[("certainty", certainty_label(certainty).to_string())],
        ));
    }
    if parts.is_empty() {
        tr("citations.no_quality").to_string()
    } else {
        parts.join(" · ")
    }
}

fn source_combo(ui: &mut egui::Ui, value: &mut Option<SourceQuality>) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt("citation_quality_source")
        .selected_text(
            value
                .map(quality_source_label)
                .unwrap_or_else(|| tr("citations.dont_know")),
        )
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(value.is_none(), tr("citations.dont_know"))
                .clicked()
            {
                *value = None;
                changed = true;
            }
            for option in SourceQuality::ALL {
                if ui
                    .selectable_label(*value == Some(option), quality_source_label(option))
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
        .selected_text(
            value
                .map(quality_information_label)
                .unwrap_or_else(|| tr("citations.dont_know")),
        )
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(value.is_none(), tr("citations.dont_know"))
                .clicked()
            {
                *value = None;
                changed = true;
            }
            for option in InformationQuality::ALL {
                if ui
                    .selectable_label(*value == Some(option), quality_information_label(option))
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
        .selected_text(
            value
                .map(quality_evidence_label)
                .unwrap_or_else(|| tr("citations.dont_know")),
        )
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(value.is_none(), tr("citations.dont_know"))
                .clicked()
            {
                *value = None;
                changed = true;
            }
            for option in EvidenceQuality::ALL {
                if ui
                    .selectable_label(*value == Some(option), quality_evidence_label(option))
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
        .map(|certainty| format!("{} ({})", certainty_label(certainty), certainty.value()))
        .unwrap_or_else(|| tr("citations.dont_know").to_string());
    egui::ComboBox::from_id_salt("citation_quality_certainty")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(value.is_none(), tr("citations.dont_know"))
                .clicked()
            {
                *value = None;
                changed = true;
            }
            for option in Certainty::ALL {
                let response = ui
                    .selectable_label(
                        *value == Some(option),
                        format!("{} ({})", certainty_label(option), option.value()),
                    )
                    .on_hover_text(certainty_description(option));
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
    fn groups_shared_records_into_one_draft() {
        let mut death = record();
        death.event_id = "e2".to_string();
        death.event_type = "DEATH".to_string();
        death.usage = 2;
        let mut birth = record();
        birth.usage = 2;
        let drafts = CitationDraft::from_records(&[birth, death]);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].usage, 2);
        assert_eq!(drafts[0].uses.len(), 2);
        assert_eq!(drafts[0].uses[1].event_type, "DEATH");
    }

    #[test]
    fn blank_pages_clear() {
        let mut draft = CitationDraft::from_record(&record());
        draft.page = "   ".to_string();
        assert_eq!(draft.to_edit().page, None);
    }

    #[test]
    fn labels_cover_family_events() {
        assert_eq!(use_label("MARRIAGE", Some("1900")), "Marriage (1900)");
        assert_eq!(use_label("DEATH", None), "Death");
    }

    #[test]
    fn quality_summary_lists_known_ratings() {
        let choice = SourceCitation {
            id: "c1".to_string(),
            page: Some("12".to_string()),
            quality_source: Some(SourceQuality::Original),
            quality_information: None,
            quality_evidence: None,
            quality_certainty: Some(Certainty::Primary),
            usage: 1,
        };
        assert_eq!(quality_summary(&choice), "Original · certainty: Primary");
    }

    #[test]
    fn citations_card_renders_headless() {
        let ctx = Context::default();
        let mut shared = record();
        let mut death = record();
        death.event_id = "e2".to_string();
        death.event_type = "DEATH".to_string();
        shared.usage = 2;
        let mut state = CitationsState {
            citations: CitationDraft::from_records(&[shared, death]),
            sources: vec![SourceSummary {
                id: "s1".to_string(),
                title: "Parish Book".to_string(),
                template_key: None,
                citation_count: 1,
            }],
            targets: vec![
                CitationTarget {
                    key: "BIRTH".to_string(),
                    label: "Birth".to_string(),
                    event_type: "BIRTH".to_string(),
                    event_id: Some("e1".to_string()),
                },
                CitationTarget {
                    key: "MARRIAGE:f1".to_string(),
                    label: "Marriage to Otto King".to_string(),
                    event_type: "MARRIAGE".to_string(),
                    event_id: None,
                },
            ],
            add_source: Some("s1".to_string()),
            choices: vec![SourceCitation {
                id: "c1".to_string(),
                page: Some("12".to_string()),
                quality_source: Some(SourceQuality::Original),
                quality_information: Some(InformationQuality::Primary),
                quality_evidence: Some(EvidenceQuality::Direct),
                quality_certainty: Some(Certainty::Primary),
                usage: 2,
            }],
            choices_source: Some("s1".to_string()),
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
