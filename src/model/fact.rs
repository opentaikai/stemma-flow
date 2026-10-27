//! Built-in fact types and their GEDCOM mappings.

/// Where a fact belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactScope {
    Person,
    Family,
    Both,
}

impl FactScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Family => "family",
            Self::Both => "both",
        }
    }
}

/// Whether a fact is an event or an attribute with a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactKind {
    Event,
    Attribute,
}

impl FactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Attribute => "attribute",
        }
    }
}

/// One built-in fact type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactType {
    pub key: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub gedcom_tag: &'static str,
    pub scope: FactScope,
    pub kind: FactKind,
}

const fn fact(
    key: &'static str,
    label: &'static str,
    category: &'static str,
    gedcom_tag: &'static str,
    scope: FactScope,
    kind: FactKind,
) -> FactType {
    FactType {
        key,
        label,
        category,
        gedcom_tag,
        scope,
        kind,
    }
}

/// The built-in catalogue. Shared GEDCOM tags (`CENS`, `RESI`, `EVEN`) use
/// one `Both` fact instead of duplicate person/family rows.
pub static FACTS: [FactType; 61] = [
    // Person events.
    fact(
        "ADOPTION",
        "Adoption",
        "Vital",
        "ADOP",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "BAPTISM",
        "Baptism",
        "Baptism",
        "BAPM",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "BAR_MITZVAH",
        "Bar Mitzvah",
        "Baptism",
        "BARM",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "BAS_MITZVAH",
        "Bas Mitzvah",
        "Baptism",
        "BASM",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "BIRTH",
        "Birth",
        "Vital",
        "BIRT",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "BLESSING",
        "Blessing",
        "Baptism",
        "BLES",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "BURIAL",
        "Burial",
        "Vital",
        "BURI",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "CENSUS",
        "Census",
        "Census",
        "CENS",
        FactScope::Both,
        FactKind::Event,
    ),
    fact(
        "CHRISTENING",
        "Christening",
        "Baptism",
        "CHR",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "ADULT_CHRISTENING",
        "Adult Christening",
        "Baptism",
        "CHRA",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "CONFIRMATION",
        "Confirmation",
        "Baptism",
        "CONF",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "CREMATION",
        "Cremation",
        "Vital",
        "CREM",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "DEATH",
        "Death",
        "Vital",
        "DEAT",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "EMIGRATION",
        "Emigration",
        "Migration",
        "EMIG",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "FIRST_COMMUNION",
        "First Communion",
        "Baptism",
        "FCOM",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "GRADUATION",
        "Graduation",
        "Education",
        "GRAD",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "IMMIGRATION",
        "Immigration",
        "Migration",
        "IMMI",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "NATURALIZATION",
        "Naturalization",
        "Migration",
        "NATU",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "ORDINATION",
        "Ordination",
        "Religion",
        "ORDN",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "PROBATE",
        "Probate",
        "Legal",
        "PROB",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "RETIREMENT",
        "Retirement",
        "Occupation",
        "RETI",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "WILL",
        "Will",
        "Legal",
        "WILL",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "EVENT",
        "Event",
        "Other",
        "EVEN",
        FactScope::Both,
        FactKind::Event,
    ),
    // Person attributes.
    fact(
        "CASTE",
        "Caste",
        "Other",
        "CAST",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "DESCRIPTION",
        "Physical Description",
        "Other",
        "DSCR",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "EDUCATION",
        "Education",
        "Education",
        "EDUC",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "ID_NUMBER",
        "ID Number",
        "Other",
        "IDNO",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "NATIONALITY",
        "Nationality",
        "Other",
        "NATI",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "CHILDREN_COUNT",
        "Number of Children",
        "Other",
        "NCHI",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "MARRIAGES_COUNT",
        "Number of Marriages",
        "Other",
        "NMR",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "OCCUPATION",
        "Occupation",
        "Occupation",
        "OCCU",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "PROPERTY",
        "Property",
        "Legal",
        "PROP",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "RELIGION",
        "Religion",
        "Religion",
        "RELI",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "RESIDENCE",
        "Residence",
        "Residence",
        "RESI",
        FactScope::Both,
        FactKind::Attribute,
    ),
    fact(
        "SSN",
        "Social Security Number",
        "Other",
        "SSN",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "TITLE",
        "Title",
        "Other",
        "TITL",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "FACT",
        "Fact",
        "Other",
        "FACT",
        FactScope::Person,
        FactKind::Attribute,
    ),
    // Family events.
    fact(
        "ANNULMENT",
        "Annulment",
        "Family",
        "ANUL",
        FactScope::Family,
        FactKind::Event,
    ),
    fact(
        "DIVORCE",
        "Divorce",
        "Family",
        "DIV",
        FactScope::Family,
        FactKind::Event,
    ),
    fact(
        "DIVORCE_FILED",
        "Divorce Filed",
        "Family",
        "DIVF",
        FactScope::Family,
        FactKind::Event,
    ),
    fact(
        "ENGAGEMENT",
        "Engagement",
        "Family",
        "ENGA",
        FactScope::Family,
        FactKind::Event,
    ),
    fact(
        "MARRIAGE_BANNS",
        "Marriage Banns",
        "Family",
        "MARB",
        FactScope::Family,
        FactKind::Event,
    ),
    fact(
        "MARRIAGE_CONTRACT",
        "Marriage Contract",
        "Family",
        "MARC",
        FactScope::Family,
        FactKind::Event,
    ),
    fact(
        "MARRIAGE",
        "Marriage",
        "Family",
        "MARR",
        FactScope::Family,
        FactKind::Event,
    ),
    fact(
        "MARRIAGE_LICENSE",
        "Marriage License",
        "Family",
        "MARL",
        FactScope::Family,
        FactKind::Event,
    ),
    fact(
        "MARRIAGE_SETTLEMENT",
        "Marriage Settlement",
        "Family",
        "MARS",
        FactScope::Family,
        FactKind::Event,
    ),
    // LDS ordinances.
    fact(
        "BAPTISM_LDS",
        "LDS Baptism",
        "LDS",
        "BAPL",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "CONFIRMATION_LDS",
        "LDS Confirmation",
        "LDS",
        "CONL",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "ENDOWMENT_LDS",
        "LDS Endowment",
        "LDS",
        "ENDL",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "SEALING_CHILD",
        "LDS Sealing to Parents",
        "LDS",
        "SLGC",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "SEALING_SPOUSE",
        "LDS Sealing to Spouse",
        "LDS",
        "SLGS",
        FactScope::Family,
        FactKind::Event,
    ),
    // App-specific facts.
    fact(
        "MILITARY",
        "Military Service",
        "Military",
        "_MILI",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "MEDICAL",
        "Medical",
        "Medical",
        "_MEDI",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "HOSPITAL",
        "Hospital",
        "Medical",
        "_HOSP",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "ILLNESS",
        "Illness",
        "Medical",
        "_ILL",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "EMPLOYMENT",
        "Employment",
        "Occupation",
        "_EMPL",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "AWARD",
        "Award",
        "Other",
        "_AWARD",
        FactScope::Person,
        FactKind::Event,
    ),
    fact(
        "DEGREE",
        "Degree",
        "Education",
        "_DEG",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "MEMBERSHIP",
        "Membership",
        "Other",
        "_MEMB",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "ELECTED_OFFICE",
        "Elected Office",
        "Legal",
        "_ELECT",
        FactScope::Person,
        FactKind::Attribute,
    ),
    fact(
        "MIGRATION",
        "Migration",
        "Migration",
        "_MIGR",
        FactScope::Person,
        FactKind::Event,
    ),
];

/// The built-in catalogue in declaration order.
pub fn builtin_facts() -> &'static [FactType] {
    &FACTS
}

/// Looks up a fact by its database key.
pub fn by_key(key: &str) -> Option<&'static FactType> {
    FACTS.iter().find(|fact| fact.key == key)
}

/// Looks up a fact by GEDCOM tag for a person or family record.
pub fn by_tag(tag: &str, scope: FactScope) -> Option<&'static FactType> {
    FACTS.iter().find(|fact| {
        fact.gedcom_tag == tag
            && match scope {
                FactScope::Person => fact.scope != FactScope::Family,
                FactScope::Family => fact.scope != FactScope::Person,
                FactScope::Both => true,
            }
    })
}

/// Display label for a stored fact key, falling back to the key.
pub fn label(key: &str) -> &str {
    by_key(key).map(|fact| fact.label).unwrap_or(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalogue_has_sixty_one_unique_facts() {
        assert_eq!(FACTS.len(), 61);
        let keys: HashSet<_> = FACTS.iter().map(|fact| fact.key).collect();
        assert_eq!(keys.len(), FACTS.len(), "fact keys are unique");
        let tags: HashSet<_> = FACTS.iter().map(|fact| fact.gedcom_tag).collect();
        assert_eq!(tags.len(), FACTS.len(), "GEDCOM tags are unique");
    }

    #[test]
    fn looks_up_keys_and_scoped_tags() {
        let birth = by_key("BIRTH").expect("birth");
        assert_eq!(birth.gedcom_tag, "BIRT");
        assert_eq!(
            by_tag("BIRT", FactScope::Person).map(|f| f.key),
            Some("BIRTH")
        );
        assert!(by_tag("BIRT", FactScope::Family).is_none());

        assert_eq!(
            by_tag("MARR", FactScope::Family).map(|f| f.key),
            Some("MARRIAGE")
        );
        assert!(by_tag("MARR", FactScope::Person).is_none());

        assert_eq!(
            by_tag("CENS", FactScope::Person).map(|f| f.key),
            Some("CENSUS")
        );
        assert_eq!(
            by_tag("CENS", FactScope::Family).map(|f| f.key),
            Some("CENSUS")
        );
        assert_eq!(label("OCCUPATION"), "Occupation");
        assert_eq!(label("UNKNOWN"), "UNKNOWN");
    }
}
