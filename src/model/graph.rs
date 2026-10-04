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
