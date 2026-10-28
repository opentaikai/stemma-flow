//! Deriving a person's relatives and their display labels.

use std::collections::HashMap;

use crate::db::{FamilyTree, MarriageInfo, Person};

/// One relative listed in the inspector; `id` drives click-to-focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relative {
    pub id: String,
    pub label: String,
    /// Marriage date and place, set on spouse rows when known.
    pub marriage: Option<MarriageInfo>,
}

/// The selected person's existing family ties, ready to render.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Connections {
    pub parents: Vec<Relative>,
    pub siblings: Vec<Relative>,
    pub spouses: Vec<Relative>,
    pub children: Vec<Relative>,
}

/// Lists parents, siblings, spouses and children straight off the loaded
/// tree, sorted by display label. Siblings cover every family that
/// contains one of the parents, so remarriage half-siblings appear too.
/// `marriages` supplies the details shown next to spouse names.
pub fn connections(
    tree: &FamilyTree,
    person_id: &str,
    marriages: &HashMap<String, MarriageInfo>,
    surname_first: bool,
) -> Connections {
    let mut parent_ids: Vec<String> = Vec::new();
    for (family_id, child_id) in &tree.child_links {
        if child_id != person_id {
            continue;
        }
        let Some(family) = tree.families.iter().find(|family| &family.id == family_id) else {
            continue;
        };
        for slot in [&family.husband_id, &family.wife_id] {
            if let Some(parent_id) = slot
                && parent_id != person_id
                && !parent_ids.contains(parent_id)
            {
                parent_ids.push(parent_id.clone());
            }
        }
    }

    let mut sibling_ids: Vec<String> = Vec::new();
    for parent_id in &parent_ids {
        let their_families = tree.families.iter().filter(|family| {
            family.husband_id.as_ref() == Some(parent_id)
                || family.wife_id.as_ref() == Some(parent_id)
        });
        for family in their_families {
            for (family_id, child_id) in &tree.child_links {
                if family_id == &family.id
                    && child_id != person_id
                    && !sibling_ids.contains(child_id)
                {
                    sibling_ids.push(child_id.clone());
                }
            }
        }
    }

    let mut spouse_ids: Vec<(String, Option<MarriageInfo>)> = Vec::new();
    let mut child_ids: Vec<String> = Vec::new();
    for family in &tree.families {
        let is_spouse = family.husband_id.as_deref() == Some(person_id)
            || family.wife_id.as_deref() == Some(person_id);
        if !is_spouse {
            continue;
        }
        let other = if family.husband_id.as_deref() == Some(person_id) {
            family.wife_id.as_deref()
        } else {
            family.husband_id.as_deref()
        };
        if let Some(spouse_id) = other
            && spouse_id != person_id
            && !spouse_ids.iter().any(|(id, _)| id == spouse_id)
        {
            spouse_ids.push((spouse_id.to_string(), marriages.get(&family.id).cloned()));
        }
        for (family_id, child_id) in &tree.child_links {
            if family_id == &family.id && child_id != person_id && !child_ids.contains(child_id) {
                child_ids.push(child_id.clone());
            }
        }
    }

    Connections {
        parents: by_label(relatives(tree, parent_ids, surname_first)),
        siblings: by_label(relatives(tree, sibling_ids, surname_first)),
        spouses: by_label(
            spouse_ids
                .into_iter()
                .filter_map(|(id, marriage)| relative(tree, &id, marriage, surname_first))
                .collect(),
        ),
        children: by_label(relatives(tree, child_ids, surname_first)),
    }
}

/// Builds a labelled relative, skipping ids with no stored person.
fn relative(
    tree: &FamilyTree,
    id: &str,
    marriage: Option<MarriageInfo>,
    surname_first: bool,
) -> Option<Relative> {
    let person = tree.people.iter().find(|person| person.id == id)?;
    Some(Relative {
        id: person.id.clone(),
        label: person_label(person, surname_first),
        marriage,
    })
}

fn relatives(tree: &FamilyTree, ids: Vec<String>, surname_first: bool) -> Vec<Relative> {
    ids.iter()
        .filter_map(|id| relative(tree, id, None, surname_first))
        .collect()
}

fn by_label(mut relatives: Vec<Relative>) -> Vec<Relative> {
    relatives.sort_by(|left, right| {
        left.label
            .to_lowercase()
            .cmp(&right.label.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    relatives
}

/// Reverses marriage place names for read-only inspector displays.
pub fn reverse_marriage_places(connections: &mut Connections) {
    for relative in connections
        .parents
        .iter_mut()
        .chain(connections.siblings.iter_mut())
        .chain(connections.spouses.iter_mut())
        .chain(connections.children.iter_mut())
    {
        if let Some(marriage) = &mut relative.marriage
            && let Some(place) = marriage.place.as_deref()
        {
            marriage.place = Some(crate::db::places::reverse_name(place));
        }
    }
}

/// Display label for a stored person; blank names fall back gracefully.
pub fn person_label(person: &Person, surname_first: bool) -> String {
    let combined = crate::model::name::format_name(
        &person.name_prefix,
        &person.given_name,
        &person.middle_name,
        &person.surname,
        &person.name_suffix,
        surname_first,
    );
    if combined.is_empty() {
        "(unnamed)".to_string()
    } else {
        combined
    }
}
