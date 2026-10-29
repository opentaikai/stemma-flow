//! Configurable columns for the People list.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// One selectable data column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PeopleColumn {
    Prefix,
    GivenName,
    MiddleName,
    Surname,
    Suffix,
    Nickname,
    Gender,
    BirthDate,
    BirthPlace,
    DeathDate,
    DeathPlace,
    AgeAtDeath,
    Notes,
    Events,
    Citations,
    Media,
    Tasks,
    Groups,
    Associations,
    UpdatedAt,
}

impl PeopleColumn {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Prefix => "people.column_prefix",
            Self::GivenName => "people.column_given",
            Self::MiddleName => "people.column_middle",
            Self::Surname => "people.column_surname",
            Self::Suffix => "people.column_suffix",
            Self::Nickname => "people.column_nickname",
            Self::Gender => "people.column_gender",
            Self::BirthDate => "people.column_birth_date",
            Self::BirthPlace => "people.column_birth_place",
            Self::DeathDate => "people.column_death_date",
            Self::DeathPlace => "people.column_death_place",
            Self::AgeAtDeath => "people.column_age",
            Self::Notes => "people.column_notes",
            Self::Events => "people.column_events",
            Self::Citations => "people.column_citations",
            Self::Media => "people.column_media",
            Self::Tasks => "people.column_tasks",
            Self::Groups => "people.column_groups",
            Self::Associations => "people.column_associations",
            Self::UpdatedAt => "people.column_updated",
        }
    }

    pub fn default_width(self) -> f32 {
        match self {
            Self::GivenName | Self::Surname | Self::BirthPlace | Self::DeathPlace => 130.0,
            Self::MiddleName | Self::Nickname | Self::Prefix | Self::Suffix => 100.0,
            Self::Notes => 220.0,
            Self::UpdatedAt => 150.0,
            _ => 90.0,
        }
    }
}

/// Visible/order/width configuration for one column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeopleColumnConfig {
    pub column: PeopleColumn,
    pub visible: bool,
    pub width: f32,
}

/// The fixed legacy People layout.
pub fn default_columns() -> Vec<PeopleColumnConfig> {
    all_columns()
        .into_iter()
        .map(|column| PeopleColumnConfig {
            column,
            visible: matches!(
                column,
                PeopleColumn::GivenName
                    | PeopleColumn::Surname
                    | PeopleColumn::Gender
                    | PeopleColumn::BirthDate
                    | PeopleColumn::DeathDate
            ),
            width: column.default_width(),
        })
        .collect()
}

/// Every selectable column in default order.
pub fn all_columns() -> Vec<PeopleColumn> {
    vec![
        PeopleColumn::Prefix,
        PeopleColumn::GivenName,
        PeopleColumn::MiddleName,
        PeopleColumn::Surname,
        PeopleColumn::Suffix,
        PeopleColumn::Nickname,
        PeopleColumn::Gender,
        PeopleColumn::BirthDate,
        PeopleColumn::BirthPlace,
        PeopleColumn::DeathDate,
        PeopleColumn::DeathPlace,
        PeopleColumn::AgeAtDeath,
        PeopleColumn::Notes,
        PeopleColumn::Events,
        PeopleColumn::Citations,
        PeopleColumn::Media,
        PeopleColumn::Tasks,
        PeopleColumn::Groups,
        PeopleColumn::Associations,
        PeopleColumn::UpdatedAt,
    ]
}

/// De-duplicates columns, clamps widths and fills in missing columns.
pub fn sanitize_columns(columns: &[PeopleColumnConfig]) -> Vec<PeopleColumnConfig> {
    let mut seen = HashSet::new();
    let mut cleaned = Vec::new();
    for config in columns {
        if seen.insert(config.column) {
            cleaned.push(PeopleColumnConfig {
                column: config.column,
                visible: config.visible,
                width: config.width.clamp(60.0, 500.0),
            });
        }
    }
    for column in all_columns() {
        if !seen.contains(&column) {
            cleaned.push(PeopleColumnConfig {
                column,
                visible: false,
                width: column.default_width(),
            });
        }
    }
    if cleaned.is_empty() {
        default_columns()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_legacy_watch_list() {
        let columns = default_columns();
        let visible: Vec<PeopleColumn> = columns
            .iter()
            .filter(|config| config.visible)
            .map(|config| config.column)
            .collect();
        assert_eq!(
            visible,
            vec![
                PeopleColumn::GivenName,
                PeopleColumn::Surname,
                PeopleColumn::Gender,
                PeopleColumn::BirthDate,
                PeopleColumn::DeathDate,
            ]
        );
        assert_eq!(columns.len(), all_columns().len());
    }

    #[test]
    fn sanitize_removes_duplicates_clamps_widths_and_fills_missing() {
        let columns = vec![
            PeopleColumnConfig {
                column: PeopleColumn::GivenName,
                visible: true,
                width: 20.0,
            },
            PeopleColumnConfig {
                column: PeopleColumn::GivenName,
                visible: true,
                width: 200.0,
            },
        ];
        let cleaned = sanitize_columns(&columns);
        assert_eq!(cleaned.len(), all_columns().len());
        assert_eq!(cleaned[0].width, 60.0);
        assert_eq!(
            cleaned
                .iter()
                .filter(|config| config.column == PeopleColumn::GivenName)
                .count(),
            1
        );
        assert!(
            cleaned
                .iter()
                .find(|config| config.column == PeopleColumn::Surname)
                .is_some_and(|config| !config.visible)
        );
    }

    #[test]
    fn every_column_has_a_key_and_width() {
        for column in all_columns() {
            assert!(!column.label_key().is_empty());
            assert!(column.default_width() >= 60.0);
        }
    }
}
