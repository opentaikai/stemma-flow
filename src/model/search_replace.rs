//! Literal search-and-replace planning for names, places, notes and media paths.

use crate::model::dataclean::NameParts;

/// The kind of record a replacement row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReplaceKind {
    Name,
    AlternateName,
    Place,
    Note,
    MediaPath,
}

impl ReplaceKind {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Name => "replace.kind_names",
            Self::AlternateName => "replace.kind_alternate_names",
            Self::Place => "replace.kind_places",
            Self::Note => "replace.kind_notes",
            Self::MediaPath => "replace.kind_media_paths",
        }
    }
}

/// One editable person-name component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NameField {
    Prefix,
    Given,
    Middle,
    Surname,
    Suffix,
    Nickname,
}

impl NameField {
    pub fn key(self) -> &'static str {
        match self {
            Self::Prefix => "prefix",
            Self::Given => "given",
            Self::Middle => "middle",
            Self::Surname => "surname",
            Self::Suffix => "suffix",
            Self::Nickname => "nickname",
        }
    }

    pub fn column(self) -> &'static str {
        match self {
            Self::Prefix => "name_prefix",
            Self::Given => "given_name",
            Self::Middle => "middle_name",
            Self::Surname => "surname",
            Self::Suffix => "name_suffix",
            Self::Nickname => "nickname",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Prefix => "replace.field_prefix",
            Self::Given => "replace.field_given",
            Self::Middle => "replace.field_middle",
            Self::Surname => "replace.field_surname",
            Self::Suffix => "replace.field_suffix",
            Self::Nickname => "replace.field_nickname",
        }
    }
}

/// The exact record and field a replacement updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplaceTarget {
    Person {
        person_id: String,
        field: NameField,
    },
    AlternateName {
        alternate_id: String,
        field: NameField,
    },
    Place {
        place_id: String,
    },
    Note {
        note_id: String,
    },
    MediaPath {
        media_id: String,
    },
}

/// One previewable field replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceChange {
    pub key: String,
    pub target: ReplaceTarget,
    pub owner: String,
    pub field: String,
    pub before: String,
    pub after: String,
}

impl ReplaceChange {
    pub fn kind(&self) -> ReplaceKind {
        match self.target {
            ReplaceTarget::Person { .. } => ReplaceKind::Name,
            ReplaceTarget::AlternateName { .. } => ReplaceKind::AlternateName,
            ReplaceTarget::Place { .. } => ReplaceKind::Place,
            ReplaceTarget::Note { .. } => ReplaceKind::Note,
            ReplaceTarget::MediaPath { .. } => ReplaceKind::MediaPath,
        }
    }
}

/// User-entered replacement options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplaceRequest {
    pub find: String,
    pub replace: String,
    pub match_case: bool,
    pub names: bool,
    pub places: bool,
    pub notes: bool,
    pub filenames: bool,
}

impl ReplaceRequest {
    pub fn has_scope(&self) -> bool {
        self.names || self.places || self.notes || self.filenames
    }
}

/// A person or alternate name with its current parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameSource {
    pub person_id: String,
    pub alternate_id: Option<String>,
    pub owner: String,
    pub parts: NameParts,
}

/// A place name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceSource {
    pub id: String,
    pub name: String,
}

/// A note with its owner label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSource {
    pub id: String,
    pub body: String,
    pub owner: String,
}

/// A media record's stored path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSource {
    pub id: String,
    pub file_path: String,
    pub owner: String,
}

/// The complete proposed replacement before selection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplacePlan {
    pub changes: Vec<ReplaceChange>,
}

impl ReplacePlan {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Builds a deterministic plan from loaded record text.
pub fn build_plan(
    request: &ReplaceRequest,
    names: &[NameSource],
    places: &[PlaceSource],
    notes: &[NoteSource],
    media: &[MediaSource],
) -> ReplacePlan {
    if request.find.is_empty() || !request.has_scope() {
        return ReplacePlan::default();
    }
    let mut changes = Vec::new();

    if request.names {
        for source in names {
            let parts = &source.parts;
            let owner = display_owner(parts);
            for (field, value) in [
                (NameField::Prefix, &parts.prefix),
                (NameField::Given, &parts.given_name),
                (NameField::Middle, &parts.middle_name),
                (NameField::Surname, &parts.surname),
                (NameField::Suffix, &parts.suffix),
                (NameField::Nickname, &parts.nickname),
            ] {
                let Some(after) =
                    replace_text(value, &request.find, &request.replace, request.match_case)
                else {
                    continue;
                };
                let (key, target) = match &source.alternate_id {
                    Some(alternate_id) => (
                        format!("alternate:{alternate_id}:{}", field.key()),
                        ReplaceTarget::AlternateName {
                            alternate_id: alternate_id.clone(),
                            field,
                        },
                    ),
                    None => (
                        format!("person:{}:{}", source.person_id, field.key()),
                        ReplaceTarget::Person {
                            person_id: source.person_id.clone(),
                            field,
                        },
                    ),
                };
                changes.push(ReplaceChange {
                    key,
                    target,
                    owner: owner.clone(),
                    field: field.label_key().to_string(),
                    before: value.clone(),
                    after,
                });
            }
        }
    }

    if request.places {
        for source in places {
            if let Some(after) = replace_text(
                &source.name,
                &request.find,
                &request.replace,
                request.match_case,
            ) {
                changes.push(ReplaceChange {
                    key: format!("place:{}", source.id),
                    target: ReplaceTarget::Place {
                        place_id: source.id.clone(),
                    },
                    owner: if source.name.trim().is_empty() {
                        "(unnamed place)".to_string()
                    } else {
                        source.name.clone()
                    },
                    field: "replace.field_place_name".to_string(),
                    before: source.name.clone(),
                    after,
                });
            }
        }
    }

    if request.notes {
        for source in notes {
            if let Some(after) = replace_text(
                &source.body,
                &request.find,
                &request.replace,
                request.match_case,
            ) {
                changes.push(ReplaceChange {
                    key: format!("note:{}", source.id),
                    target: ReplaceTarget::Note {
                        note_id: source.id.clone(),
                    },
                    owner: source.owner.clone(),
                    field: "replace.field_note".to_string(),
                    before: source.body.clone(),
                    after,
                });
            }
        }
    }

    if request.filenames {
        for source in media {
            if let Some(after) = replace_text(
                &source.file_path,
                &request.find,
                &request.replace,
                request.match_case,
            ) {
                changes.push(ReplaceChange {
                    key: format!("media:{}", source.id),
                    target: ReplaceTarget::MediaPath {
                        media_id: source.id.clone(),
                    },
                    owner: source.owner.clone(),
                    field: "replace.field_media_path".to_string(),
                    before: source.file_path.clone(),
                    after,
                });
            }
        }
    }

    changes.sort_by(|left, right| {
        left.kind()
            .cmp(&right.kind())
            .then_with(|| left.owner.cmp(&right.owner))
            .then_with(|| left.key.cmp(&right.key))
    });
    ReplacePlan { changes }
}

fn display_owner(parts: &NameParts) -> String {
    let name = parts.display_name();
    if name.trim().is_empty() {
        "(unnamed)".to_string()
    } else {
        name
    }
}

/// Replaces every occurrence of `find`, optionally ignoring case.
pub fn replace_text(
    haystack: &str,
    find: &str,
    replacement: &str,
    match_case: bool,
) -> Option<String> {
    if find.is_empty() {
        return None;
    }
    if match_case && !haystack.contains(find) {
        return None;
    }
    let after = if match_case {
        haystack.replace(find, replacement)
    } else {
        replace_ignore_case(haystack, find, replacement)
    };
    (after != haystack).then_some(after)
}

fn replace_ignore_case(haystack: &str, find: &str, replacement: &str) -> String {
    let lower_find = find.to_lowercase();
    if lower_find.is_empty() {
        return haystack.to_string();
    }
    let mut lower = String::with_capacity(haystack.len());
    let mut bounds = Vec::new();
    for (start, character) in haystack.char_indices() {
        let end = start + character.len_utf8();
        let lower_start = lower.len();
        for lower_character in character.to_lowercase() {
            lower.push(lower_character);
        }
        if lower.len() > lower_start {
            bounds.push((lower_start, lower.len(), start, end));
        }
    }

    let mut result = String::with_capacity(haystack.len());
    let mut last = 0;
    let mut search_from = 0;
    let mut matched = false;
    while let Some(offset) = lower[search_from..].find(&lower_find) {
        let start = search_from + offset;
        let end = start + lower_find.len();
        let Some(original_start) = bounds
            .iter()
            .find(|(lower_start, lower_end, _, _)| *lower_start <= start && start < *lower_end)
            .map(|(_, _, original_start, _)| *original_start)
        else {
            break;
        };
        let Some(original_end) = bounds
            .iter()
            .rev()
            .find(|(lower_start, lower_end, _, _)| *lower_start < end && end <= *lower_end)
            .map(|(_, _, _, original_end)| *original_end)
        else {
            break;
        };
        if !haystack.is_char_boundary(original_start) || !haystack.is_char_boundary(original_end) {
            break;
        }
        result.push_str(&haystack[last..original_start]);
        result.push_str(replacement);
        last = original_end;
        search_from = end;
        matched = true;
    }
    if !matched {
        return haystack.to_string();
    }
    result.push_str(&haystack[last..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(given: &str, surname: &str) -> NameParts {
        NameParts {
            given_name: given.to_string(),
            surname: surname.to_string(),
            ..NameParts::default()
        }
    }

    fn request(find: &str, replace: &str) -> ReplaceRequest {
        ReplaceRequest {
            find: find.to_string(),
            replace: replace.to_string(),
            match_case: true,
            names: true,
            places: true,
            notes: true,
            filenames: true,
        }
    }

    #[test]
    fn replaces_literally_with_optional_case_folding() {
        assert_eq!(
            replace_text("Ada and ada", "ada", "Grace", true).as_deref(),
            Some("Ada and Grace")
        );
        assert_eq!(
            replace_text("Ada and ADA", "ada", "Grace", false).as_deref(),
            Some("Grace and Grace")
        );
        assert_eq!(replace_text("Ada", "missing", "Grace", false), None);
        assert_eq!(
            replace_text("Ada King", "Ada ", "", true).as_deref(),
            Some("King")
        );
        assert_eq!(
            replace_text("Москва", "МОСКВА", "Moscow", false).as_deref(),
            Some("Moscow")
        );
    }

    #[test]
    fn plan_respects_scopes_and_builds_field_keys() {
        let request = request("Ann", "Jane");
        let plan = build_plan(
            &request,
            &[
                NameSource {
                    person_id: "p1".to_string(),
                    alternate_id: None,
                    owner: "Ann King".to_string(),
                    parts: parts("Ann", "King"),
                },
                NameSource {
                    person_id: "p1".to_string(),
                    alternate_id: Some("a1".to_string()),
                    owner: "Annie King".to_string(),
                    parts: parts("Annie", "King"),
                },
            ],
            &[PlaceSource {
                id: "pl1".to_string(),
                name: "Ann Arbor".to_string(),
            }],
            &[NoteSource {
                id: "n1".to_string(),
                body: "Ann visited".to_string(),
                owner: "Ann King".to_string(),
            }],
            &[MediaSource {
                id: "m1".to_string(),
                file_path: "/photos/Ann.jpg".to_string(),
                owner: "Ann portrait".to_string(),
            }],
        );
        assert!(
            plan.changes
                .iter()
                .any(|change| change.key == "person:p1:given")
        );
        assert!(
            plan.changes
                .iter()
                .any(|change| change.key == "alternate:a1:given")
        );
        assert!(plan.changes.iter().any(|change| change.key == "place:pl1"));
        assert!(plan.changes.iter().any(|change| change.key == "note:n1"));
        assert!(plan.changes.iter().any(|change| change.key == "media:m1"));

        let places_only = build_plan(
            &ReplaceRequest {
                places: true,
                names: false,
                notes: false,
                filenames: false,
                ..request.clone()
            },
            &[],
            &[PlaceSource {
                id: "pl1".to_string(),
                name: "Ann Arbor".to_string(),
            }],
            &[],
            &[],
        );
        assert_eq!(places_only.changes.len(), 1);
        assert_eq!(places_only.changes[0].kind(), ReplaceKind::Place);
    }

    #[test]
    fn no_matches_or_empty_find_produces_an_empty_plan() {
        assert!(
            build_plan(
                &request("missing", "x"),
                &[NameSource {
                    person_id: "p1".to_string(),
                    alternate_id: None,
                    owner: "Ada King".to_string(),
                    parts: parts("Ada", "King"),
                }],
                &[],
                &[],
                &[],
            )
            .is_empty()
        );
        assert!(build_plan(&request("", "x"), &[], &[], &[], &[]).is_empty());
        assert!(
            build_plan(
                &ReplaceRequest {
                    find: "Ada".to_string(),
                    replace: "Grace".to_string(),
                    match_case: true,
                    names: false,
                    places: false,
                    notes: false,
                    filenames: false,
                },
                &[],
                &[],
                &[],
                &[],
            )
            .is_empty()
        );
    }
}
