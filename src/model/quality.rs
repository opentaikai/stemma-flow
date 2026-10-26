//! Evidence Explained citation quality values and GEDCOM certainty.
//!
//! Quality is assessed per citation use, so every value is optional: `None`
//! represents "Don't know" and is stored as NULL.

/// Quality of the source: original, derivative or authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceQuality {
    Original,
    Derivative,
    Authored,
}

impl SourceQuality {
    pub const ALL: [Self; 3] = [Self::Original, Self::Derivative, Self::Authored];

    pub fn key(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Derivative => "derivative",
            Self::Authored => "authored",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Derivative => "Derivative",
            Self::Authored => "Authored",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "original" => Some(Self::Original),
            "derivative" => Some(Self::Derivative),
            "authored" => Some(Self::Authored),
            _ => None,
        }
    }
}

/// Quality of the information: primary, secondary or undetermined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InformationQuality {
    Primary,
    Secondary,
    Undetermined,
}

impl InformationQuality {
    pub const ALL: [Self; 3] = [Self::Primary, Self::Secondary, Self::Undetermined];

    pub fn key(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Undetermined => "undetermined",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Secondary => "Secondary",
            Self::Undetermined => "Undetermined",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "primary" => Some(Self::Primary),
            "secondary" => Some(Self::Secondary),
            "undetermined" => Some(Self::Undetermined),
            _ => None,
        }
    }
}

/// Quality of the evidence: direct, indirect or negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceQuality {
    Direct,
    Indirect,
    Negative,
}

impl EvidenceQuality {
    pub const ALL: [Self; 3] = [Self::Direct, Self::Indirect, Self::Negative];

    pub fn key(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Indirect => "indirect",
            Self::Negative => "negative",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Direct => "Direct",
            Self::Indirect => "Indirect",
            Self::Negative => "Negative",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "direct" => Some(Self::Direct),
            "indirect" => Some(Self::Indirect),
            "negative" => Some(Self::Negative),
            _ => None,
        }
    }
}

/// GEDCOM `QUAY` certainty assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Certainty {
    Unreliable,
    Questionable,
    Secondary,
    Primary,
}

impl Certainty {
    pub const ALL: [Self; 4] = [
        Self::Unreliable,
        Self::Questionable,
        Self::Secondary,
        Self::Primary,
    ];

    pub fn value(self) -> &'static str {
        match self {
            Self::Unreliable => "0",
            Self::Questionable => "1",
            Self::Secondary => "2",
            Self::Primary => "3",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unreliable => "Unreliable",
            Self::Questionable => "Questionable",
            Self::Secondary => "Secondary",
            Self::Primary => "Primary",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Unreliable => "Unreliable evidence or estimated data",
            Self::Questionable => "Questionable reliability of evidence",
            Self::Secondary => "Secondary evidence, recorded sometime after the event",
            Self::Primary => "Direct and primary evidence, or dominance of the evidence",
        }
    }

    pub fn from_value(value: &str) -> Option<Self> {
        match value.trim() {
            "0" => Some(Self::Unreliable),
            "1" => Some(Self::Questionable),
            "2" => Some(Self::Secondary),
            "3" => Some(Self::Primary),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_quality_round_trips() {
        for value in SourceQuality::ALL {
            assert_eq!(SourceQuality::from_key(value.key()), Some(value));
            assert_eq!(SourceQuality::from_key(value.label()), Some(value));
        }
        assert_eq!(
            SourceQuality::from_key("  ORIGINAL "),
            Some(SourceQuality::Original)
        );
        assert_eq!(SourceQuality::from_key("unknown"), None);
    }

    #[test]
    fn information_quality_round_trips() {
        for value in InformationQuality::ALL {
            assert_eq!(InformationQuality::from_key(value.key()), Some(value));
            assert_eq!(InformationQuality::from_key(value.label()), Some(value));
        }
        assert_eq!(InformationQuality::from_key("perhaps"), None);
    }

    #[test]
    fn evidence_quality_round_trips() {
        for value in EvidenceQuality::ALL {
            assert_eq!(EvidenceQuality::from_key(value.key()), Some(value));
            assert_eq!(EvidenceQuality::from_key(value.label()), Some(value));
        }
        assert_eq!(EvidenceQuality::from_key("circumstantial"), None);
    }

    #[test]
    fn certainty_round_trips() {
        for value in Certainty::ALL {
            assert_eq!(Certainty::from_value(value.value()), Some(value));
            assert!(!value.label().is_empty());
            assert!(!value.description().is_empty());
        }
        assert_eq!(Certainty::from_value("4"), None);
        assert_eq!(Certainty::from_value("U"), None);
    }
}
