//! Coloured group and label badges attached to people.

/// Where a badge came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BadgeKind {
    Group,
    Label,
}

/// One visual badge for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonBadge {
    pub name: String,
    pub color: Option<u32>,
    pub kind: BadgeKind,
}

impl PersonBadge {
    pub fn sort_key(&self) -> (BadgeKind, String) {
        (self.kind, self.name.to_lowercase())
    }
}
