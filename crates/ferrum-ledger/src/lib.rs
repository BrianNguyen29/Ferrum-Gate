use ferrum_proto::ProvenanceEvent;

#[derive(Default)]
pub struct InMemoryLedger {
    entries: Vec<ProvenanceEvent>,
}

impl InMemoryLedger {
    pub fn append(&mut self, event: ProvenanceEvent) {
        self.entries.push(event);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
