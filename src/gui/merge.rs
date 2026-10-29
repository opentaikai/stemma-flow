//! Side-by-side comparison for permanently merging duplicate people.

use egui::{Color32, Context, RichText, Ui, Window};

use crate::db::FamilyTree;
use crate::db::merge::{MergeBlocker, MergeDraft, MergePreview};
use crate::db::people::PersonDetails;
use crate::i18n::{tr, tr_args};

use super::inspector::person_label;

const CONFLICT_COLOR: Color32 = Color32::from_rgb(217, 119, 6);

/// Which record supplies one compared value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MergePick {
    #[default]
    Keep,
    Merge,
}

/// Field-by-field choices applied to the kept record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeChoices {
    pub name_prefix: MergePick,
    pub given_name: MergePick,
    pub middle_name: MergePick,
    pub surname: MergePick,
    pub name_suffix: MergePick,
    pub nickname: MergePick,
    pub gender: MergePick,
    pub birth_date: MergePick,
    pub birth_place: MergePick,
    pub death_date: MergePick,
    pub death_place: MergePick,
    pub privacy: MergePick,
    pub preserve_primary_names: bool,
}

impl MergeChoices {
    /// Builds smart defaults: keep nonblank values, fill blanks from the duplicate.
    pub fn from_preview(preview: &MergePreview) -> Self {
        let keep = &preview.keep.details;
        let merge = &preview.merge.details;
        Self {
            name_prefix: pick_text(&keep.name_prefix, &merge.name_prefix),
            given_name: pick_text(&keep.given_name, &merge.given_name),
            middle_name: pick_text(&keep.middle_name, &merge.middle_name),
            surname: pick_text(&keep.surname, &merge.surname),
            name_suffix: pick_text(&keep.name_suffix, &merge.name_suffix),
            nickname: pick_text(&keep.nickname, &merge.nickname),
            gender: if keep.gender == "U" && merge.gender != "U" {
                MergePick::Merge
            } else {
                MergePick::Keep
            },
            birth_date: pick_optional(&keep.birth_date, &merge.birth_date),
            birth_place: pick_optional(&keep.birth_place, &merge.birth_place),
            death_date: pick_optional(&keep.death_date, &merge.death_date),
            death_place: pick_optional(&keep.death_place, &merge.death_place),
            privacy: MergePick::Keep,
            preserve_primary_names: true,
        }
    }

    /// Applies the choices to the kept profile.
    pub fn details(&self, preview: &MergePreview) -> PersonDetails {
        let mut details = preview.keep.details.clone();
        let merge = &preview.merge.details;
        if self.name_prefix == MergePick::Merge {
            details.name_prefix = merge.name_prefix.clone();
        }
        if self.given_name == MergePick::Merge {
            details.given_name = merge.given_name.clone();
        }
        if self.middle_name == MergePick::Merge {
            details.middle_name = merge.middle_name.clone();
        }
        if self.surname == MergePick::Merge {
            details.surname = merge.surname.clone();
        }
        if self.name_suffix == MergePick::Merge {
            details.name_suffix = merge.name_suffix.clone();
        }
        if self.nickname == MergePick::Merge {
            details.nickname = merge.nickname.clone();
        }
        if self.gender == MergePick::Merge {
            details.gender = merge.gender.clone();
        }
        if self.birth_date == MergePick::Merge {
            details.birth_date = merge.birth_date.clone();
        }
        if self.birth_place == MergePick::Merge {
            details.birth_place = merge.birth_place.clone();
        }
        if self.death_date == MergePick::Merge {
            details.death_date = merge.death_date.clone();
        }
        if self.death_place == MergePick::Merge {
            details.death_place = merge.death_place.clone();
        }
        details
    }

    pub fn privacy(&self, preview: &MergePreview) -> (bool, Option<bool>) {
        match self.privacy {
            MergePick::Keep => (preview.keep.private, preview.keep.living_override),
            MergePick::Merge => (preview.merge.private, preview.merge.living_override),
        }
    }
}

fn pick_text(keep: &str, merge: &str) -> MergePick {
    if keep.trim().is_empty() && !merge.trim().is_empty() {
        MergePick::Merge
    } else {
        MergePick::Keep
    }
}

fn pick_optional(keep: &Option<String>, merge: &Option<String>) -> MergePick {
    let keep_blank = keep.as_deref().map(str::trim).is_none_or(str::is_empty);
    let merge_blank = merge.as_deref().map(str::trim).is_none_or(str::is_empty);
    if keep_blank && !merge_blank {
        MergePick::Merge
    } else {
        MergePick::Keep
    }
}

/// State of the merge dialog.
#[derive(Debug, Default)]
pub struct MergeState {
    pub open: bool,
    pub keep: Option<String>,
    pub merge: Option<String>,
    pub keep_search: String,
    pub merge_search: String,
    pub preview: Option<MergePreview>,
    pub preview_pair: Option<(String, String)>,
    pub choices: MergeChoices,
    pub confirm: bool,
    pub error: Option<String>,
}

impl MergeState {
    pub fn open_with(&mut self, keep: Option<String>) {
        *self = Self {
            open: true,
            keep,
            ..Self::default()
        };
    }

    pub fn set_preview(&mut self, preview: MergePreview) {
        self.choices = MergeChoices::from_preview(&preview);
        self.preview_pair = Some((preview.keep.id.clone(), preview.merge.id.clone()));
        self.preview = Some(preview);
        self.confirm = false;
        self.error = None;
    }

    pub fn clear_preview(&mut self) {
        self.preview = None;
        self.preview_pair = None;
        self.confirm = false;
    }
}

/// What the dialog asks the window to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeEvent {
    Confirm(Box<MergeDraft>),
}

/// Draws the merge window. The caller loads the preview when the pair changes.
pub fn show(
    ctx: &Context,
    state: &mut MergeState,
    tree: &FamilyTree,
    surname_first: bool,
) -> Option<MergeEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("merge.title"))
        .id(egui::Id::new("merge_people_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(760.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("merge.hint")).weak().small());
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    person_picker(
                        ui,
                        &tr("merge.keep_label"),
                        &mut state.keep,
                        &mut state.keep_search,
                        tree,
                        surname_first,
                    );
                });
                ui.separator();
                ui.vertical(|ui| {
                    person_picker(
                        ui,
                        &tr("merge.merge_label"),
                        &mut state.merge,
                        &mut state.merge_search,
                        tree,
                        surname_first,
                    );
                });
            });

            let same = state.keep.is_some() && state.keep == state.merge;
            if same {
                state.error = Some(tr("merge.blocked_same"));
            }

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        state.keep.is_some() && state.merge.is_some() && !same,
                        egui::Button::new(tr("merge.swap")),
                    )
                    .clicked()
                {
                    std::mem::swap(&mut state.keep, &mut state.merge);
                    std::mem::swap(&mut state.keep_search, &mut state.merge_search);
                    state.clear_preview();
                    state.error = None;
                }
                if let Some(error) = &state.error {
                    ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
                }
            });

            let Some(preview) = state.preview.as_ref() else {
                if state.error.is_some() {
                    return;
                }
                if state.keep.is_some() && state.merge.is_some() && !same {
                    ui.label(RichText::new(tr("merge.loading")).weak());
                } else if !same {
                    ui.label(RichText::new(tr("merge.choose_two")).weak());
                }
                return;
            };
            if state.keep.as_deref() != Some(preview.keep.id.as_str())
                || state.merge.as_deref() != Some(preview.merge.id.as_str())
            {
                ui.label(RichText::new(tr("merge.loading")).weak());
                return;
            }

            ui.separator();
            if let Some(blocker) = preview.blocked {
                ui.label(
                    RichText::new(blocker_text(blocker))
                        .color(ui.visuals().error_fg_color)
                        .strong(),
                );
            }

            ui.label(
                RichText::new(tr_args(
                    "merge.columns",
                    &[
                        (
                            "keep",
                            person_label_by_id(tree, &preview.keep.id, surname_first),
                        ),
                        (
                            "merge",
                            person_label_by_id(tree, &preview.merge.id, surname_first),
                        ),
                    ],
                ))
                .strong(),
            );
            egui::Grid::new("merge_field_grid")
                .num_columns(3)
                .striped(true)
                .spacing([12.0, 5.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(tr("merge.field")).strong());
                    ui.label(RichText::new(tr("merge.keep_column")).strong());
                    ui.label(RichText::new(tr("merge.merge_column")).strong());
                    ui.end_row();
                    choice_row(
                        ui,
                        &tr("merge.field_prefix"),
                        &preview.keep.details.name_prefix,
                        &preview.merge.details.name_prefix,
                        &mut state.choices.name_prefix,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_given"),
                        &preview.keep.details.given_name,
                        &preview.merge.details.given_name,
                        &mut state.choices.given_name,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_middle"),
                        &preview.keep.details.middle_name,
                        &preview.merge.details.middle_name,
                        &mut state.choices.middle_name,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_surname"),
                        &preview.keep.details.surname,
                        &preview.merge.details.surname,
                        &mut state.choices.surname,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_suffix"),
                        &preview.keep.details.name_suffix,
                        &preview.merge.details.name_suffix,
                        &mut state.choices.name_suffix,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_nickname"),
                        &preview.keep.details.nickname,
                        &preview.merge.details.nickname,
                        &mut state.choices.nickname,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_gender"),
                        &gender_label(&preview.keep.details.gender),
                        &gender_label(&preview.merge.details.gender),
                        &mut state.choices.gender,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_birth_date"),
                        preview.keep.details.birth_date.as_deref().unwrap_or(""),
                        preview.merge.details.birth_date.as_deref().unwrap_or(""),
                        &mut state.choices.birth_date,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_birth_place"),
                        preview.keep.details.birth_place.as_deref().unwrap_or(""),
                        preview.merge.details.birth_place.as_deref().unwrap_or(""),
                        &mut state.choices.birth_place,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_death_date"),
                        preview.keep.details.death_date.as_deref().unwrap_or(""),
                        preview.merge.details.death_date.as_deref().unwrap_or(""),
                        &mut state.choices.death_date,
                    );
                    choice_row(
                        ui,
                        &tr("merge.field_death_place"),
                        preview.keep.details.death_place.as_deref().unwrap_or(""),
                        preview.merge.details.death_place.as_deref().unwrap_or(""),
                        &mut state.choices.death_place,
                    );
                    let keep_privacy =
                        privacy_label(preview.keep.private, preview.keep.living_override);
                    let merge_privacy =
                        privacy_label(preview.merge.private, preview.merge.living_override);
                    choice_row(
                        ui,
                        &tr("merge.field_privacy"),
                        &keep_privacy,
                        &merge_privacy,
                        &mut state.choices.privacy,
                    );
                });

            ui.checkbox(
                &mut state.choices.preserve_primary_names,
                tr("merge.preserve_names"),
            );
            ui.separator();
            summary(ui, preview, tree, surname_first);

            ui.separator();
            if state.confirm {
                ui.label(
                    RichText::new(tr_args(
                        "merge.confirm_warning",
                        &[
                            (
                                "merge",
                                person_label_by_id(tree, &preview.merge.id, surname_first),
                            ),
                            (
                                "keep",
                                person_label_by_id(tree, &preview.keep.id, surname_first),
                            ),
                        ],
                    ))
                    .color(ui.visuals().error_fg_color)
                    .strong(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            preview.blocked.is_none(),
                            egui::Button::new(RichText::new(tr("merge.confirm")).strong())
                                .fill(ui.visuals().error_fg_color),
                        )
                        .clicked()
                    {
                        let (private, living_override) = state.choices.privacy(preview);
                        event = Some(MergeEvent::Confirm(Box::new(MergeDraft {
                            keep_id: preview.keep.id.clone(),
                            merge_id: preview.merge.id.clone(),
                            details: state.choices.details(preview),
                            private,
                            living_override,
                            preserve_primary_names: state.choices.preserve_primary_names,
                        })));
                    }
                    if ui.button(tr("merge.back")).clicked() {
                        state.confirm = false;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            preview.blocked.is_none(),
                            egui::Button::new(RichText::new(tr("merge.start")).strong()),
                        )
                        .clicked()
                    {
                        state.confirm = true;
                    }
                    if ui.button(tr("common.cancel")).clicked() {
                        state.open = false;
                    }
                });
            }
        });
    if !open {
        state.open = false;
    }
    event
}

fn person_picker(
    ui: &mut Ui,
    label: &str,
    selected: &mut Option<String>,
    search: &mut String,
    tree: &FamilyTree,
    surname_first: bool,
) {
    ui.label(RichText::new(label).strong());
    let selected_label = selected
        .as_deref()
        .and_then(|id| {
            tree.people
                .iter()
                .find(|person| person.id == id)
                .map(|person| person_label(person, surname_first))
        })
        .unwrap_or_else(|| tr("merge.none"));
    ui.label(selected_label);
    ui.add(egui::TextEdit::singleline(search).hint_text(tr("merge.search")));
    let needle = search.trim().to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt(("merge_picker", label))
        .max_height(150.0)
        .show(ui, |ui| {
            for person in tree.people.iter().filter(|person| {
                needle.is_empty()
                    || person_label(person, surname_first)
                        .to_lowercase()
                        .contains(&needle)
            }) {
                if ui
                    .selectable_label(
                        selected.as_deref() == Some(person.id.as_str()),
                        person_label(person, surname_first),
                    )
                    .clicked()
                {
                    *selected = Some(person.id.clone());
                }
            }
        });
}

fn choice_row(ui: &mut Ui, label: &str, keep: &str, merge: &str, pick: &mut MergePick) {
    ui.label(label);
    let conflict =
        !keep.trim().is_empty() && !merge.trim().is_empty() && keep.trim() != merge.trim();
    let color = if conflict {
        CONFLICT_COLOR
    } else {
        ui.visuals().text_color()
    };
    let keep_text = if keep.trim().is_empty() {
        tr("merge.empty")
    } else {
        keep.to_string()
    };
    let merge_text = if merge.trim().is_empty() {
        tr("merge.empty")
    } else {
        merge.to_string()
    };
    if ui
        .selectable_label(
            *pick == MergePick::Keep,
            RichText::new(keep_text).color(color),
        )
        .clicked()
    {
        *pick = MergePick::Keep;
    }
    if ui
        .selectable_label(
            *pick == MergePick::Merge,
            RichText::new(merge_text).color(color),
        )
        .clicked()
    {
        *pick = MergePick::Merge;
    }
    ui.end_row();
}

fn summary(ui: &mut Ui, preview: &MergePreview, tree: &FamilyTree, surname_first: bool) {
    let moved = &preview.merge.counts;
    ui.label(
        RichText::new(tr_args(
            "merge.moved_summary",
            &[
                ("events", moved.events.to_string()),
                ("notes", moved.notes.to_string()),
                ("media", moved.media.to_string()),
                ("tasks", moved.tasks.to_string()),
                ("groups", moved.groups.to_string()),
                ("associations", moved.associations.to_string()),
                ("names", moved.alternate_names.to_string()),
            ],
        ))
        .small(),
    );
    ui.label(
        RichText::new(tr_args(
            "merge.family_summary",
            &[
                (
                    "spouse_families",
                    preview.family_impact.spouse_families.to_string(),
                ),
                (
                    "parent_families",
                    preview.family_impact.parent_families.to_string(),
                ),
                ("children", preview.family_impact.child_links.to_string()),
            ],
        ))
        .small(),
    );
    ui.label(
        RichText::new(tr_args(
            "merge.delete_warning",
            &[(
                "name",
                person_label_by_id(tree, &preview.merge.id, surname_first),
            )],
        ))
        .color(ui.visuals().error_fg_color)
        .small(),
    );
}

fn person_label_by_id(tree: &FamilyTree, id: &str, surname_first: bool) -> String {
    tree.people
        .iter()
        .find(|person| person.id == id)
        .map(|person| person_label(person, surname_first))
        .unwrap_or_else(|| tr("merge.none"))
}

fn gender_label(code: &str) -> String {
    tr(match code {
        "M" => "merge.gender_male",
        "F" => "merge.gender_female",
        _ => "merge.gender_unknown",
    })
}

fn privacy_label(private: bool, living_override: Option<bool>) -> String {
    let private = if private {
        tr("merge.private_yes")
    } else {
        tr("merge.private_no")
    };
    let living = match living_override {
        Some(true) => tr("merge.living_yes"),
        Some(false) => tr("merge.living_no"),
        None => tr("merge.living_auto"),
    };
    format!("{private}, {living}")
}

fn blocker_text(blocker: MergeBlocker) -> String {
    tr(match blocker {
        MergeBlocker::SamePerson => "merge.blocked_same",
        MergeBlocker::DirectParentChild => "merge.blocked_parent_child",
        MergeBlocker::Spouses => "merge.blocked_spouses",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::merge::{MergeCounts, MergePerson};
    use egui::{Context, RawInput};

    fn details(given: &str, surname: &str) -> PersonDetails {
        PersonDetails::new(given, "", surname, "", "F", None, None)
    }

    fn preview() -> MergePreview {
        let mut keep_details = details("Ada", "King");
        keep_details.birth_date = Some("1815".to_string());
        let mut merge_details = details("", "Lovelace");
        merge_details.birth_place = Some("London".to_string());
        MergePreview {
            keep: MergePerson {
                id: "keep".to_string(),
                details: keep_details,
                private: false,
                living_override: None,
                counts: MergeCounts::default(),
            },
            merge: MergePerson {
                id: "merge".to_string(),
                details: merge_details,
                private: true,
                living_override: Some(false),
                counts: MergeCounts::default(),
            },
            family_impact: Default::default(),
            blocked: None,
        }
    }

    #[test]
    fn smart_defaults_fill_blanks_and_keep_nonblank_values() {
        let preview = preview();
        let choices = MergeChoices::from_preview(&preview);
        assert_eq!(choices.given_name, MergePick::Keep);
        assert_eq!(choices.surname, MergePick::Keep);
        assert_eq!(choices.middle_name, MergePick::Keep);
        assert_eq!(choices.birth_date, MergePick::Keep);
        assert_eq!(choices.birth_place, MergePick::Merge);
        let details = choices.details(&preview);
        assert_eq!(details.given_name, "Ada");
        assert_eq!(details.surname, "King");
        assert_eq!(details.birth_place.as_deref(), Some("London"));

        let mut changed = choices;
        changed.given_name = MergePick::Merge;
        changed.surname = MergePick::Merge;
        let details = changed.details(&preview);
        assert_eq!(details.given_name, "");
        assert_eq!(details.surname, "Lovelace");
    }

    #[test]
    fn dialog_renders_headless() {
        let ctx = Context::default();
        let mut state = MergeState {
            open: true,
            keep: Some("keep".to_string()),
            merge: Some("merge".to_string()),
            ..MergeState::default()
        };
        state.set_preview(preview());
        let mut keep = crate::db::Person::new("Ada", "King", "F");
        keep.id = "keep".to_string();
        let mut merge = crate::db::Person::new("Augusta", "Lovelace", "F");
        merge.id = "merge".to_string();
        let tree = FamilyTree {
            people: vec![keep, merge],
            families: Vec::new(),
            child_links: Vec::new(),
        };
        let mut event = Some(MergeEvent::Confirm(Box::new(MergeDraft {
            keep_id: String::new(),
            merge_id: String::new(),
            details: details("", ""),
            private: false,
            living_override: None,
            preserve_primary_names: true,
        })));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            event = show(ui.ctx(), &mut state, &tree, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(event.is_none(), "no input does not merge");
        assert!(state.open);
    }

    #[test]
    fn blocked_previews_are_reported() {
        assert_eq!(
            blocker_text(MergeBlocker::DirectParentChild),
            tr("merge.blocked_parent_child")
        );
        let mut preview = preview();
        preview.blocked = Some(MergeBlocker::Spouses);
        assert_eq!(preview.blocked, Some(MergeBlocker::Spouses));
    }
}
