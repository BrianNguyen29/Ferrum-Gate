use std::collections::{HashMap, HashSet};

use ferrum_proto::{EventId, ProvenanceEvent, ProvenanceEventKind};

#[derive(Default)]
pub struct LineageGraph {
    events: Vec<ProvenanceEvent>,
}

impl LineageGraph {
    pub fn from_events(events: Vec<ProvenanceEvent>) -> Self {
        let mut graph = Self { events };
        graph.sort();
        graph
    }

    pub fn push(&mut self, event: ProvenanceEvent) {
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

    fn sort(&mut self) {
        self.events
            .sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at));
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
}
