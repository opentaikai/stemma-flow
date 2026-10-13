//! Connected-component labelling of the family graph.

use std::collections::HashMap;

use petgraph::graph::UnGraph;
use petgraph::unionfind::UnionFind;
use petgraph::visit::EdgeRef;

use crate::db::{Family, FamilyTree};

/// Undirected graph linking every spouse pair and every parent to their
/// children; people with no links stay isolated nodes.
pub(super) fn component_graph(tree: &FamilyTree) -> UnGraph<(), ()> {
    let mut graph: UnGraph<(), ()> = UnGraph::default();
    let mut position: HashMap<&str, petgraph::graph::NodeIndex> = HashMap::new();
    for person in &tree.people {
        position.insert(person.id.as_str(), graph.add_node(()));
    }
    for family in &tree.families {
        let husband = family
            .husband_id
            .as_deref()
            .and_then(|id| position.get(id))
            .copied();
        let wife = family
            .wife_id
            .as_deref()
            .and_then(|id| position.get(id))
            .copied();
        if let (Some(husband), Some(wife)) = (husband, wife) {
            graph.add_edge(husband, wife, ());
        }
    }
    let families: HashMap<&str, &Family> = tree
        .families
        .iter()
        .map(|family| (family.id.as_str(), family))
        .collect();
    for (family_id, child_id) in &tree.child_links {
        let Some(family) = families.get(family_id.as_str()) else {
            continue;
        };
        let Some(&child) = position.get(child_id.as_str()) else {
            continue;
        };
        for parent in [family.husband_id.as_deref(), family.wife_id.as_deref()]
            .into_iter()
            .flatten()
        {
            let Some(&parent_node) = position.get(parent) else {
                continue;
            };
            graph.add_edge(parent_node, child, ());
        }
    }
    graph
}

/// Connected-component label per person, dense and assigned in
/// first-seen person order so repeated builds stay deterministic.
pub(super) fn compute_person_components(tree: &FamilyTree) -> Vec<usize> {
    let graph = component_graph(tree);
    let mut sets = UnionFind::new(graph.node_count());
    for edge in graph.edge_references() {
        sets.union(edge.source().index(), edge.target().index());
    }
    let mut remap: HashMap<usize, usize> = HashMap::new();
    let mut next = 0usize;
    sets.into_labeling()
        .into_iter()
        .map(|label| match remap.get(&label) {
            Some(&component) => component,
            None => {
                remap.insert(label, next);
                next += 1;
                next - 1
            }
        })
        .collect()
}
