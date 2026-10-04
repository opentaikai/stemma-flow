//! In-memory genealogy graph: the bridge between the SQLite schema and the
//! `egui` render loop.
//!
//! Uses [`StableGraph`] so removing a person never invalidates the
//! [`NodeIndex`]es held for the remaining people.

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::Direction;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use petgraph::visit::EdgeRef;

/// A single individual as held in the render graph.
#[derive(Debug, Clone)]
pub struct PersonNode {
    pub id: String,
    pub given_name: String,
    pub surname: String,
    pub gender: String,
}

/// The kinds of connections drawn between two [`PersonNode`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relationship {
    /// Directional: parent -> child.
    ParentChild,
    /// Bidirectional link between spouses.
    Spouse,
}

/// Graph-backed person store with fast UUID lookups.
#[derive(Debug, Clone)]
pub struct GenealogyDb {
    pub graph: StableGraph<PersonNode, Relationship>,
    pub id_to_node: HashMap<String, NodeIndex>,
}

impl Default for GenealogyDb {
    fn default() -> Self {
        Self::new()
    }
}

impl GenealogyDb {
    pub fn new() -> Self {
        Self {
            graph: StableGraph::new(),
            id_to_node: HashMap::new(),
        }
    }

    /// Adds a person, or updates the data of an already registered UUID in
    /// place while keeping its index and edges. Returns the node index.
    pub fn add_person(&mut self, person: PersonNode) -> NodeIndex {
        if let Some(&idx) = self.id_to_node.get(&person.id)
            && let Some(weight) = self.graph.node_weight_mut(idx)
        {
            *weight = person;
            return idx;
        }
        let id = person.id.clone();
        let idx = self.graph.add_node(person);
        self.id_to_node.insert(id, idx);
        idx
    }

    /// Links `parent_id` to `child_id` with a [`Relationship::ParentChild`]
    /// edge. Fails on unknown ids, self-links and cycles.
    pub fn add_parent_child(&mut self, parent_id: &str, child_id: &str) -> Result<(), String> {
        let parent = self.node(parent_id)?;
        let child = self.node(child_id)?;
        if parent == child {
            return Err(format!("person cannot be their own parent: {parent_id}"));
        }
        if self.is_descendant(child, parent) {
            return Err(format!(
                "cycle rejected: {parent_id} is already a descendant of {child_id}"
            ));
        }
        self.graph
            .update_edge(parent, child, Relationship::ParentChild);
        Ok(())
    }

    /// Links two people with reciprocal [`Relationship::Spouse`] edges.
    pub fn add_spouses(&mut self, spouse_a_id: &str, spouse_b_id: &str) -> Result<(), String> {
        let a = self.node(spouse_a_id)?;
        let b = self.node(spouse_b_id)?;
        if a == b {
            return Err(format!("person cannot be their own spouse: {spouse_a_id}"));
        }
        self.graph.update_edge(a, b, Relationship::Spouse);
        self.graph.update_edge(b, a, Relationship::Spouse);
        Ok(())
    }

    /// Removes a person and every connected edge, returning their data.
    pub fn remove_person(&mut self, person_id: &str) -> Option<PersonNode> {
        let idx = self.id_to_node.remove(person_id)?;
        self.graph.remove_node(idx)
    }

    /// All direct parents of a person (incoming [`Relationship::ParentChild`]
    /// edges). Unknown ids yield an empty vector.
    pub fn get_parents(&self, person_id: &str) -> Vec<PersonNode> {
        let Some(&idx) = self.id_to_node.get(person_id) else {
            return Vec::new();
        };
        self.graph
            .edges_directed(idx, Direction::Incoming)
            .filter(|edge| *edge.weight() == Relationship::ParentChild)
            .filter_map(|edge| self.graph.node_weight(edge.source()).cloned())
            .collect()
    }

    /// All direct children of a person (outgoing [`Relationship::ParentChild`]
    /// edges). Unknown ids yield an empty vector.
    pub fn get_children(&self, person_id: &str) -> Vec<PersonNode> {
        let Some(&idx) = self.id_to_node.get(person_id) else {
            return Vec::new();
        };
        self.graph
            .edges_directed(idx, Direction::Outgoing)
            .filter(|edge| *edge.weight() == Relationship::ParentChild)
            .filter_map(|edge| self.graph.node_weight(edge.target()).cloned())
            .collect()
    }

    /// Everyone linked to a person via [`Relationship::Spouse`], deduplicated.
    /// Unknown ids yield an empty vector.
    pub fn get_spouses(&self, person_id: &str) -> Vec<PersonNode> {
        let Some(&idx) = self.id_to_node.get(person_id) else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut spouses = Vec::new();
        let edges = self
            .graph
            .edges_directed(idx, Direction::Outgoing)
            .chain(self.graph.edges_directed(idx, Direction::Incoming));
        for edge in edges {
            if *edge.weight() != Relationship::Spouse {
                continue;
            }
            let neighbor = if edge.source() == idx {
                edge.target()
            } else {
                edge.source()
            };
            if seen.insert(neighbor)
                && let Some(node) = self.graph.node_weight(neighbor)
            {
                spouses.push(node.clone());
            }
        }
        spouses
    }

    /// Every ancestor reachable through incoming [`Relationship::ParentChild`]
    /// edges, breadth-first and deduplicated. Unknown ids yield an empty
    /// vector.
    pub fn get_ancestors(&self, person_id: &str) -> Vec<PersonNode> {
        let Some(&start) = self.id_to_node.get(person_id) else {
            return Vec::new();
        };
        let mut visited = HashSet::from([start]);
        let mut queue = VecDeque::from([start]);
        let mut ancestors = Vec::new();
        while let Some(idx) = queue.pop_front() {
            for edge in self.graph.edges_directed(idx, Direction::Incoming) {
                if *edge.weight() != Relationship::ParentChild {
                    continue;
                }
                let parent = edge.source();
                if visited.insert(parent)
                    && let Some(node) = self.graph.node_weight(parent)
                {
                    ancestors.push(node.clone());
                    queue.push_back(parent);
                }
            }
        }
        ancestors
    }

    fn node(&self, person_id: &str) -> Result<NodeIndex, String> {
        self.id_to_node
            .get(person_id)
            .copied()
            .ok_or_else(|| format!("person not found: {person_id}"))
    }

    /// Whether `target` is a descendant of `start`, i.e. reachable by walking
    /// parent -> child edges from `start`. Used to reject the back-edge that
    /// `add_parent_child` would otherwise create.
    fn is_descendant(&self, start: NodeIndex, target: NodeIndex) -> bool {
        let mut visited = HashSet::new();
        let mut stack = vec![start];
        while let Some(idx) = stack.pop() {
            if idx == target {
                return true;
            }
            if !visited.insert(idx) {
                continue;
            }
            for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                if *edge.weight() == Relationship::ParentChild {
                    stack.push(edge.target());
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GM: &str = "gm";
    const GP: &str = "gp";
    const PA: &str = "pa";
    const OT: &str = "ot";
    const CH: &str = "ch";

    fn person(id: &str, given_name: &str) -> PersonNode {
        PersonNode {
            id: id.to_string(),
            given_name: given_name.to_string(),
            surname: "Testerson".to_string(),
            gender: "U".to_string(),
        }
    }

    fn ids_of(people: Vec<PersonNode>) -> HashSet<String> {
        people.into_iter().map(|person| person.id).collect()
    }

    fn id_set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    /// gm --spouse-- gp       pa --spouse-- ot
    ///       \      /
    ///          pa
    ///          |
    ///          ch
    ///
    /// `gm` and `gp` are the parents of `pa`; `pa` and `ot` are the parents
    /// of `ch`.
    fn pedigree() -> Result<GenealogyDb, String> {
        let mut db = GenealogyDb::new();
        for id in [GM, GP, PA, OT, CH] {
            db.add_person(person(id, id));
        }
        db.add_parent_child(GM, PA)?;
        db.add_parent_child(GP, PA)?;
        db.add_parent_child(PA, CH)?;
        db.add_parent_child(OT, CH)?;
        db.add_spouses(GM, GP)?;
        db.add_spouses(PA, OT)?;
        Ok(db)
    }

    #[test]
    fn new_starts_empty() {
        let db = GenealogyDb::new();
        assert_eq!(db.graph.node_count(), 0);
        assert_eq!(db.graph.edge_count(), 0);
        assert!(db.id_to_node.is_empty());

        let default = GenealogyDb::default();
        assert_eq!(default.graph.node_count(), 0);
        assert!(default.id_to_node.is_empty());
    }

    #[test]
    fn add_person_inserts_and_updates_in_place() {
        let mut db = GenealogyDb::new();

        let index = db.add_person(person("id-1", "Lars"));
        assert_eq!(db.graph.node_count(), 1);
        assert_eq!(db.id_to_node.get("id-1"), Some(&index));

        let again = db.add_person(person("id-1", "Lars-Erik"));
        assert_eq!(again, index, "existing uuid must keep its index");
        assert_eq!(db.graph.node_count(), 1, "no duplicate node");
        let name = db
            .graph
            .node_weight(index)
            .map(|node| node.given_name.as_str());
        assert_eq!(name, Some("Lars-Erik"));
    }

    #[test]
    fn add_parent_child_links_and_reports_bad_ids() -> Result<(), String> {
        let mut db = GenealogyDb::new();
        let a = db.add_person(person("a", "Anna"));
        let b = db.add_person(person("b", "Bo"));

        db.add_parent_child("a", "b")?;
        assert!(db.graph.contains_edge(a, b));
        assert!(!db.graph.contains_edge(b, a));

        db.add_parent_child("a", "b")?;
        assert_eq!(db.graph.edge_count(), 1, "relinking must not duplicate");

        assert!(db.add_parent_child("ghost", "b").is_err());
        assert!(db.add_parent_child("a", "ghost").is_err());

        match db.add_parent_child("a", "a") {
            Err(message) => assert!(message.contains("own parent")),
            Ok(()) => panic!("self-link must be rejected"),
        }
        assert_eq!(db.graph.edge_count(), 1);
        Ok(())
    }

    #[test]
    fn cycles_are_prevented_without_mutating_the_graph() -> Result<(), String> {
        let mut db = pedigree()?;
        let edges_before = db.graph.edge_count();

        for (parent, child) in [(PA, GM), (CH, GP), (CH, GM)] {
            match db.add_parent_child(parent, child) {
                Err(message) => assert!(message.contains("cycle")),
                Ok(()) => panic!("cycle {parent} -> {child} must be rejected"),
            }
        }
        assert_eq!(db.graph.edge_count(), edges_before);

        // A shortcut to an existing descendant is not a cycle.
        db.add_parent_child(GM, CH)?;
        assert_eq!(db.graph.edge_count(), edges_before + 1);
        assert_eq!(ids_of(db.get_parents(CH)), id_set(&[PA, OT, GM]));
        Ok(())
    }

    #[test]
    fn add_spouses_links_both_directions_idempotently() -> Result<(), String> {
        let mut db = GenealogyDb::new();
        let a = db.add_person(person("a", "Astrid"));
        let b = db.add_person(person("b", "Bjorn"));

        db.add_spouses("a", "b")?;
        assert!(db.graph.contains_edge(a, b));
        assert!(db.graph.contains_edge(b, a));
        assert_eq!(db.graph.edge_count(), 2);

        db.add_spouses("a", "b")?;
        assert_eq!(db.graph.edge_count(), 2, "relinking must not duplicate");
        assert_eq!(ids_of(db.get_spouses("a")), id_set(&["b"]));
        assert_eq!(ids_of(db.get_spouses("b")), id_set(&["a"]));

        match db.add_spouses("a", "a") {
            Err(message) => assert!(message.contains("own spouse")),
            Ok(()) => panic!("self-spouse must be rejected"),
        }
        assert!(db.add_spouses("a", "ghost").is_err());
        Ok(())
    }

    #[test]
    fn traversals_return_connected_people() -> Result<(), String> {
        let db = pedigree()?;

        assert_eq!(ids_of(db.get_parents(PA)), id_set(&[GM, GP]));
        assert_eq!(ids_of(db.get_parents(GM)), id_set(&[]));
        assert_eq!(ids_of(db.get_children(CH)), id_set(&[]));
        assert_eq!(ids_of(db.get_children(PA)), id_set(&[CH]));
        assert_eq!(ids_of(db.get_spouses(PA)), id_set(&[OT]));
        assert_eq!(
            ids_of(db.get_ancestors(CH)),
            id_set(&[PA, OT, GM, GP]),
            "all four grandparents exactly once"
        );
        assert_eq!(ids_of(db.get_ancestors(GM)), id_set(&[]));

        assert!(db.get_parents("ghost").is_empty());
        assert!(db.get_children("ghost").is_empty());
        assert!(db.get_spouses("ghost").is_empty());
        assert!(db.get_ancestors("ghost").is_empty());
        Ok(())
    }

    #[test]
    fn remove_person_drops_edges_and_preserves_other_indices() -> Result<(), String> {
        let mut db = pedigree()?;
        let pa_idx = *db.id_to_node.get(PA).ok_or("pa missing")?;
        let ch_idx = *db.id_to_node.get(CH).ok_or("ch missing")?;

        let removed = db.remove_person(PA).ok_or("pa not removed")?;
        assert_eq!(removed.id, PA);
        assert!(!db.id_to_node.contains_key(PA));
        assert_eq!(db.graph.node_count(), 4);

        assert!(db.graph.node_weight(pa_idx).is_none(), "slot freed");
        assert!(
            db.graph.node_weight(ch_idx).is_some(),
            "other indices stay valid"
        );
        assert_eq!(ids_of(db.get_parents(CH)), id_set(&[OT]));
        assert!(db.get_children(GM).is_empty());
        assert_eq!(ids_of(db.get_spouses(OT)), id_set(&[]));

        for (id, idx) in &db.id_to_node {
            let node = db.graph.node_weight(*idx).ok_or("stale index")?;
            assert_eq!(&node.id, id);
        }

        let reused = db.add_person(person(PA, "Par"));
        assert!(db.graph.node_weight(reused).is_some());
        Ok(())
    }
}
