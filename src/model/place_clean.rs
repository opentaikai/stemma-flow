//! Batch standardisation of comma-separated place-name components.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::db::places::PlaceSummary;
use crate::model::dataclean::{
    DataCleanPlan, PlaceCleanup, normalize_name_text, normalize_place_name,
};

/// One observed spelling of a place component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceComponentVariant {
    pub text: String,
    pub count: usize,
}

/// A repeated place component with its editable standard spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceComponent {
    pub key: String,
    pub variants: Vec<PlaceComponentVariant>,
    pub after: String,
    pub place_count: usize,
}

/// Collects every comma-separated component used by the place list.
pub fn build_components(places: &[PlaceSummary]) -> Vec<PlaceComponent> {
    struct Entry {
        variants: BTreeMap<String, usize>,
        places: HashSet<usize>,
    }

    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    for (index, place) in places.iter().enumerate() {
        for part in place.name.split(',') {
            let text = part.trim();
            if text.is_empty() {
                continue;
            }
            let key = normalize_name_text(text).to_lowercase();
            if key.is_empty() {
                continue;
            }
            let entry = entries.entry(key).or_insert_with(|| Entry {
                variants: BTreeMap::new(),
                places: HashSet::new(),
            });
            *entry.variants.entry(text.to_string()).or_insert(0) += 1;
            entry.places.insert(index);
        }
    }

    let mut components: Vec<PlaceComponent> = entries
        .into_iter()
        .map(|(key, entry)| {
            let mut variants: Vec<PlaceComponentVariant> = entry
                .variants
                .into_iter()
                .map(|(text, count)| PlaceComponentVariant { text, count })
                .collect();
            variants.sort_by(|left, right| {
                right
                    .count
                    .cmp(&left.count)
                    .then_with(|| left.text.cmp(&right.text))
            });
            let after = variants
                .first()
                .map(|variant| normalize_name_text(&variant.text))
                .unwrap_or_default();
            PlaceComponent {
                key,
                variants,
                after,
                place_count: entry.places.len(),
            }
        })
        .collect();
    components.sort_by(|left, right| {
        right
            .place_count
            .cmp(&left.place_count)
            .then_with(|| left.key.cmp(&right.key))
    });
    components
}

/// Builds rename and merge rows from the selected component rules.
pub fn preview_plan(
    places: &[PlaceSummary],
    components: &[PlaceComponent],
    selected: &HashSet<String>,
) -> DataCleanPlan {
    let rules: HashMap<&str, String> = components
        .iter()
        .filter(|component| selected.contains(&component.key))
        .map(|component| {
            (
                component.key.as_str(),
                normalize_name_text(&component.after),
            )
        })
        .filter(|(_, after)| !after.is_empty())
        .collect();
    if rules.is_empty() {
        return DataCleanPlan::default();
    }

    struct Rebuilt<'a> {
        place: &'a PlaceSummary,
        after: String,
        key: String,
    }

    let mut changed_keys = HashSet::new();
    let mut rebuilt = Vec::with_capacity(places.len());
    for place in places {
        let (after, replaced) = rewrite_name(&place.name, &rules);
        let key = normalize_place_name(&after).to_lowercase();
        if replaced && after != place.name {
            changed_keys.insert(key.clone());
        }
        rebuilt.push(Rebuilt { place, after, key });
    }

    let mut groups: BTreeMap<String, Vec<Rebuilt<'_>>> = BTreeMap::new();
    for item in rebuilt {
        groups.entry(item.key.clone()).or_default().push(item);
    }

    let mut cleanups = Vec::new();
    for (key, mut group) in groups {
        if !changed_keys.contains(&key) {
            continue;
        }
        group.sort_by(|left, right| {
            right
                .place
                .event_count
                .cmp(&left.place.event_count)
                .then_with(|| left.place.id.cmp(&right.place.id))
        });
        let keep_id = group[0].place.id.clone();
        let after = group[0].after.clone();
        let merged_ids: Vec<String> = group
            .iter()
            .skip(1)
            .map(|item| item.place.id.clone())
            .collect();
        let before_names: Vec<String> = group.iter().map(|item| item.place.name.clone()).collect();
        let event_count = group.iter().map(|item| item.place.event_count).sum();
        cleanups.push(PlaceCleanup {
            key,
            keep_id,
            merged_ids,
            before_names,
            after,
            event_count,
        });
    }
    cleanups.sort_by(|left, right| left.key.cmp(&right.key));

    DataCleanPlan {
        names: Vec::new(),
        places: cleanups,
    }
}

fn rewrite_name(name: &str, rules: &HashMap<&str, String>) -> (String, bool) {
    let mut replaced = false;
    let parts: Vec<String> = name
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let key = normalize_name_text(part).to_lowercase();
            match rules.get(key.as_str()) {
                Some(after) => {
                    replaced = true;
                    after.clone()
                }
                None => part.to_string(),
            }
        })
        .collect();
    (parts.join(", "), replaced)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(id: &str, name: &str, event_count: i64) -> PlaceSummary {
        PlaceSummary {
            id: id.to_string(),
            name: name.to_string(),
            notes: None,
            latitude: None,
            longitude: None,
            event_count,
        }
    }

    fn component<'a>(components: &'a [PlaceComponent], key: &str) -> &'a PlaceComponent {
        components
            .iter()
            .find(|component| component.key == key)
            .expect("component exists")
    }

    #[test]
    fn collects_variants_and_defaults_to_normalized_text() {
        let components = build_components(&[
            place("p1", "Boston, USA", 3),
            place("p2", "Boston, usa", 1),
            place("p3", "PARIS, France", 2),
        ]);
        let boston = component(&components, "boston");
        assert_eq!(boston.place_count, 2);
        assert_eq!(boston.variants[0].text, "Boston");
        assert_eq!(boston.variants[0].count, 2);

        let usa = component(&components, "usa");
        assert_eq!(usa.place_count, 2);
        assert_eq!(usa.after, "USA");

        assert_eq!(component(&components, "paris").after, "Paris");
    }

    #[test]
    fn preview_applies_only_selected_components() {
        let places = [
            place("p1", "Boston, USA", 3),
            place("p2", "Boston, usa", 1),
            place("p3", "Paris, France", 2),
        ];
        let mut components = build_components(&places);
        assert!(
            preview_plan(&places, &components, &HashSet::new())
                .places
                .is_empty()
        );

        let usa = component(&components, "usa").key.clone();
        components
            .iter_mut()
            .find(|component| component.key == usa)
            .expect("usa")
            .after = "United States".to_string();
        let preview = preview_plan(&places, &components, &HashSet::from([usa]));

        assert_eq!(preview.places.len(), 1);
        let cleanup = &preview.places[0];
        assert_eq!(cleanup.keep_id, "p1");
        assert_eq!(cleanup.merged_ids, ["p2"]);
        assert_eq!(cleanup.after, "Boston, United States");
        assert_eq!(cleanup.before_names, ["Boston, USA", "Boston, usa"]);
        assert_eq!(cleanup.event_count, 4);
    }

    #[test]
    fn preview_merges_with_an_existing_standard_name() {
        let places = [
            place("p1", "Boston, USA", 2),
            place("p2", "Boston, United States", 5),
        ];
        let mut components = build_components(&places);
        let usa = component(&components, "usa").key.clone();
        components
            .iter_mut()
            .find(|component| component.key == usa)
            .expect("usa")
            .after = "United States".to_string();

        let preview = preview_plan(&places, &components, &HashSet::from([usa]));
        assert_eq!(preview.places.len(), 1);
        assert_eq!(preview.places[0].keep_id, "p2");
        assert_eq!(preview.places[0].merged_ids, ["p1"]);
        assert_eq!(preview.places[0].after, "Boston, United States");
    }

    #[test]
    fn unrelated_components_are_left_alone() {
        let places = [place("p1", "Paris , France", 1)];
        let components = build_components(&places);
        let usa = components
            .iter()
            .find(|component| component.key == "usa")
            .map(|component| component.key.clone());
        let selected = usa.into_iter().collect();
        assert!(
            preview_plan(&places, &components, &selected)
                .places
                .is_empty()
        );
    }
}
