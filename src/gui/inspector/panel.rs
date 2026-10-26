//! Rendering the inspector panel: framed cards and the profile editors.

use crate::gui::citations::{self, CitationsEvent, CitationsState};

use egui::{RichText, Ui};

use crate::db::Person;

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
            ui.label("Given name:");
            ui.text_edit_singleline(&mut form.given_name);
            ui.end_row();

            ui.label("Middle name:");
            ui.text_edit_singleline(&mut form.middle_name);
            ui.end_row();

            ui.label("Surname:");
            ui.text_edit_singleline(&mut form.surname);
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
}

/// Draws the inspector body and returns the action to apply, if any.
pub fn show(
    ui: &mut Ui,
    person: Option<&Person>,
    form: &mut InspectorForm,
    connections: Connections,
    citations: &mut CitationsState,
) -> Option<InspectorAction> {
    let Some(person) = person else {
        ui.label(RichText::new("Selected person not found.").weak());
        return None;
    };

    let mut action = None;
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
    let footer_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y * 5.0;
    let body_height = (ui.available_height() - footer_height).max(120.0);
    egui::ScrollArea::vertical()
        .id_salt("person_inspector_body")
        .auto_shrink([false, false])
        .max_height(body_height)
        .show(ui, |ui| {
            card(ui, &person_label(person), |ui| {
                ui.label(
                    RichText::new(gender_word(person.gender.as_str()))
                        .weak()
                        .size(12.0),
                );
            });
            card(ui, "Profile", |ui| name_editors(ui, form));
            card(ui, "Vital Facts", |ui| vital_editors(ui, form));
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
                            let facts = [marriage.date.as_deref(), marriage.place.as_deref()]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" \u{00b7} ");
                            if !facts.is_empty() {
                                text.push_str(&format!(" \u{00b7} {facts}"));
                            }
                        }
                        let row = egui::Label::new(RichText::new(text).strong())
                            .sense(egui::Sense::click())
                            .truncate();
                        if ui
                            .add_sized([ui.available_width(), ui.spacing().interact_size.y], row)
                            .clicked()
                        {
                            action = Some(InspectorAction::Focus(relative.id.clone()));
                        }
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
            card(ui, "Notes", |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut form.notes)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .hint_text("Research notes\u{2026}"),
                );
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
