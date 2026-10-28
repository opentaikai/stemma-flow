//! Deterministic layered layout: turns loaded family data into a
//! pre-computed canvas scene that the paint loop can traverse without
//! allocating.
//!
//! People joined by a marriage form one compound and share a single rank,
//! remarriages extend the compound into a spouse chain with one union gap
//! per family, and each family's children row is packed under its own
//! union point. Everything is derived from rowid-ordered input, so
//! repeated builds are byte-identical.

mod builder;
mod compounds;
mod graph;
mod packing;
mod placement;
mod scene;

pub use scene::{NodeGeom, TreeScene, UnionGeom, truncate_name};

use crate::db::FamilyTree;
use builder::LayoutBuilder;

/// Node width in canvas units.
pub const NODE_W: f32 = 140.0;
/// Node height in canvas units.
pub const NODE_H: f32 = 56.0;
/// Separation between chain nodes that are not married to each other.
const H_SPACING: f32 = 24.0;
/// Union gap between spouses seated next to each other in a chain.
const SPOUSE_GAP: f32 = 12.0;
/// Vertical gap between two ranks (spec floor: 120).
const V_GAP: f32 = 120.0;
/// Character budget for node name labels.
const NAME_MAX_CHARS: usize = 16;

/// Builds the canvas scene for `tree`.
pub fn build_scene(tree: &FamilyTree, surname_first: bool) -> TreeScene {
    let mut builder = LayoutBuilder::new(tree, surname_first);
    builder.layout();
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::builder::LayoutBuilder;
    use super::graph::component_graph;
    use super::scene::person_display_name;
    use super::*;
    use crate::db::{Family, FamilyTree, Person};
    use egui::{Pos2, Rect};

    fn person(id: &str, given: &str, surname: &str) -> Person {
        Person {
            id: id.to_string(),
            given_name: given.to_string(),
            middle_name: String::new(),
            surname: surname.to_string(),
            nickname: String::new(),
            name_prefix: String::new(),
            name_suffix: String::new(),
            gender: "U".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn family(id: &str, husband: Option<&str>, wife: Option<&str>) -> Family {
        Family {
            id: id.to_string(),
            husband_id: husband.map(str::to_string),
            wife_id: wife.map(str::to_string),
            created_at: String::new(),
        }
    }

    /// Johan + Maria with two children, plus an unrelated single person.
    fn fixture() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("i1", "Johan", "Ahlberg"),
                person("i2", "Maria", "Ekdahl"),
                person("i3", "Elsa", "Ahlberg"),
                person("i4", "Nils", "Ahlberg"),
                person("i5", "Oskar", "Friis"),
            ],
            families: vec![family("f1", Some("i1"), Some("i2"))],
            child_links: vec![
                ("f1".to_string(), "i3".to_string()),
                ("f1".to_string(), "i4".to_string()),
            ],
        }
    }

    /// Johan + Maria -> Elsa, who marries Nils and has Otto: three ranks.
    fn lineage_tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("i1", "Johan", "Ahlberg"),
                person("i2", "Maria", "Ekdahl"),
                person("i3", "Elsa", "Ahlberg"),
                person("i4", "Nils", "Friis"),
                person("i5", "Otto", "Friis"),
            ],
            families: vec![
                family("f1", Some("i1"), Some("i2")),
                family("f2", Some("i3"), Some("i4")),
            ],
            child_links: vec![
                ("f1".to_string(), "i3".to_string()),
                ("f2".to_string(), "i5".to_string()),
            ],
        }
    }

    /// Johan remarries Karin: f1(i1, i2) -> Elsa, f2(i3, i1) -> Nils.
    fn remarriage_tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("i1", "Johan", "Ahlberg"),
                person("i2", "Maria", "Ekdahl"),
                person("i3", "Karin", "Vik"),
                person("i4", "Elsa", "Ahlberg"),
                person("i5", "Nils", "Ahlberg"),
            ],
            families: vec![
                family("f1", Some("i1"), Some("i2")),
                family("f2", Some("i3"), Some("i1")),
            ],
            child_links: vec![
                ("f1".to_string(), "i4".to_string()),
                ("f2".to_string(), "i5".to_string()),
            ],
        }
    }

    /// Johan with three successive spouses, no children.
    fn polygamy_tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("i1", "Johan", "Ahlberg"),
                person("i2", "Maria", "Ekdahl"),
                person("i3", "Karin", "Vik"),
                person("i4", "Ingrid", "Holm"),
            ],
            families: vec![
                family("f1", Some("i1"), Some("i2")),
                family("f2", Some("i1"), Some("i3")),
                family("f3", Some("i1"), Some("i4")),
            ],
            child_links: Vec::new(),
        }
    }

    /// Johan + Maria -> Elsa with three children fanned wider than her
    /// own node, and childless Nils as the sibling to tuck in.
    fn interlock_tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("i1", "Johan", "Ahlberg"),
                person("i2", "Maria", "Ekdahl"),
                person("i3", "Elsa", "Ahlberg"),
                person("i4", "Nils", "Ahlberg"),
                person("i5", "Otto", "Friis"),
                person("i6", "Eva", "Friis"),
                person("i7", "Lars", "Friis"),
            ],
            families: vec![
                family("f1", Some("i1"), Some("i2")),
                family("f2", Some("i3"), None),
            ],
            child_links: vec![
                ("f1".to_string(), "i3".to_string()),
                ("f1".to_string(), "i4".to_string()),
                ("f2".to_string(), "i5".to_string()),
                ("f2".to_string(), "i6".to_string()),
                ("f2".to_string(), "i7".to_string()),
            ],
        }
    }

    /// Six husband+wife couples with no links between them.
    fn six_couples_tree() -> FamilyTree {
        let mut people = Vec::new();
        let mut families = Vec::new();
        for couple in 0..6 {
            let husband = format!("h{couple}");
            let wife = format!("w{couple}");
            let family_id = format!("m{couple}");
            people.push(person(&husband, "Husband", "Tree"));
            people.push(person(&wife, "Wife", "Tree"));
            families.push(family(&family_id, Some(&husband), Some(&wife)));
        }
        FamilyTree {
            people,
            families,
            child_links: Vec::new(),
        }
    }

    fn node<'a>(scene: &'a TreeScene, id: &str) -> &'a NodeGeom {
        scene
            .nodes
            .iter()
            .find(|node| node.person_id == id)
            .unwrap_or_else(|| panic!("missing node {id}"))
    }

    fn union<'a>(scene: &'a TreeScene, family_id: &str) -> &'a UnionGeom {
        scene
            .unions
            .iter()
            .find(|union| union.family_id == family_id)
            .unwrap_or_else(|| panic!("missing union {family_id}"))
    }

    #[test]
    fn children_sit_below_their_parents() {
        let scene = build_scene(&fixture(), false);
        let father = node(&scene, "i1");
        let mother = node(&scene, "i2");
        for child in ["i3", "i4"] {
            let child = node(&scene, child);
            assert!(
                child.bounds.min.y > father.bounds.max.y,
                "child must be below the father"
            );
            assert!(
                child.bounds.min.y > mother.bounds.max.y,
                "child must be below the mother"
            );
        }
    }

    #[test]
    fn spouses_are_adjacent_and_children_centred() {
        let scene = build_scene(&fixture(), false);
        let father = node(&scene, "i1");
        let mother = node(&scene, "i2");
        assert!(
            (mother.bounds.min.x - father.bounds.max.x - SPOUSE_GAP).abs() < 1e-3,
            "spouses are separated by exactly SPOUSE_GAP"
        );

        let left = node(&scene, "i3").bounds.min.x;
        let right = node(&scene, "i4").bounds.max.x;
        let children_centre = (left + right) / 2.0;
        let couple_centre = (father.bounds.min.x + mother.bounds.max.x) / 2.0;
        assert!(
            (children_centre - couple_centre).abs() < 1e-3,
            "children row must be centred under the couple"
        );
    }

    #[test]
    fn layout_is_deterministic() {
        let tree = fixture();
        let first = build_scene(&tree, false);
        let second = build_scene(&tree, false);
        assert_eq!(first, second, "repeated builds must be identical");
        assert_eq!(first.nodes.len(), 5);
        assert_eq!(first.unions.len(), 1);
    }

    #[test]
    fn unions_carry_spouse_bar_and_child_anchors() {
        let scene = build_scene(&fixture(), false);
        let union = scene
            .unions
            .first()
            .unwrap_or_else(|| panic!("union must exist"));
        let line = union
            .spouse_line
            .unwrap_or_else(|| panic!("spouse bar must exist"));
        let [bar_start, bar_end] = line;
        let midpoint = bar_start + (bar_end - bar_start) * 0.5;
        assert_eq!(
            union.union, midpoint,
            "union point is the spouse-bar midpoint"
        );
        assert_eq!(union.children.len(), 2);
        for (child_id, anchor) in &union.children {
            let bounds = node(&scene, child_id).bounds;
            assert_eq!(*anchor, Pos2::new(bounds.center().x, bounds.min.y));
        }
    }

    #[test]
    fn single_parent_unions_drop_from_the_node_bottom() {
        let mut tree = fixture();
        tree.families = vec![family("f1", Some("i1"), None)];
        let scene = build_scene(&tree, false);
        let union = scene
            .unions
            .first()
            .unwrap_or_else(|| panic!("union must exist"));
        assert!(union.spouse_line.is_none());
        let parent = node(&scene, "i1").bounds;
        assert_eq!(union.union, parent.center_bottom());
    }

    #[test]
    fn empty_tree_yields_empty_scene() {
        let scene = build_scene(&FamilyTree::default(), false);
        assert!(scene.nodes.is_empty());
        assert!(scene.unions.is_empty());
    }

    #[test]
    fn layout_survives_cycles() {
        let tree = FamilyTree {
            people: vec![person("i1", "Self", "Loop")],
            families: vec![family("f1", Some("i1"), None)],
            child_links: vec![("f1".to_string(), "i1".to_string())],
        };
        let scene = build_scene(&tree, false);
        assert_eq!(scene.nodes.len(), 1, "cyclic data must not hang");
        assert_eq!(scene.unions.len(), 1);
    }

    #[test]
    fn names_are_truncated_with_ellipsis() {
        assert_eq!(truncate_name("Elsa", 16), "Elsa");
        assert_eq!(truncate_name("", 16), "");
        assert_eq!(
            truncate_name("Anna Maria von Rosenfelt", 16),
            "Anna Maria von…"
        );
        assert!(
            truncate_name("Anna Maria von Rosenfelt", 16)
                .chars()
                .count()
                <= 16,
            "truncated names stay within the budget"
        );
        assert_eq!(truncate_name("anything", 0), "");
        assert_eq!(
            person_display_name(&person("i1", "Anna Maria", "von Rosenfelt"), false),
            "Anna Maria von…"
        );
        let mut with_middle = person("i2", "Ana", "Lee");
        with_middle.middle_name = "Mar".to_string();
        assert_eq!(
            person_display_name(&with_middle, false),
            "Ana Mar Lee",
            "canvas labels include the middle name"
        );
    }

    #[test]
    fn cross_generation_couple_shares_one_rank() {
        // The married-in spouse must not stay on the root rank next to
        // the grandparents.
        let scene = build_scene(&lineage_tree(), false);
        let child = node(&scene, "i3");
        let spouse = node(&scene, "i4");
        assert_eq!(
            child.bounds.min.y, spouse.bounds.min.y,
            "both spouses sit on the same rank"
        );
        let grandparent = node(&scene, "i1");
        assert_eq!(
            child.bounds.min.y - grandparent.bounds.min.y,
            NODE_H + V_GAP,
            "the couple sits exactly one rank below the grandparents"
        );
        assert_eq!(
            node(&scene, "i5").bounds.min.y - child.bounds.min.y,
            NODE_H + V_GAP,
            "the next generation follows one rank below"
        );
    }

    #[test]
    fn second_family_children_stay_in_the_block() {
        // Both children belong under their own family's union, inside
        // the chain block.
        let scene = build_scene(&remarriage_tree(), false);
        let first = node(&scene, "i1").bounds;
        let second = node(&scene, "i2").bounds;
        let third = node(&scene, "i3").bounds;
        assert!(
            third.center().x < first.center().x && first.center().x < second.center().x,
            "chain order is i3, i1, i2"
        );

        let first_child = node(&scene, "i4").bounds;
        let second_child = node(&scene, "i5").bounds;
        assert_eq!(
            first_child.min.y, second_child.min.y,
            "both children rows share the child rank"
        );
        assert!(
            (second_child.center().x - union(&scene, "f2").union.x).abs() < 1e-3,
            "f2 children centred under the f2 union"
        );
        // The two unions sit only SPOUSE_GAP + NODE_W apart, so the f1
        // row slides right to keep the rows H_SPACING clear.
        let f1_shift = first_child.center().x - union(&scene, "f1").union.x;
        assert!(
            (-1e-3..=H_SPACING + 1e-3).contains(&f1_shift),
            "f1 row stays under its union, pushed clear of the f2 row"
        );
        assert!(
            first_child.min.x >= second_child.max.x + H_SPACING - 1e-3,
            "the two children rows keep at least H_SPACING apart"
        );
    }

    #[test]
    fn spacing_constants_meet_spec_minimums() {
        let scene = build_scene(&fixture(), false);
        let parent = node(&scene, "i1").bounds;
        let mother = node(&scene, "i2").bounds;
        let child = node(&scene, "i3").bounds;
        assert_eq!(
            child.min.y - parent.min.y,
            NODE_H + V_GAP,
            "row pitch is NODE_H + V_GAP"
        );
        assert!(
            child.min.y - parent.max.y >= 120.0 - 1e-6,
            "generations are at least 120 apart"
        );
        assert_eq!(
            mother.min.x - parent.max.x,
            SPOUSE_GAP,
            "spouses sit one narrow union gap apart"
        );
        let sibling = node(&scene, "i4").bounds;
        assert!(
            sibling.min.x - node(&scene, "i3").bounds.max.x >= H_SPACING - 1e-6,
            "siblings keep the node separation"
        );
    }

    #[test]
    fn polygamy_skips_the_crossing_spouse_bar() {
        // i1 has three spouses; the third marriage is not adjacent on the
        // chain, so its bar would cut across the nodes in between.
        let scene = build_scene(&polygamy_tree(), false);
        assert!(
            union(&scene, "f1").spouse_line.is_some(),
            "the first marriage keeps its bar"
        );
        assert!(
            union(&scene, "f2").spouse_line.is_some(),
            "the second marriage sits adjacent and keeps its bar"
        );
        assert!(
            union(&scene, "f3").spouse_line.is_none(),
            "the far-apart marriage skips the crossing bar"
        );
        let third_union = union(&scene, "f3").union.x;
        for id in ["i1", "i2", "i3", "i4"] {
            let bounds = node(&scene, id).bounds;
            assert!(
                third_union <= bounds.min.x || third_union >= bounds.max.x,
                "the union sits in a gap, not inside node {id}"
            );
        }
    }

    #[test]
    fn sibling_subtrees_interlock_without_overlapping() {
        // Elsa's three children fan out wider than her own node; the
        // childless sibling must clear her chain by one node gap, not by
        // the whole grandchildren row.
        let scene = build_scene(&interlock_tree(), false);
        let deep = node(&scene, "i3").bounds;
        let tucked = node(&scene, "i4").bounds;
        assert_eq!(
            tucked.min.x - deep.max.x,
            H_SPACING,
            "the childless sibling clears the deep chain by exactly the node gap"
        );
        assert!(
            node(&scene, "i7").bounds.max.x > tucked.min.x,
            "the wide grandchildren row reaches past the tucked sibling"
        );
        assert_eq!(
            tucked.min.y, deep.min.y,
            "both siblings still share their rank"
        );
    }

    #[test]
    fn no_two_nodes_overlap() {
        for tree in [
            fixture(),
            lineage_tree(),
            remarriage_tree(),
            polygamy_tree(),
            interlock_tree(),
            six_couples_tree(),
        ] {
            assert_no_overlap(&build_scene(&tree, false));
        }
    }

    fn assert_no_overlap(scene: &TreeScene) {
        for (index, node) in scene.nodes.iter().enumerate() {
            for other in scene.nodes.iter().skip(index + 1) {
                assert!(
                    !node.bounds.intersects(other.bounds),
                    "nodes {} and {} overlap",
                    node.person_id,
                    other.person_id
                );
            }
        }
    }

    #[test]
    fn component_extent_is_measured_not_summed() {
        let tree = lineage_tree();
        let mut builder = LayoutBuilder::new(&tree, false);
        let bucket: Vec<usize> = (0..builder.compounds.len()).collect();
        let inflated: f32 = bucket
            .iter()
            .map(|&compound| builder.block_of(compound).width)
            .sum::<f32>()
            + H_SPACING * (bucket.len().saturating_sub(1)) as f32;

        let (width, height) = builder.measure_bucket(&bucket);

        let scene = build_scene(&tree, false);
        assert!(
            (width - scene.bounds.width()).abs() < 0.5,
            "measured width {width} must match the scene {}",
            scene.bounds.width()
        );
        assert!((height - scene.bounds.height()).abs() < 0.5);
        assert!(
            width < inflated,
            "descendants must not be counted twice: {width} < {inflated}"
        );
    }

    #[test]
    fn trunk_sits_half_a_gap_below_the_rank() {
        let scene = build_scene(&fixture(), false);
        let trunk = union(&scene, "f1").trunk_y;
        let parent = node(&scene, "i1").bounds;
        let child = node(&scene, "i3").bounds;
        assert_eq!(trunk, 60.0, "rank 0 trunk is half a 120 gap below 0");
        assert!(
            trunk > parent.max.y && trunk < child.min.y,
            "the trunk row falls in the open gap between the ranks"
        );

        let lineage = build_scene(&lineage_tree(), false);
        let second_rank_trunk = union(&lineage, "f2").trunk_y;
        assert_eq!(
            second_rank_trunk,
            (NODE_H + V_GAP) + V_GAP * 0.5,
            "rank 1 trunk follows the same formula"
        );
    }

    #[test]
    fn disconnected_components_pack_into_a_grid() {
        use petgraph::algo::connected_components;

        let tree = six_couples_tree();
        assert_eq!(
            connected_components(&component_graph(&tree)),
            6,
            "the sample is six separate family groups"
        );
        let scene = build_scene(&tree, false);
        let mut ranks: Vec<f32> = scene.nodes.iter().map(|node| node.bounds.min.y).collect();
        ranks.sort_by(f32::total_cmp);
        ranks.dedup();
        assert!(
            ranks.len() >= 2,
            "the couples spread over several grid rows"
        );
        let shared_row = scene.nodes.iter().enumerate().any(|(index, node)| {
            scene
                .nodes
                .iter()
                .skip(index + 1)
                .any(|other| other.bounds.min.y == node.bounds.min.y)
        });
        assert!(
            shared_row,
            "some row holds more than one couple: a grid, not a column"
        );
        let mut trunks: Vec<f32> = scene.unions.iter().map(|union| union.trunk_y).collect();
        trunks.sort_by(f32::total_cmp);
        trunks.dedup();
        assert_eq!(
            trunks,
            vec![60.0, 236.0, 412.0],
            "trunk rows follow the three grid rows"
        );
    }

    /// Interior probe: a segment only counts as a hit when it crosses the
    /// node box with a 0.5 margin, so endpoints resting on the border
    /// (anchors, bar ends) do not trip it.
    fn segment_enters_box([start, end]: &[Pos2; 2], bounds: Rect) -> bool {
        let inner = bounds.expand(-0.5);
        if start.x == end.x {
            let (top, bottom) = if start.y <= end.y {
                (start.y, end.y)
            } else {
                (end.y, start.y)
            };
            start.x >= inner.min.x
                && start.x <= inner.max.x
                && bottom >= inner.min.y
                && top <= inner.max.y
        } else {
            let (left, right) = if start.x <= end.x {
                (start.x, end.x)
            } else {
                (end.x, start.x)
            };
            start.y >= inner.min.y
                && start.y <= inner.max.y
                && right >= inner.min.x
                && left <= inner.max.x
        }
    }

    fn assert_connectors_avoid_nodes(scene: &TreeScene) {
        let segments = crate::gui::connectors::build_segments(&scene.unions);
        for segment in &segments {
            for node in &scene.nodes {
                assert!(
                    !segment_enters_box(segment, node.bounds),
                    "segment {segment:?} cuts node {}",
                    node.person_id
                );
            }
        }
    }

    #[test]
    fn no_connector_enters_a_node_box() {
        for tree in [
            fixture(),
            remarriage_tree(),
            polygamy_tree(),
            interlock_tree(),
        ] {
            let scene = build_scene(&tree, false);
            assert!(
                !crate::gui::connectors::build_segments(&scene.unions).is_empty(),
                "fixture must produce connectors"
            );
            assert_connectors_avoid_nodes(&scene);
        }
    }

    #[test]
    fn layout_real_export_when_present() -> Result<(), crate::gedcom::GedcomError> {
        // Private WikiTree export in gitignored `test_data/`; skipped in CI.
        let Ok(entries) = std::fs::read_dir("test_data") else {
            return Ok(());
        };
        let sample = entries
            .flatten()
            .find(|entry| entry.path().extension().is_some_and(|ext| ext == "ged"));
        let Some(sample) = sample else {
            return Ok(());
        };
        let mut conn = rusqlite::Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        crate::db::init_db(&conn)?;
        let file = std::fs::File::open(sample.path())?;
        crate::gedcom::import_gedcom(&mut conn, std::io::BufReader::new(file))?;
        let tree = crate::db::load_family_tree(&conn)?;
        let scene = build_scene(&tree, false);
        assert!(!scene.nodes.is_empty(), "the real export must layout");
        assert_no_overlap(&scene);
        assert_connectors_avoid_nodes(&scene);
        assert_eq!(
            scene,
            build_scene(&tree, false),
            "real layouts stay deterministic"
        );
        Ok(())
    }
}
