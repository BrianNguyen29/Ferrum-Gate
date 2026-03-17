use ferrum_proto::{ActionProposal, IntentEnvelope, TrustLabel};

pub trait SemanticFirewall {
    fn label_input(&self, _content: &str, _existing: &[TrustLabel]) -> Vec<TrustLabel> {
        vec![]
    }

    fn contradiction_check(&self, _intent: &IntentEnvelope, _proposal: &ActionProposal) -> Vec<String> {
        vec![]
    }

    fn sanitize_output(&self, value: serde_json::Value) -> serde_json::Value {
        value
    }

    fn dlp_findings(&self, _value: &serde_json::Value) -> Vec<String> {
        vec![]
    }
}

pub struct NoopFirewall;

impl SemanticFirewall for NoopFirewall {}
