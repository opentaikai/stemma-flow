//! Rendering the inspector panel: framed cards and the profile editors.

use crate::gui::citations::{self, CitationsEvent, CitationsState};
use crate::gui::names::{self, NamesEvent, NamesState};

use egui::{RichText, Ui};

use crate::db::Person;
use crate::db::associations::AssociationRecord;
use crate::db::facts::PersonFact;
use crate::db::groups::GroupSummary;
use crate::db::labels::LabelSummary;
use crate::db::media::MediaOwner;
use crate::db::notes::NoteOwner;
use crate::db::people::{ParentRole, VitalDates};
use crate::db::privacy::PersonPrivacy;
use crate::db::tasks::TaskOwner;
use crate::gui::date_picker::date_edit;
use crate::i18n::{fact_label, tr, tr_args};
use crate::model::age;

use super::ACCENT;
use super::InspectorAction;
use super::RelationKind;
use super::connections::{Connections, person_label};
use super::form::InspectorForm;

/// Renders the profile editors shared by the panel and relation pop-up.
pub fn field_editors(ui: &mut Ui, form: &mut InspectorForm) {
    name_editors(ui, form);
    vital_editors(ui, form);
}

/// Name and gender fields, used by every form that edits a person.
pub fn name_editors(ui: &mut Ui, form: &mut InspectorForm) {
    let has_name = form.has_name();
    egui::Grid::new("person_name_fields")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label(tr("inspector.prefix"));
            ui.text_edit_singleline(&mut form.name_prefix);
            ui.end_row();

            ui.label(tr("inspector.given_name"));
            ui.text_edit_singleline(&mut form.given_name);
            ui.end_row();

            ui.label(tr("inspector.middle_name"));
            ui.text_edit_singleline(&mut form.middle_name);
            ui.end_row();

            ui.label(tr("inspector.surname"));
            ui.text_edit_singleline(&mut form.surname);
            ui.end_row();

            ui.label(tr("inspector.suffix"));
            ui.text_edit_singleline(&mut form.name_suffix);
            ui.end_row();
            if !has_name {
                ui.label("");
                ui.label(
                    RichText::new(tr("inspector.name_required"))
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
                ui.end_row();
            }

            ui.label(tr("inspector.nickname"));
            ui.text_edit_singleline(&mut form.nickname);
            ui.end_row();

            ui.label(tr("inspector.gender"));
            ui.horizontal(|ui| {
                ui.selectable_value(&mut form.gender, "M".to_string(), tr("inspector.male"));
                ui.selectable_value(&mut form.gender, "F".to_string(), tr("inspector.female"));
                ui.selectable_value(&mut form.gender, "U".to_string(), tr("inspector.unknown"));
            });
            ui.end_row();
        });
}

/// Birth and death dates with their places.
pub fn vital_editors(ui: &mut Ui, form: &mut InspectorForm) {
    let errors = form.date_errors();
    egui::Grid::new("person_vital_fields")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label(tr("inspector.birth_date"));
            date_edit(ui, &mut form.birth, "birth_date");
            ui.end_row();
            if let Some(error) = &errors.birth {
                ui.label("");
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
                ui.end_row();
            }

            ui.label(tr("inspector.birth_place"));
            ui.text_edit_singleline(&mut form.birth_place);
            ui.end_row();

            ui.label(tr("inspector.death_date"));
            date_edit(ui, &mut form.death, "death_date");
            ui.end_row();
            if let Some(error) = &errors.death {
                ui.label("");
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
                ui.end_row();
            }

            ui.label(tr("inspector.death_place"));
            ui.text_edit_singleline(&mut form.death_place);
            ui.end_row();
        });
    if let Some(derived) = age::age_at(Some(&form.birth), Some(&form.death)) {
        ui.label(
            RichText::new(tr_args(
                "inspector.age_at_death",
                &[("age", derived.to_string())],
            ))
            .weak()
            .small(),
        );
    }
}

/// Draws the inspector body and returns the action to apply, if any.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    person: Option<&Person>,
    form: &mut InspectorForm,
    dates: Option<&VitalDates>,
    connections: Connections,
    associations: &[AssociationRecord],
    groups: &[GroupSummary],
    labels: &[LabelSummary],
    bookmarked: bool,
    citations: &mut CitationsState,
    facts: &[PersonFact],
    privacy: Option<&PersonPrivacy>,
    names: &mut NamesState,
    root_relationship: Option<&str>,
    is_root: bool,
    surname_first: bool,
    editor_mode: bool,
) -> Option<InspectorAction> {
    let Some(person) = person else {
        ui.label(RichText::new(tr("inspector.not_found")).weak());
        return None;
    };

    let mut action = None;
    if !editor_mode {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // U+00D7 is in Ubuntu-Light; U+2715 (✕) is missing from the
                // bundled fonts and would render as a replacement box.
                if ui
                    .small_button("×")
                    .on_hover_text(tr("inspector.close_hint"))
                    .clicked()
                {
                    action = Some(InspectorAction::Close);
                }
            });
        });
    }
    let footer_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y * 5.0;
    let body_height = (ui.available_height() - footer_height).max(120.0);
    egui::ScrollArea::vertical()
        .id_salt("person_inspector_body")
        .auto_shrink([false, false])
        .max_height(body_height)
        .show(ui, |ui| {
            card(ui, &person_label(person, surname_first), |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(gender_word(person.gender.as_str()))
                            .weak()
                            .size(12.0),
                    );
                    let mark = if bookmarked {
                        tr("bookmarks.marked")
                    } else {
                        tr("bookmarks.mark")
                    };
                    let tooltip = if bookmarked {
                        tr("bookmarks.remove_current")
                    } else {
                        tr("bookmarks.add_current")
                    };
                    if ui.small_button(mark).on_hover_text(tooltip).clicked() {
                        action = Some(InspectorAction::ToggleBookmark);
                    }
                });
                if is_root {
                    ui.label(RichText::new(tr("root.badge")).strong().small());
                } else if let Some(relationship) = root_relationship {
                    ui.label(
                        RichText::new(crate::i18n::tr_args(
                            "root.relationship",
                            &[("relationship", relationship.to_string())],
                        ))
                        .weak()
                        .small(),
                    );
                }
                if is_root {
                    if ui.small_button(tr("root.clear")).clicked() {
                        action = Some(InspectorAction::ClearRoot);
                    }
                } else if ui.small_button(tr("root.set")).clicked() {
                    action = Some(InspectorAction::SetRoot);
                }
            });
            card(ui, &tr("inspector.card_profile"), |ui| {
                name_editors(ui, form)
            });
            card(ui, &tr("inspector.card_names"), |ui| {
                if let Some(event) = names::show(ui, names, surname_first) {
                    action = Some(match event {
                        NamesEvent::Save(draft) => InspectorAction::SaveAlternateName(draft),
                        NamesEvent::Delete(id) => InspectorAction::DeleteAlternateName(id),
                    });
                }
            });
            card(ui, &tr("inspector.card_vital"), |ui| {
                vital_editors(ui, form);
                if let Some(dates) = dates {
                    for (label, event_id) in [
                        (tr("inspector.birth"), dates.birth_event_id.as_deref()),
                        (tr("inspector.death"), dates.death_event_id.as_deref()),
                    ] {
                        if let Some(event_id) = event_id
                            && ui
                                .small_button(tr_args(
                                    "inspector.event_notes_button",
                                    &[("label", label.clone())],
                                ))
                                .clicked()
                        {
                            action = Some(InspectorAction::OpenNotes {
                                owner: NoteOwner::Event(event_id.to_string()),
                                title: tr_args(
                                    "inspector.notes_title",
                                    &[("label", label.to_string())],
                                ),
                            });
                        }
                    }
                }
            });
            if !connections.families.is_empty() {
                card(ui, &tr("inspector.card_families"), |ui| {
                    for family in &connections.families {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&family.label).strong());
                            if ui.small_button(tr("inspector.notes_button")).clicked() {
                                action = Some(InspectorAction::OpenNotes {
                                    owner: NoteOwner::Family(family.id.clone()),
                                    title: tr_args(
                                        "inspector.notes_title",
                                        &[("label", family.label.clone())],
                                    ),
                                });
                            }
                        });
                    }
                });
            }
            card(ui, &tr("inspector.card_associations"), |ui| {
                if associations.is_empty() {
                    ui.label(RichText::new(tr("inspector.none")).weak().size(12.0));
                }
                for association in associations {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&association.relationship).strong());
                        if ui
                            .link(&association.associate_name)
                            .on_hover_text(tr("inspector.focus_person"))
                            .clicked()
                        {
                            action = Some(InspectorAction::Focus(association.associate_id.clone()));
                        }
                        if ui.small_button(tr("common.edit")).clicked() {
                            action = Some(InspectorAction::EditAssociation(association.id.clone()));
                        }
                        if ui.small_button(tr("common.delete")).clicked() {
                            action =
                                Some(InspectorAction::DeleteAssociation(association.id.clone()));
                        }
                    });
                }
                if ui.button(tr("inspector.add_association")).clicked() {
                    action = Some(InspectorAction::AddAssociation);
                }
            });
            if !groups.is_empty() {
                card(ui, &tr("editor.groups"), |ui| {
                    for group in groups {
                        ui.horizontal(|ui| {
                            if let Some(color) = group.color {
                                crate::gui::symbols::color_swatch(
                                    ui,
                                    crate::gui::labels::color32(color),
                                );
                            }
                            ui.label(RichText::new(&group.name).strong());
                        });
                    }
                    if ui.button(tr("editor.manage_groups")).clicked() {
                        action = Some(InspectorAction::OpenGroups);
                    }
                });
            }
            if !labels.is_empty() {
                card(ui, &tr("editor.labels"), |ui| {
                    for label in labels {
                        ui.horizontal(|ui| {
                            crate::gui::symbols::color_swatch(
                                ui,
                                crate::gui::labels::color32(label.color),
                            );
                            ui.label(RichText::new(&label.name).strong());
                        });
                    }
                    if ui.button(tr("editor.manage_labels")).clicked() {
                        action = Some(InspectorAction::OpenLabels);
                    }
                });
            }
            card(ui, &tr("inspector.card_other_facts"), |ui| {
                if facts.is_empty() {
                    ui.label(RichText::new(tr("inspector.none")).weak().size(12.0));
                }
                for fact in facts {
                    ui.push_id(&fact.event_id, |ui| {
                        let fact_label = fact_label(&fact.event_type, &fact.label);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(fact_label.clone()).strong());
                            if let Some(role) = &fact.role {
                                ui.label(
                                    RichText::new(tr_args(
                                        "inspector.role_for",
                                        &[("role", role.to_string())],
                                    ))
                                    .weak()
                                    .small(),
                                );
                                if let Some(principal) = &fact.principal {
                                    if let Some(principal_id) = &fact.principal_id {
                                        if ui
                                            .link(principal)
                                            .on_hover_text(tr("inspector.focus_person"))
                                            .clicked()
                                        {
                                            action =
                                                Some(InspectorAction::Focus(principal_id.clone()));
                                        }
                                    } else {
                                        ui.label(RichText::new(principal).weak().small());
                                    }
                                }
                            }
                        });
                        let details = [
                            fact.date.as_deref(),
                            fact.place.as_deref(),
                            fact.value.as_deref(),
                        ]
                        .into_iter()
                        .flatten()
                        .filter(|detail| !detail.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join(" \u{00b7} ");
                        if details.is_empty() {
                            ui.label(RichText::new("\u{2014}").weak().small());
                        } else {
                            ui.label(RichText::new(details).weak().small());
                        }
                        if let Some(derived) = age::age_at(Some(&form.birth), fact.date.as_deref())
                        {
                            ui.label(
                                RichText::new(tr_args(
                                    "inspector.age",
                                    &[("age", derived.to_string())],
                                ))
                                .weak()
                                .small(),
                            );
                        }
                        if fact.role.is_none() && !fact.participants.is_empty() {
                            let witnesses = fact
                                .participants
                                .iter()
                                .map(|participant| {
                                    format!(
                                        "{} ({})",
                                        participant.person_name, participant.role_label
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            ui.label(
                                RichText::new(tr_args(
                                    "inspector.witnesses",
                                    &[("witnesses", witnesses)],
                                ))
                                .weak()
                                .small(),
                            );
                        }
                        let label = if fact.participants.is_empty() {
                            tr("inspector.add_witnesses")
                        } else {
                            tr("inspector.edit_witnesses")
                        };
                        if ui.small_button(label).clicked() {
                            action = Some(InspectorAction::EditParticipants(fact.event_id.clone()));
                        }
                        if ui.small_button(tr("inspector.notes_button")).clicked() {
                            action = Some(InspectorAction::OpenNotes {
                                owner: NoteOwner::Event(fact.event_id.clone()),
                                title: tr_args("inspector.notes_title", &[("label", fact_label)]),
                            });
                        }
                    });
                    ui.add_space(4.0);
                }
            });
            if let Some(privacy) = privacy {
                card(ui, &tr("inspector.card_privacy"), |ui| {
                    let mut private = privacy.private;
                    if ui
                        .checkbox(&mut private, tr("inspector.private_person"))
                        .changed()
                    {
                        action = Some(InspectorAction::SetPersonPrivate(private));
                    }
                    let current_override = match privacy.living_override {
                        None => tr_args(
                            "inspector.living_auto",
                            &[(
                                "status",
                                if privacy.auto_living {
                                    tr("inspector.living")
                                } else {
                                    tr("inspector.deceased")
                                },
                            )],
                        ),
                        Some(true) => tr("inspector.living"),
                        Some(false) => tr("inspector.deceased"),
                    };
                    ui.horizontal(|ui| {
                        ui.label(tr("inspector.living_status"));
                        egui::ComboBox::from_id_salt("privacy_living_override")
                            .selected_text(current_override)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        privacy.living_override.is_none(),
                                        tr_args(
                                            "inspector.living_auto",
                                            &[(
                                                "status",
                                                if privacy.auto_living {
                                                    tr("inspector.living")
                                                } else {
                                                    tr("inspector.deceased")
                                                },
                                            )],
                                        ),
                                    )
                                    .clicked()
                                {
                                    action = Some(InspectorAction::SetLivingOverride(None));
                                }
                                if ui
                                    .selectable_label(
                                        privacy.living_override == Some(true),
                                        tr("inspector.living"),
                                    )
                                    .clicked()
                                {
                                    action = Some(InspectorAction::SetLivingOverride(Some(true)));
                                }
                                if ui
                                    .selectable_label(
                                        privacy.living_override == Some(false),
                                        tr("inspector.deceased"),
                                    )
                                    .clicked()
                                {
                                    action = Some(InspectorAction::SetLivingOverride(Some(false)));
                                }
                            });
                    });
                    ui.label(
                        RichText::new(tr_args(
                            "inspector.effective_status",
                            &[(
                                "status",
                                if privacy.effective_living {
                                    tr("inspector.living")
                                } else {
                                    tr("inspector.deceased")
                                },
                            )],
                        ))
                        .weak()
                        .small(),
                    );
                    ui.separator();
                    ui.label(RichText::new(tr("inspector.private_facts")).strong());
                    if privacy.events.is_empty() {
                        ui.label(RichText::new(tr("inspector.none")).weak().small());
                    }
                    for event in &privacy.events {
                        ui.push_id(&event.event_id, |ui| {
                            ui.horizontal(|ui| {
                                let mut private = event.private;
                                let label = match event.date.as_deref() {
                                    Some(date) => format!(
                                        "{} ({date})",
                                        fact_label(&event.event_type, &event.label)
                                    ),
                                    None => fact_label(&event.event_type, &event.label),
                                };
                                if ui.checkbox(&mut private, label).changed() {
                                    action = Some(InspectorAction::SetEventPrivate {
                                        event_id: event.event_id.clone(),
                                        private,
                                    });
                                }
                                if event.family_event {
                                    ui.label(
                                        RichText::new(tr("inspector.family_badge")).weak().small(),
                                    );
                                }
                            });
                        });
                    }
                });
            }
            card(ui, &tr("inspector.card_citations"), |ui| {
                if let Some(event) = citations::show(ui, citations) {
                    action = Some(match event {
                        CitationsEvent::Save(edits) => {
                            InspectorAction::SaveCitations(Box::new(edits))
                        }
                        CitationsEvent::Add {
                            source_id,
                            target,
                            page,
                        } => InspectorAction::AddCitation {
                            source_id,
                            target,
                            page,
                        },
                        CitationsEvent::Attach {
                            citation_id,
                            target,
                        } => InspectorAction::AttachCitation {
                            citation_id,
                            target,
                        },
                        CitationsEvent::Detach {
                            citation_id,
                            event_id,
                        } => InspectorAction::DetachCitation {
                            citation_id,
                            event_id,
                        },
                        CitationsEvent::Delete(id) => InspectorAction::DeleteCitation(id),
                        CitationsEvent::LoadChoices(source_id) => {
                            InspectorAction::LoadCitationChoices(source_id)
                        }
                        CitationsEvent::Notes { citation_id, title } => {
                            InspectorAction::OpenNotes {
                                owner: NoteOwner::Citation(citation_id),
                                title,
                            }
                        }
                        CitationsEvent::Tasks { citation_id, title } => {
                            InspectorAction::ManageTasks {
                                owner: TaskOwner::Citation(citation_id),
                                title,
                            }
                        }
                    });
                }
            });
            card(ui, &tr("inspector.card_relationships"), |ui| {
                for (title, relatives, kind) in [
                    (
                        tr("inspector.parents"),
                        &connections.parents,
                        Some(RelationKind::Parent),
                    ),
                    (tr("inspector.siblings"), &connections.siblings, None),
                    (
                        tr("inspector.spouses"),
                        &connections.spouses,
                        Some(RelationKind::Spouse),
                    ),
                    (
                        tr("inspector.children"),
                        &connections.children,
                        Some(RelationKind::Child),
                    ),
                ] {
                    ui.label(RichText::new(title).weak().size(12.0));
                    if relatives.is_empty() {
                        ui.label(RichText::new(tr("inspector.none")).weak().size(12.0));
                    }
                    for relative in relatives {
                        let mut text = relative.label.clone();
                        if let Some(marriage) = &relative.marriage {
                            let mut marriage_facts =
                                [marriage.date.as_deref(), marriage.place.as_deref()]
                                    .into_iter()
                                    .flatten()
                                    .map(str::to_string)
                                    .collect::<Vec<_>>();
                            if let Some(derived) =
                                age::age_at(Some(&form.birth), marriage.date.as_deref())
                            {
                                marriage_facts.push(tr_args(
                                    "inspector.age_short",
                                    &[("age", derived.to_string())],
                                ));
                            }
                            if !marriage_facts.is_empty() {
                                text.push_str(&format!(
                                    " \u{00b7} {}",
                                    marriage_facts.join(" \u{00b7} ")
                                ));
                            }
                        }
                        let event_id = relative
                            .marriage
                            .as_ref()
                            .and_then(|marriage| marriage.event_id.clone());
                        let switch = editor_mode
                            && matches!(kind, Some(RelationKind::Parent | RelationKind::Spouse));
                        let hint = if switch {
                            tr("editor.switch_hint")
                        } else {
                            tr("editor.focus_hint")
                        };
                        let row = egui::Label::new(RichText::new(text).strong())
                            .sense(egui::Sense::click())
                            .truncate();
                        ui.horizontal(|ui| {
                            let width = if event_id.is_some() {
                                (ui.available_width() - 86.0).max(80.0)
                            } else if switch {
                                (ui.available_width() - 52.0).max(80.0)
                            } else {
                                ui.available_width()
                            };
                            if ui
                                .add_sized([width, ui.spacing().interact_size.y], row)
                                .on_hover_text(hint)
                                .clicked()
                            {
                                action = Some(if switch {
                                    InspectorAction::SwitchEditor(relative.id.clone())
                                } else {
                                    InspectorAction::Focus(relative.id.clone())
                                });
                            }
                            if switch && ui.small_button(tr("editor.focus")).clicked() {
                                action = Some(InspectorAction::Focus(relative.id.clone()));
                            }
                            if let Some(event_id) = event_id
                                && ui.small_button(tr("inspector.witnesses_button")).clicked()
                            {
                                action = Some(InspectorAction::EditParticipants(event_id));
                            }
                        });
                    }
                    match kind {
                        Some(RelationKind::Parent) => {
                            ui.horizontal_wrapped(|ui| {
                                if ui.small_button(tr("inspector.add_father")).clicked() {
                                    action = Some(InspectorAction::AddParentAs(ParentRole::Father));
                                }
                                if ui.small_button(tr("inspector.add_mother")).clicked() {
                                    action = Some(InspectorAction::AddParentAs(ParentRole::Mother));
                                }
                            });
                        }
                        Some(RelationKind::Spouse)
                            if ui.small_button(tr("inspector.add_spouse")).clicked() =>
                        {
                            action = Some(InspectorAction::AddSpouse);
                        }
                        Some(RelationKind::Child)
                            if ui.small_button(tr("inspector.add_child")).clicked() =>
                        {
                            action = Some(InspectorAction::AddChild);
                        }
                        Some(RelationKind::Spouse | RelationKind::Child) | None => {}
                    }
                    if kind.is_some() {
                        ui.add_space(4.0);
                    }
                }
            });
            card(ui, &tr("inspector.card_media"), |ui| {
                if ui.button(tr("inspector.manage_media")).clicked() {
                    action = Some(InspectorAction::ManageMedia {
                        owner: MediaOwner::Person(person.id.clone()),
                        title: tr_args(
                            "inspector.media_title",
                            &[("label", person_label(person, surname_first))],
                        ),
                    });
                }
            });
            card(ui, &tr("inspector.card_notes"), |ui| {
                if ui.button(tr("inspector.edit_notes")).clicked() {
                    action = Some(InspectorAction::OpenNotes {
                        owner: NoteOwner::Person(person.id.clone()),
                        title: tr_args(
                            "inspector.notes_title",
                            &[("name", person_label(person, surname_first))],
                        ),
                    });
                }
            });
        });
    ui.separator();
    let can_save = form.has_name() && form.dates_valid();
    if ui
        .add_enabled(
            can_save,
            egui::Button::new(RichText::new(tr("inspector.save_changes")).strong()).fill(ACCENT),
        )
        .clicked()
    {
        action = Some(InspectorAction::Save(Box::new(form.to_details())));
    }
    if !editor_mode && ui.button(tr("editor.open")).clicked() {
        action = Some(InspectorAction::OpenEditor);
    }
    action
}

/// One framed, titled section of the inspector body.
fn card(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(4.0)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong());
            ui.add_space(2.0);
            body(ui);
        });
}

fn gender_word(code: &str) -> String {
    tr(match code {
        "M" => "inspector.male",
        "F" => "inspector.female",
        _ => "inspector.unknown",
    })
}
