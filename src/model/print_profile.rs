//! Per-report print customization profiles.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Every printable report that can have its own print profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ReportKind {
    GroupRoster,
    ResearchLog,
    PhotoTree,
    Scrapbook,
    PedigreeChart,
    FamilyGroupSheet,
    IndividualSummary,
    IndividualList,
    AhnentafelList,
    DescendantList,
    KinshipReport,
    RelationshipReport,
    CalendarReports,
    TimelineReport,
    AdministrativeLists,
    CleanupReports,
    FanChart,
    HourglassChart,
    WallChart,
    FamilyHistoryBook,
}

impl ReportKind {
    pub const ALL: [Self; 20] = [
        Self::GroupRoster,
        Self::ResearchLog,
        Self::PhotoTree,
        Self::Scrapbook,
        Self::PedigreeChart,
        Self::FamilyGroupSheet,
        Self::IndividualSummary,
        Self::IndividualList,
        Self::AhnentafelList,
        Self::DescendantList,
        Self::KinshipReport,
        Self::RelationshipReport,
        Self::CalendarReports,
        Self::TimelineReport,
        Self::AdministrativeLists,
        Self::CleanupReports,
        Self::FanChart,
        Self::HourglassChart,
        Self::WallChart,
        Self::FamilyHistoryBook,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::GroupRoster => "Group Roster",
            Self::ResearchLog => "Research Log",
            Self::PhotoTree => "Photo Tree",
            Self::Scrapbook => "Scrapbook",
            Self::PedigreeChart => "Pedigree Chart",
            Self::FamilyGroupSheet => "Family Group Sheet",
            Self::IndividualSummary => "Individual Summary",
            Self::IndividualList => "Individual List",
            Self::AhnentafelList => "Ahnentafel List",
            Self::DescendantList => "Descendant List",
            Self::KinshipReport => "Kinship Report",
            Self::RelationshipReport => "Relationship Report",
            Self::CalendarReports => "Calendar Reports",
            Self::TimelineReport => "Timeline Report",
            Self::AdministrativeLists => "Lists & Statistics",
            Self::CleanupReports => "Cleanup Reports",
            Self::FanChart => "Fan Chart",
            Self::HourglassChart => "Hourglass & Bow-tie",
            Self::WallChart => "Wall Charts",
            Self::FamilyHistoryBook => "Family History Book",
        }
    }
}

/// Font family offered for printed reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrintFont {
    /// Keep the report's designed font.
    #[default]
    Default,
    SansSerif,
    Serif,
    Monospace,
}

impl PrintFont {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Report default",
            Self::SansSerif => "Sans-serif",
            Self::Serif => "Serif",
            Self::Monospace => "Monospace",
        }
    }

    pub fn css_family(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::SansSerif => Some("system-ui, -apple-system, \"Segoe UI\", sans-serif"),
            Self::Serif => Some("Georgia, \"Times New Roman\", serif"),
            Self::Monospace => Some("\"SFMono-Regular\", Consolas, \"Liberation Mono\", monospace"),
        }
    }
}

/// One report's print customization.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrintProfile {
    /// Supports `{report}`, `{tree}` and `{date}` placeholders.
    pub header: String,
    /// Supports `{report}`, `{tree}` and `{date}` placeholders.
    pub footer: String,
    /// Overrides the report's page margin when set.
    pub margin_mm: Option<u32>,
    pub font: PrintFont,
    /// Base body text size in points.
    pub font_size_pt: Option<f32>,
}

impl PrintProfile {
    pub fn is_default(&self) -> bool {
        self.header.trim().is_empty()
            && self.footer.trim().is_empty()
            && self.margin_mm.is_none()
            && self.font == PrintFont::Default
            && self.font_size_pt.is_none()
    }

    pub fn sanitize(&mut self) {
        self.header = clean_text(&self.header);
        self.footer = clean_text(&self.footer);
        self.margin_mm = self.margin_mm.map(|margin| margin.clamp(8, 40));
        self.font_size_pt = self
            .font_size_pt
            .filter(|size| size.is_finite())
            .map(|size| size.clamp(6.0, 24.0));
    }
}

fn clean_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= 200 {
        trimmed.to_string()
    } else {
        trimmed.chars().take(200).collect()
    }
}

/// All profiles, keyed by report type.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrintProfiles {
    pub reports: BTreeMap<ReportKind, PrintProfile>,
}

impl PrintProfiles {
    pub fn get(&self, kind: ReportKind) -> Option<&PrintProfile> {
        self.reports.get(&kind)
    }

    /// Stores a sanitized profile, removing it when it is effectively empty.
    pub fn set(&mut self, kind: ReportKind, mut profile: PrintProfile) {
        profile.sanitize();
        if profile.is_default() {
            self.reports.remove(&kind);
        } else {
            self.reports.insert(kind, profile);
        }
    }

    pub fn reset(&mut self, kind: ReportKind) {
        self.reports.remove(&kind);
    }

    pub fn sanitize(&mut self) {
        for profile in self.reports.values_mut() {
            profile.sanitize();
        }
        self.reports.retain(|_, profile| !profile.is_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> PrintProfile {
        PrintProfile {
            header: "  Family report  ".to_string(),
            footer: "{report} · {tree}".to_string(),
            margin_mm: Some(12),
            font: PrintFont::Serif,
            font_size_pt: Some(10.5),
        }
    }

    #[test]
    fn profiles_trim_and_clamp_their_values() {
        let mut profile = PrintProfile {
            header: "  padded  ".to_string(),
            margin_mm: Some(2),
            font_size_pt: Some(40.0),
            ..PrintProfile::default()
        };
        profile.sanitize();
        assert_eq!(profile.header, "padded");
        assert_eq!(profile.margin_mm, Some(8));
        assert_eq!(profile.font_size_pt, Some(24.0));
        assert!(!profile.is_default());

        let mut empty = PrintProfile::default();
        empty.sanitize();
        assert!(empty.is_default());
    }

    #[test]
    fn profiles_store_only_non_default_values() {
        let mut profiles = PrintProfiles::default();
        profiles.set(ReportKind::PedigreeChart, profile());
        assert_eq!(
            profiles
                .get(ReportKind::PedigreeChart)
                .map(|profile| profile.header.as_str()),
            Some("Family report")
        );
        profiles.reset(ReportKind::PedigreeChart);
        assert!(profiles.get(ReportKind::PedigreeChart).is_none());

        profiles.set(ReportKind::FanChart, PrintProfile::default());
        assert!(profiles.get(ReportKind::FanChart).is_none());
    }

    #[test]
    fn profiles_round_trip_through_toml() {
        let mut profiles = PrintProfiles::default();
        profiles.set(ReportKind::FamilyGroupSheet, profile());
        let text = toml::to_string(&profiles).expect("serialize");
        let parsed: PrintProfiles = toml::from_str(&text).expect("deserialize");
        assert_eq!(parsed, profiles);
    }

    #[test]
    fn font_families_expose_css_stacks() {
        assert_eq!(PrintFont::Default.css_family(), None);
        assert!(
            PrintFont::Serif
                .css_family()
                .is_some_and(|css| css.contains("serif"))
        );
        assert_eq!(PrintFont::Monospace.label(), "Monospace");
        assert_eq!(ReportKind::ALL.len(), 20);
    }
}
