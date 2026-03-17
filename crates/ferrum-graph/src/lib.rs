use ferrum_proto::ProvenanceEvent;

#[derive(Default)]
pub struct LineageGraph {
    events: Vec<ProvenanceEvent>,
}

impl LineageGraph {
    pub fn push(&mut self, event: ProvenanceEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[ProvenanceEvent] {
        &self.events
    }
}
