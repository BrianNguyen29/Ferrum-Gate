use std::collections::{HashMap, HashSet};

use ferrum_proto::{EventId, ProvenanceEvent, ProvenanceEventKind};

#[derive(Debug, thiserror::Error)]
pub enum LineageGraphError {
    #[error("cyclic or malformed lineage detected: {0:?}")]
    CyclicLineage(Vec<EventId>),
}

#[derive(Default)]
pub struct LineageGraph {
    events: Vec<ProvenanceEvent>,
    /// Reverse index: from_event_id -> event_ids that have this as a parent
    child_index: HashMap<EventId, Vec<EventId>>,
}

impl LineageGraph {
    pub fn from_events(events: Vec<ProvenanceEvent>) -> Self {
        let mut graph = Self {
            events,
            child_index: HashMap::new(),
        };
        // Build child_index for forward traversal
        for event in &graph.events {
            for edge in &event.parent_edges {
                graph
                    .child_index
                    .entry(edge.from_event_id)
                    .or_default()
                    .push(event.event_id);
            }
        }
        graph.sort();
        graph
    }

    pub fn push(&mut self, event: ProvenanceEvent) {
        // Update child_index when pushing
        for edge in &event.parent_edges {
            self.child_index
                .entry(edge.from_event_id)
                .or_default()
                .push(event.event_id);
        }
        self.events.push(event);
        self.sort();
    }

    pub fn events(&self) -> &[ProvenanceEvent] {
        &self.events
    }

    pub fn into_events(self) -> Vec<ProvenanceEvent> {
        self.events
    }

    pub fn terminal_events(&self) -> Vec<ProvenanceEvent> {
        self.events
            .iter()
            .filter(|event| is_terminal_kind(&event.kind))
            .cloned()
            .collect()
    }

    pub fn walk_backwards_from(&self, event_id: EventId) -> Vec<ProvenanceEvent> {
        let by_id: HashMap<EventId, &ProvenanceEvent> = self
            .events
            .iter()
            .map(|event| (event.event_id, event))
            .collect();
        let mut visited: HashSet<EventId> = HashSet::new();
        let mut frontier = vec![event_id];
        let mut lineage = Vec::new();

        while let Some(current_id) = frontier.pop() {
            if !visited.insert(current_id) {
                continue;
            }

            let Some(event) = by_id.get(&current_id) else {
                continue;
            };

            lineage.push((*event).clone());

            for edge in &event.parent_edges {
                frontier.push(edge.from_event_id);
            }
        }

        lineage.sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at));
        lineage
    }

    /// Walk forwards from a given event, collecting all descendant events.
    /// Uses the child_index to find children at each step.
    pub fn walk_forwards_from(&self, event_id: EventId) -> Vec<ProvenanceEvent> {
        let by_id: HashMap<EventId, &ProvenanceEvent> = self
            .events
            .iter()
            .map(|event| (event.event_id, event))
            .collect();
        let mut visited: HashSet<EventId> = HashSet::new();
        let mut frontier = vec![event_id];
        let mut descendants = Vec::new();

        while let Some(current_id) = frontier.pop() {
            if !visited.insert(current_id) {
                continue;
            }

            // Get children from the child_index
            if let Some(child_ids) = self.child_index.get(&current_id) {
                for &child_id in child_ids {
                    if !visited.contains(&child_id) {
                        frontier.push(child_id);
                    }
                }
            }

            // Don't include the starting event in descendants
            if current_id != event_id {
                if let Some(event) = by_id.get(&current_id) {
                    descendants.push((*event).clone());
                }
            }
        }

        descendants.sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at));
        descendants
    }

    fn sort(&mut self) {
        self.events
            .sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at));
    }

    /// Returns a topologically sorted list of events using Kahn's algorithm.
    /// Events with no parent edges (roots) come first, then their dependents.
    /// Within the same topological level, events are sorted by occurred_at for stability,
    /// with event_id as a deterministic tie-breaker.
    pub fn topological_sort(&self) -> Result<Vec<ProvenanceEvent>, LineageGraphError> {
        let by_id: HashMap<EventId, &ProvenanceEvent> = self
            .events
            .iter()
            .map(|event| (event.event_id, event))
            .collect();

        // in_degree[event] = number of parent_edges pointing to event (i.e., number of parents)
        let mut in_degree: HashMap<EventId, usize> = HashMap::new();
        for event in &self.events {
            in_degree.insert(event.event_id, event.parent_edges.len());
        }

        // Start with events that have no parents (in_degree == 0), sorted by occurred_at,
        // then by event_id for deterministic tie-breaking
        let mut queue: Vec<EventId> = in_degree
            .iter()
            .filter(|&(_, d)| *d == 0)
            .map(|(&id, _)| id)
            .collect();
        queue.sort_by(|a, b| {
            let evt_a = by_id.get(a).unwrap();
            let evt_b = by_id.get(b).unwrap();
            evt_a
                .occurred_at
                .cmp(&evt_b.occurred_at)
                .then_with(|| evt_a.event_id.0.cmp(&evt_b.event_id.0))
        });

        let mut result: Vec<ProvenanceEvent> = Vec::new();
        let mut processed: HashSet<EventId> = HashSet::new();

        while !queue.is_empty() {
            // queue.pop() takes the last element, but we sorted ascending (earliest first),
            // so we need to take from the front (index 0) to process in correct order
            let event_id = queue.remove(0);
            if processed.contains(&event_id) {
                continue;
            }
            processed.insert(event_id);

            if let Some(event) = by_id.get(&event_id) {
                result.push((*event).clone());
            }

            // For each child of event_id (found via child_index), decrement its in_degree
            if let Some(children) = self.child_index.get(&event_id) {
                for &child_id in children {
                    if let Some(deg) = in_degree.get_mut(&child_id) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 && !processed.contains(&child_id) {
                            // Insert child into queue maintaining occurred_at order,
                            // then event_id for deterministic tie-breaking
                            let child_evt = by_id.get(&child_id).unwrap();
                            let insert_pos = queue
                                .iter()
                                .position(|&id| {
                                    let id_evt = by_id.get(&id).unwrap();
                                    id_evt.occurred_at > child_evt.occurred_at
                                        || (id_evt.occurred_at == child_evt.occurred_at
                                            && id_evt.event_id.0 > child_evt.event_id.0)
                                })
                                .unwrap_or(queue.len());
                            queue.insert(insert_pos, child_id);
                        }
                    }
                }
            }
        }

        // Fail-closed: if we have unprocessed events (cycle/malformed lineage), return error
        let remaining: Vec<EventId> = self
            .events
            .iter()
            .filter(|e| !processed.contains(&e.event_id))
            .map(|e| e.event_id)
            .collect();
        if !remaining.is_empty() {
            return Err(LineageGraphError::CyclicLineage(remaining));
        }

        Ok(result)
    }
}

pub fn is_terminal_kind(kind: &ProvenanceEventKind) -> bool {
    matches!(
        kind,
        ProvenanceEventKind::SideEffectCommitted
            | ProvenanceEventKind::SideEffectCompensated
            | ProvenanceEventKind::SideEffectRolledBack
            | ProvenanceEventKind::ApprovalDenied
            | ProvenanceEventKind::Quarantined
            | ProvenanceEventKind::ErrorRaised
    )
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use ferrum_proto::{
        ActorRef, ActorType, HashChainRef, JsonMap, ObjectRef, ObjectType, ProvenanceEdge,
        ProvenanceEdgeType,
    };

    use super::*;

    fn make_event(
        kind: ProvenanceEventKind,
        occurred_at: chrono::DateTime<Utc>,
        parents: Vec<EventId>,
    ) -> ProvenanceEvent {
        ProvenanceEvent {
            event_id: EventId::new(),
            kind,
            occurred_at,
            actor: ActorRef {
                actor_type: ActorType::System,
                actor_id: "system".to_string(),
                display_name: Some("System".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Unknown,
                object_id: "object".to_string(),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: parents
                .into_iter()
                .map(|from_event_id| ProvenanceEdge {
                    edge_type: ProvenanceEdgeType::DerivedFrom,
                    from_event_id,
                    summary: None,
                })
                .collect(),
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: JsonMap::new(),
        }
    }

    #[test]
    fn terminal_events_only_return_terminal_kinds() {
        let base_time = Utc::now();
        let first = make_event(ProvenanceEventKind::IntentCompiled, base_time, Vec::new());
        let second = make_event(
            ProvenanceEventKind::SideEffectCommitted,
            base_time + Duration::seconds(1),
            vec![first.event_id],
        );
        let graph = LineageGraph::from_events(vec![first.clone(), second.clone()]);

        let terminal = graph.terminal_events();

        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0].event_id, second.event_id);
    }

    #[test]
    fn walk_backwards_returns_multi_hop_lineage() {
        let base_time = Utc::now();
        let root = make_event(ProvenanceEventKind::IntentCompiled, base_time, Vec::new());
        let middle = make_event(
            ProvenanceEventKind::ToolCallPrepared,
            base_time + Duration::seconds(1),
            vec![root.event_id],
        );
        let leaf = make_event(
            ProvenanceEventKind::SideEffectCommitted,
            base_time + Duration::seconds(2),
            vec![middle.event_id],
        );
        let graph = LineageGraph::from_events(vec![leaf.clone(), root.clone(), middle.clone()]);

        let lineage = graph.walk_backwards_from(leaf.event_id);
        let lineage_ids: Vec<EventId> = lineage.into_iter().map(|event| event.event_id).collect();

        assert_eq!(
            lineage_ids,
            vec![root.event_id, middle.event_id, leaf.event_id]
        );
    }

    #[test]
    fn walk_forwards_returns_multi_hop_descendants() {
        let base_time = Utc::now();
        let root = make_event(ProvenanceEventKind::IntentCompiled, base_time, Vec::new());
        let middle = make_event(
            ProvenanceEventKind::ToolCallPrepared,
            base_time + Duration::seconds(1),
            vec![root.event_id],
        );
        let leaf = make_event(
            ProvenanceEventKind::SideEffectCommitted,
            base_time + Duration::seconds(2),
            vec![middle.event_id],
        );
        let graph = LineageGraph::from_events(vec![leaf.clone(), root.clone(), middle.clone()]);

        let descendants = graph.walk_forwards_from(root.event_id);
        let descendant_ids: Vec<EventId> = descendants
            .into_iter()
            .map(|event| event.event_id)
            .collect();

        assert_eq!(descendant_ids, vec![middle.event_id, leaf.event_id]);
    }

    #[test]
    fn topological_sort_returns_parent_before_child() {
        let base_time = Utc::now();
        let root = make_event(ProvenanceEventKind::IntentCompiled, base_time, Vec::new());
        let middle = make_event(
            ProvenanceEventKind::ToolCallPrepared,
            base_time + Duration::seconds(1),
            vec![root.event_id],
        );
        let leaf = make_event(
            ProvenanceEventKind::SideEffectCommitted,
            base_time + Duration::seconds(2),
            vec![middle.event_id],
        );
        // Pass events out of order to verify sorting
        let graph = LineageGraph::from_events(vec![leaf.clone(), root.clone(), middle.clone()]);

        let sorted = graph.topological_sort().expect("no cycles expected");

        // Build position map
        let pos: HashMap<EventId, usize> = sorted
            .iter()
            .enumerate()
            .map(|(i, e)| (e.event_id, i))
            .collect();

        // Parents must appear before children
        assert!(
            pos[&root.event_id] < pos[&middle.event_id],
            "root should be before middle"
        );
        assert!(
            pos[&middle.event_id] < pos[&leaf.event_id],
            "middle should be before leaf"
        );
    }

    #[test]
    fn topological_sort_deterministic_tie_break_by_event_id() {
        let base_time = Utc::now();
        // Create two events with same timestamp
        let event_a = make_event(ProvenanceEventKind::IntentCompiled, base_time, Vec::new());
        let event_b = make_event(ProvenanceEventKind::ToolCallPrepared, base_time, Vec::new());

        // Make both roots (no parents) so they are in the same topological level
        let graph = LineageGraph::from_events(vec![event_b.clone(), event_a.clone()]);

        let sorted = graph.topological_sort().expect("no cycles expected");

        // With same timestamp, event_id ordering should determine order
        // The one with smaller event_id should come first
        assert!(
            sorted[0].event_id.0 < sorted[1].event_id.0,
            "smaller event_id should come first when timestamps are equal"
        );
    }

    #[test]
    fn topological_sort_returns_error_on_cyclic_lineage() {
        let base_time = Utc::now();
        // Valid inputs should work
        let root = make_event(ProvenanceEventKind::IntentCompiled, base_time, Vec::new());
        let child = make_event(
            ProvenanceEventKind::ToolCallPrepared,
            base_time + Duration::seconds(1),
            vec![root.event_id],
        );
        let valid_graph = LineageGraph::from_events(vec![root.clone(), child.clone()]);
        assert!(valid_graph.topological_sort().is_ok());

        // Verify the error type exists and can be constructed
        let cyclic_err = LineageGraphError::CyclicLineage(vec![root.event_id]);
        assert!(matches!(cyclic_err, LineageGraphError::CyclicLineage(_)));
        // Verify the error message
        let msg = cyclic_err.to_string();
        assert!(
            msg.contains("cyclic"),
            "error message should mention cyclic"
        );
    }

    /// Test: fail-closed on true cyclic lineage (A -> B -> A).
    /// This verifies that topological_sort actually detects and rejects cycles,
    /// not just that the error type exists.
    #[test]
    fn topological_sort_fails_closed_on_true_cyclic_lineage() {
        let base_time = Utc::now();

        // Create event A and event B with different timestamps
        let event_a = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::IntentCompiled,
            occurred_at: base_time,
            actor: ActorRef {
                actor_type: ActorType::System,
                actor_id: "system".to_string(),
                display_name: Some("System".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Unknown,
                object_id: "object".to_string(),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: Vec::new(), // Will be set below
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: JsonMap::new(),
        };

        let event_b = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::ToolCallPrepared,
            occurred_at: base_time + Duration::seconds(1),
            actor: ActorRef {
                actor_type: ActorType::System,
                actor_id: "system".to_string(),
                display_name: Some("System".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Unknown,
                object_id: "object".to_string(),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: Vec::new(), // Will be set below
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: JsonMap::new(),
        };

        // Now create the cycle: A has B as parent, B has A as parent
        let (id_a, id_b) = (event_a.event_id, event_b.event_id);

        let mut event_a_with_parent = event_a;
        event_a_with_parent.parent_edges = vec![ProvenanceEdge {
            edge_type: ProvenanceEdgeType::DerivedFrom,
            from_event_id: id_b, // A depends on B
            summary: None,
        }];

        let mut event_b_with_parent = event_b;
        event_b_with_parent.parent_edges = vec![ProvenanceEdge {
            edge_type: ProvenanceEdgeType::DerivedFrom,
            from_event_id: id_a, // B depends on A -- this creates a cycle
            summary: None,
        }];

        // Build graph with cyclic events
        let graph = LineageGraph::from_events(vec![event_a_with_parent, event_b_with_parent]);

        // topological_sort MUST fail-closed on cyclic lineage
        let result = graph.topological_sort();
        assert!(
            result.is_err(),
            "topological_sort should fail on cyclic lineage"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(err, LineageGraphError::CyclicLineage(_)),
            "expected CyclicLineage error, got {:?}",
            err
        );
    }
}
