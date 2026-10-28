//! Rendering the inspector panel: framed cards and the profile editors.

use crate::gui::citations::{self, CitationsEvent, CitationsState};
use crate::gui::names::{self, NamesEvent, NamesState};

use egui::{RichText, Ui};

use crate::db::Person;
use crate::db::associations::AssociationRecord;
use crate::db::facts::PersonFact;
use crate::db::groups::GroupSummary;
use crate::db::media::MediaOwner;
use crate::db::notes::NoteOwner;
use crate::db::people::VitalDates;
use crate::db::privacy::PersonPrivacy;
use crate::db::tasks::TaskOwner;
use crate::i18n::{fact_label, tr};
use crate::model::age;

use super::ACCENT;
use super::InspectorAction;
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
            ui.label("Prefix:");
            ui.text_edit_singleline(&mut form.name_prefix);
            ui.end_row();

            ui.label("Given name:");
            ui.text_edit_singleline(&mut form.given_name);
            ui.end_row();

            ui.label("Middle name:");
            ui.text_edit_singleline(&mut form.middle_name);
            ui.end_row();

            ui.label("Surname:");
            ui.text_edit_singleline(&mut form.surname);
            ui.end_row();

            ui.label("Suffix:");
            ui.text_edit_singleline(&mut form.name_suffix);
            ui.end_row();
            if !has_name {
                ui.label("");
                ui.label(
                    RichText::new("Enter at least a first or last name.")
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
                ui.end_row();
            }

            ui.label("Nickname:");
            ui.text_edit_singleline(&mut form.nickname);
            ui.end_row();

            ui.label("Gender:");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut form.gender, "M".to_string(), "Male");
                ui.selectable_value(&mut form.gender, "F".to_string(), "Female");
                ui.selectable_value(&mut form.gender, "U".to_string(), "Unknown");
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
            ui.label("Birth date:");
            ui.add(egui::TextEdit::singleline(&mut form.birth).hint_text("e.g. 12 JAN 1850"));
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

            ui.label("Birth place:");
            ui.text_edit_singleline(&mut form.birth_place);
            ui.end_row();

            ui.label("Death date:");
            ui.add(egui::TextEdit::singleline(&mut form.death).hint_text("e.g. 12 JAN 1850"));
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

            ui.label("Death place:");
            ui.text_edit_singleline(&mut form.death_place);
            ui.end_row();
        });
    if let Some(derived) = age::age_at(Some(&form.birth), Some(&form.death)) {
        ui.label(
            RichText::new(format!("Age at death: {derived}"))
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
    citations: &mut CitationsState,
    facts: &[PersonFact],
    privacy: Option<&PersonPrivacy>,
    names: &mut NamesState,
    surname_first: bool,
    editor_mode: bool,
) -> Option<InspectorAction> {
    let Some(person) = person else {
        ui.label(RichText::new("Selected person not found.").weak());
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
                    .on_hover_text("Close inspector")
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
                ui.label(
                    RichText::new(gender_word(person.gender.as_str()))
                        .weak()
                        .size(12.0),
                );
            });
            card(ui, "Profile", |ui| name_editors(ui, form));
            card(ui, "Names", |ui| {
                if let Some(event) = names::show(ui, names, surname_first) {
                    action = Some(match event {
                        NamesEvent::Save(draft) => InspectorAction::SaveAlternateName(draft),
                        NamesEvent::Delete(id) => InspectorAction::DeleteAlternateName(id),
                    });
                }
            });
            card(ui, "Vital Facts", |ui| {
                vital_editors(ui, form);
                if let Some(dates) = dates {
                    for (label, event_id) in [
                        ("Birth", dates.birth_event_id.as_deref()),
                        ("Death", dates.death_event_id.as_deref()),
                    ] {
                        if let Some(event_id) = event_id
                            && ui.small_button(format!("{label} notes…")).clicked()
                        {
                            action = Some(InspectorAction::OpenNotes {
                                owner: NoteOwner::Event(event_id.to_string()),
                                title: format!("Notes — {label}"),
                            });
                        }
                    }
                }
            });
            if !connections.families.is_empty() {
                card(ui, "Families", |ui| {
                    for family in &connections.families {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&family.label).strong());
                            if ui.small_button("Notes…").clicked() {
                                action = Some(InspectorAction::OpenNotes {
                                    owner: NoteOwner::Family(family.id.clone()),
                                    title: format!("Notes — {}", family.label),
                                });
                            }
                        });
                    }
                });
            }
            card(ui, "Associations", |ui| {
                if associations.is_empty() {
                    ui.label(RichText::new("(none)").weak().size(12.0));
                }
                for association in associations {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&association.relationship).strong());
                        if ui
                            .link(&association.associate_name)
                            .on_hover_text("Focus this person")
                            .clicked()
                        {
                            action = Some(InspectorAction::Focus(association.associate_id.clone()));
                        }
                        if ui.small_button("Edit").clicked() {
                            action = Some(InspectorAction::EditAssociation(association.id.clone()));
                        }
                        if ui.small_button("Delete").clicked() {
                            action =
                                Some(InspectorAction::DeleteAssociation(association.id.clone()));
                        }
                    });
                }
                if ui.button("+ Add Association").clicked() {
                    action = Some(InspectorAction::AddAssociation);
                }
            });
            if !groups.is_empty() {
                card(ui, &tr("editor.groups"), |ui| {
                    for group in groups {
                        ui.label(RichText::new(&group.name).strong());
                    }
                    if ui.button(tr("editor.manage_groups")).clicked() {
                        action = Some(InspectorAction::OpenGroups);
                    }
                });
            }
            card(ui, "Other Facts", |ui| {
                if facts.is_empty() {
                    ui.label(RichText::new("(none)").weak().size(12.0));
                }
                for fact in facts {
                    ui.push_id(&fact.event_id, |ui| {
                        let fact_label = fact_label(&fact.event_type, &fact.label);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(fact_label.clone()).strong());
                            if let Some(role) = &fact.role {
                                ui.label(RichText::new(format!("as {role} for")).weak().small());
                                if let Some(principal) = &fact.principal {
                                    if let Some(principal_id) = &fact.principal_id {
                                        if ui
                                            .link(principal)
                                            .on_hover_text("Focus this person")
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
                            ui.label(RichText::new(format!("Age: {derived}")).weak().small());
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
                                RichText::new(format!("Witnesses: {witnesses}"))
                                    .weak()
                                    .small(),
                            );
                        }
                        let label = if fact.participants.is_empty() {
                            "Add witnesses"
                        } else {
                            "Edit witnesses"
                        };
                        if ui.small_button(label).clicked() {
                            action = Some(InspectorAction::EditParticipants(fact.event_id.clone()));
                        }
                        if ui.small_button("Notes…").clicked() {
                            action = Some(InspectorAction::OpenNotes {
                                owner: NoteOwner::Event(fact.event_id.clone()),
                                title: format!("Notes — {}", fact_label),
                            });
                        }
                    });
                    ui.add_space(4.0);
                }
            });
            if let Some(privacy) = privacy {
                card(ui, "Privacy", |ui| {
                    let mut private = privacy.private;
                    if ui.checkbox(&mut private, "Private person").changed() {
                        action = Some(InspectorAction::SetPersonPrivate(private));
                    }
                    let current_override = match privacy.living_override {
                        None => format!(
                            "Auto ({})",
                            if privacy.auto_living {
                                "Living"
                            } else {
                                "Deceased"
                            }
                        ),
                        Some(true) => "Living".to_string(),
                        Some(false) => "Deceased".to_string(),
                    };
                    ui.horizontal(|ui| {
                        ui.label("Living status:");
                        egui::ComboBox::from_id_salt("privacy_living_override")
                            .selected_text(current_override)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        privacy.living_override.is_none(),
                                        format!(
                                            "Auto ({})",
                                            if privacy.auto_living {
                                                "Living"
                                            } else {
                                                "Deceased"
                                            }
                                        ),
                                    )
                                    .clicked()
                                {
                                    action = Some(InspectorAction::SetLivingOverride(None));
                                }
                                if ui
                                    .selectable_label(
                                        privacy.living_override == Some(true),
                                        "Living",
                                    )
                                    .clicked()
                                {
                                    action = Some(InspectorAction::SetLivingOverride(Some(true)));
                                }
                                if ui
                                    .selectable_label(
                                        privacy.living_override == Some(false),
                                        "Deceased",
                                    )
                                    .clicked()
                                {
                                    action = Some(InspectorAction::SetLivingOverride(Some(false)));
                                }
                            });
                    });
                    ui.label(
                        RichText::new(format!(
                            "Effective status: {}",
                            if privacy.effective_living {
                                "Living"
                            } else {
                                "Deceased"
                            }
                        ))
                        .weak()
                        .small(),
                    );
                    ui.separator();
                    ui.label(RichText::new("Private facts").strong());
                    if privacy.events.is_empty() {
                        ui.label(RichText::new("(none)").weak().small());
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
                                    ui.label(RichText::new("family").weak().small());
                                }
                            });
                        });
                    }
                });
            }
            card(ui, "Citations", |ui| {
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
            card(ui, "Relationships", |ui| {
                for (title, relatives) in [
                    ("Parents", &connections.parents),
                    ("Siblings", &connections.siblings),
                    ("Spouses", &connections.spouses),
                    ("Children", &connections.children),
                ] {
                    ui.label(RichText::new(title).weak().size(12.0));
                    if relatives.is_empty() {
                        ui.label(RichText::new("(none)").weak().size(12.0));
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
                                marriage_facts.push(format!("Age {derived}"));
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
                        let row = egui::Label::new(RichText::new(text).strong())
                            .sense(egui::Sense::click())
                            .truncate();
                        ui.horizontal(|ui| {
                            let width = if event_id.is_some() {
                                (ui.available_width() - 86.0).max(80.0)
                            } else {
                                ui.available_width()
                            };
                            if ui
                                .add_sized([width, ui.spacing().interact_size.y], row)
                                .clicked()
                            {
                                action = Some(InspectorAction::Focus(relative.id.clone()));
                            }
                            if let Some(event_id) = event_id
                                && ui.small_button("Witnesses").clicked()
                            {
                                action = Some(InspectorAction::EditParticipants(event_id));
                            }
                        });
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("+ Add Parent").clicked() {
                        action = Some(InspectorAction::AddParent);
                    }
                    if ui.button("+ Add Spouse").clicked() {
                        action = Some(InspectorAction::AddSpouse);
                    }
                    if ui.button("+ Add Child").clicked() {
                        action = Some(InspectorAction::AddChild);
                    }
                });
            });
            card(ui, "Media", |ui| {
                if ui.button("Manage media\u{2026}").clicked() {
                    action = Some(InspectorAction::ManageMedia {
                        owner: MediaOwner::Person(person.id.clone()),
                        title: format!("Media \u{2014} {}", person_label(person, surname_first)),
                    });
                }
            });
            card(ui, "Notes", |ui| {
                if ui.button("Edit notes\u{2026}").clicked() {
                    action = Some(InspectorAction::OpenNotes {
                        owner: NoteOwner::Person(person.id.clone()),
                        title: format!("Notes — {}", person_label(person, surname_first)),
                    });
                }
            });
        });
    ui.separator();
    let can_save = form.has_name() && form.dates_valid();
    if ui
        .add_enabled(
            can_save,
            egui::Button::new(RichText::new("Save Changes").strong()).fill(ACCENT),
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

fn gender_word(code: &str) -> &'static str {
    match code {
        "M" => "Male",
        "F" => "Female",
        _ => "Unknown",
    }
}
