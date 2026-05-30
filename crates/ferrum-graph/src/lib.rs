use std::collections::{HashMap, HashSet, VecDeque};

use ferrum_proto::ProvenanceEvent;

#[derive(Default)]
pub struct LineageGraph {
    /// event_id string → event
    events: HashMap<String, ProvenanceEvent>,
    /// parent_id → [child_ids]
    children: HashMap<String, Vec<String>>,
    /// child_id → [parent_ids]
    parents: HashMap<String, Vec<String>>,
}

impl LineageGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an event node to the graph.
    pub fn add_event(&mut self, event: ProvenanceEvent) {
        let id = event.event_id.to_string();
        self.events.insert(id.clone(), event);
        // Ensure entries exist for traversal even if no edges yet
        self.children.entry(id.clone()).or_default();
        self.parents.entry(id).or_default();
    }

    /// Add a directed edge (parent → child).
    pub fn add_edge(&mut self, parent_id: &str, child_id: &str) {
        // Add child to parent's children list
        self.children
            .entry(parent_id.to_string())
            .or_default()
            .push(child_id.to_string());
        // Add parent to child's parents list
        self.parents
            .entry(child_id.to_string())
            .or_default()
            .push(parent_id.to_string());
    }

    /// Check if an event exists in the graph.
    pub fn has_event(&self, event_id: &str) -> bool {
        self.events.contains_key(event_id)
    }

    /// Total number of events in the graph.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Total number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.children.values().map(|v| v.len()).sum()
    }

    /// BFS upward traversal (ancestors only).
    pub fn ancestors(&self, event_id: &str, max_hops: usize) -> Vec<String> {
        self.bfs(event_id, max_hops, BfsMode::Ancestors)
    }

    /// BFS downward traversal (descendants only).
    pub fn descendants(&self, event_id: &str, max_hops: usize) -> Vec<String> {
        self.bfs(event_id, max_hops, BfsMode::Descendants)
    }

    /// BFS in both directions (ancestors ∪ descendants).
    pub fn both_directions(&self, event_id: &str, max_hops: usize) -> Vec<String> {
        self.bfs(event_id, max_hops, BfsMode::Both)
    }

    /// Internal BFS implementation.
    fn bfs(&self, event_id: &str, max_hops: usize, mode: BfsMode) -> Vec<String> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut result: Vec<String> = Vec::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();

        visited.insert(event_id.to_string());
        queue.push_back((event_id.to_string(), 0));

        while let Some((current, hops)) = queue.pop_front() {
            if hops >= max_hops {
                continue;
            }

            let neighbors: Vec<String> = match mode {
                BfsMode::Ancestors => self.parents.get(&current).cloned().unwrap_or_default(),
                BfsMode::Descendants => self.children.get(&current).cloned().unwrap_or_default(),
                BfsMode::Both => {
                    let mut n = self.parents.get(&current).cloned().unwrap_or_default();
                    if let Some(child_neighbors) = self.children.get(&current) {
                        n.extend(child_neighbors.iter().cloned());
                    }
                    n
                }
            };

            for neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor.clone());
                    result.push(neighbor.clone());
                    queue.push_back((neighbor, hops + 1));
                }
            }
        }

        result
    }
}

#[derive(Clone, Copy)]
enum BfsMode {
    Ancestors,
    Descendants,
    Both,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        ActorRef, ActorType, EventId, HashChainRef, ObjectRef, ObjectType, ProvenanceEventKind,
    };
    use indexmap::IndexMap;
    use uuid::Uuid;

    /// Helper: create a minimal ProvenanceEvent for testing.
    fn make_event(id: &str) -> ProvenanceEvent {
        ProvenanceEvent {
            event_id: EventId(Uuid::parse_str(id).unwrap()),
            kind: ProvenanceEventKind::UserGoalReceived,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::User,
                actor_id: "test".to_string(),
                display_name: Some("Test".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Intent,
                object_id: "test".to_string(),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: IndexMap::new(),
            source_runtime_id: None,
        }
    }

    #[test]
    fn test_add_event_stores_event() {
        let mut graph = LineageGraph::new();
        let event = make_event("00000000-0000-0000-0000-000000000001");
        graph.add_event(event.clone());

        assert!(graph.has_event("00000000-0000-0000-0000-000000000001"));
        assert_eq!(graph.event_count(), 1);
    }

    #[test]
    fn test_add_edge_creates_parent_child() {
        let mut graph = LineageGraph::new();
        let parent = make_event("00000000-0000-0000-0000-000000000001");
        let child = make_event("00000000-0000-0000-0000-000000000002");
        graph.add_event(parent);
        graph.add_event(child);
        graph.add_edge(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        );

        // Child should see parent as ancestor
        let ancestors = graph.ancestors("00000000-0000-0000-0000-000000000002", 10);
        assert!(ancestors.contains(&"00000000-0000-0000-0000-000000000001".to_string()));

        // Parent should see child as descendant
        let descendants = graph.descendants("00000000-0000-0000-0000-000000000001", 10);
        assert!(descendants.contains(&"00000000-0000-0000-0000-000000000002".to_string()));

        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_ancestors_returns_parent_chain() {
        let mut graph = LineageGraph::new();
        // grandparent -> parent -> child
        let grandparent = make_event("00000000-0000-0000-0000-000000000001");
        let parent = make_event("00000000-0000-0000-0000-000000000002");
        let child = make_event("00000000-0000-0000-0000-000000000003");
        graph.add_event(grandparent);
        graph.add_event(parent);
        graph.add_event(child);
        graph.add_edge(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        );
        graph.add_edge(
            "00000000-0000-0000-0000-000000000002",
            "00000000-0000-0000-0000-000000000003",
        );

        let ancestors = graph.ancestors("00000000-0000-0000-0000-000000000003", 10);
        assert_eq!(ancestors.len(), 2);
        assert!(ancestors.contains(&"00000000-0000-0000-0000-000000000002".to_string()));
        assert!(ancestors.contains(&"00000000-0000-0000-0000-000000000001".to_string()));
    }

    #[test]
    fn test_descendants_returns_child_chain() {
        let mut graph = LineageGraph::new();
        // grandparent -> parent -> child
        let grandparent = make_event("00000000-0000-0000-0000-000000000001");
        let parent = make_event("00000000-0000-0000-0000-000000000002");
        let child = make_event("00000000-0000-0000-0000-000000000003");
        graph.add_event(grandparent);
        graph.add_event(parent);
        graph.add_event(child);
        graph.add_edge(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        );
        graph.add_edge(
            "00000000-0000-0000-0000-000000000002",
            "00000000-0000-0000-0000-000000000003",
        );

        let descendants = graph.descendants("00000000-0000-0000-0000-000000000001", 10);
        assert_eq!(descendants.len(), 2);
        assert!(descendants.contains(&"00000000-0000-0000-0000-000000000002".to_string()));
        assert!(descendants.contains(&"00000000-0000-0000-0000-000000000003".to_string()));
    }

    #[test]
    fn test_both_directions_combines() {
        let mut graph = LineageGraph::new();
        // A -> B -> C
        let a = make_event("00000000-0000-0000-0000-000000000001");
        let b = make_event("00000000-0000-0000-0000-000000000002");
        let c = make_event("00000000-0000-0000-0000-000000000003");
        graph.add_event(a);
        graph.add_event(b);
        graph.add_event(c);
        graph.add_edge(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        );
        graph.add_edge(
            "00000000-0000-0000-0000-000000000002",
            "00000000-0000-0000-0000-000000000003",
        );

        let both = graph.both_directions("00000000-0000-0000-0000-000000000002", 10);
        assert_eq!(both.len(), 2);
        assert!(both.contains(&"00000000-0000-0000-0000-000000000001".to_string()));
        assert!(both.contains(&"00000000-0000-0000-0000-000000000003".to_string()));
    }

    #[test]
    fn test_max_hops_limits_traversal() {
        let mut graph = LineageGraph::new();
        // A -> B -> C -> D
        let a = make_event("00000000-0000-0000-0000-000000000001");
        let b = make_event("00000000-0000-0000-0000-000000000002");
        let c = make_event("00000000-0000-0000-0000-000000000003");
        let d = make_event("00000000-0000-0000-0000-000000000004");
        graph.add_event(a);
        graph.add_event(b);
        graph.add_event(c);
        graph.add_event(d);
        graph.add_edge(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        );
        graph.add_edge(
            "00000000-0000-0000-0000-000000000002",
            "00000000-0000-0000-0000-000000000003",
        );
        graph.add_edge(
            "00000000-0000-0000-0000-000000000003",
            "00000000-0000-0000-0000-000000000004",
        );

        // With max_hops=1, only immediate neighbors
        let descendants = graph.descendants("00000000-0000-0000-0000-000000000001", 1);
        assert_eq!(descendants.len(), 1);
        assert!(descendants.contains(&"00000000-0000-0000-0000-000000000002".to_string()));

        // With max_hops=2, two hops
        let descendants2 = graph.descendants("00000000-0000-0000-0000-000000000001", 2);
        assert_eq!(descendants2.len(), 2);
        assert!(descendants2.contains(&"00000000-0000-0000-0000-000000000002".to_string()));
        assert!(descendants2.contains(&"00000000-0000-0000-0000-000000000003".to_string()));
    }

    #[test]
    fn test_ancestors_of_root_is_empty() {
        let mut graph = LineageGraph::new();
        let root = make_event("00000000-0000-0000-0000-000000000001");
        graph.add_event(root);

        let ancestors = graph.ancestors("00000000-0000-0000-0000-000000000001", 10);
        assert!(ancestors.is_empty());
    }

    #[test]
    fn test_descendants_of_leaf_is_empty() {
        let mut graph = LineageGraph::new();
        let leaf = make_event("00000000-0000-0000-0000-000000000001");
        graph.add_event(leaf);

        let descendants = graph.descendants("00000000-0000-0000-0000-000000000001", 10);
        assert!(descendants.is_empty());
    }

    #[test]
    fn test_cycle_does_not_infinite_loop() {
        let mut graph = LineageGraph::new();
        // A -> B -> C -> A (cycle)
        let a = make_event("00000000-0000-0000-0000-000000000001");
        let b = make_event("00000000-0000-0000-0000-000000000002");
        let c = make_event("00000000-0000-0000-0000-000000000003");
        graph.add_event(a);
        graph.add_event(b);
        graph.add_event(c);
        graph.add_edge(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        );
        graph.add_edge(
            "00000000-0000-0000-0000-000000000002",
            "00000000-0000-0000-0000-000000000003",
        );
        graph.add_edge(
            "00000000-0000-0000-0000-000000000003",
            "00000000-0000-0000-0000-000000000001",
        );

        // Should not infinite loop and should not return duplicated entries
        let descendants = graph.descendants("00000000-0000-0000-0000-000000000001", 10);
        // Each node should appear at most once
        let mut unique = descendants.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            descendants.len(),
            unique.len(),
            "Cycle caused duplicate entries"
        );
        // Should contain B and C (not A since it's the start node)
        assert_eq!(descendants.len(), 2);
    }

    #[test]
    fn test_event_count_and_edge_count() {
        let mut graph = LineageGraph::new();
        let e1 = make_event("00000000-0000-0000-0000-000000000001");
        let e2 = make_event("00000000-0000-0000-0000-000000000002");
        let e3 = make_event("00000000-0000-0000-0000-000000000003");
        graph.add_event(e1);
        graph.add_event(e2);
        graph.add_event(e3);
        graph.add_edge(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
        );
        graph.add_edge(
            "00000000-0000-0000-0000-000000000002",
            "00000000-0000-0000-0000-000000000003",
        );

        assert_eq!(graph.event_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }
}
