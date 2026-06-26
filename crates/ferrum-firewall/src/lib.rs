use ferrum_proto::{ActionProposal, IntentEnvelope, TrustLabel as ProtoTrustLabel};
use std::collections::HashMap;

pub trait SemanticFirewall {
    fn label_input(&self, _content: &str, _existing: &[ProtoTrustLabel]) -> Vec<ProtoTrustLabel> {
        vec![]
    }

    fn contradiction_check(
        &self,
        _intent: &IntentEnvelope,
        _proposal: &ActionProposal,
    ) -> Vec<String> {
        vec![]
    }

    fn sanitize_output(&self, value: serde_json::Value) -> serde_json::Value {
        value
    }

    fn dlp_findings(&self, _value: &serde_json::Value) -> Vec<String> {
        vec![]
    }
}

// =============================================================================
// TaintScoringFirewall types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustLabel {
    Trusted,     // taint 0-30
    Suspicious,  // taint 31-69
    Untrusted,   // taint 70-89
    Quarantined, // taint 90-100
}

impl TrustLabel {
    pub fn from_taint_score(taint_score: u8) -> Self {
        match taint_score {
            0..=30 => TrustLabel::Trusted,
            31..=69 => TrustLabel::Suspicious,
            70..=89 => TrustLabel::Untrusted,
            _ => TrustLabel::Quarantined,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Contradiction {
    pub field_a: String,
    pub value_a: String,
    pub field_b: String,
    pub value_b: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct FirewallContext {
    pub source: String,
    pub intent: Option<String>,
    pub trust_score: u8, // 0-100
    pub is_external: bool,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackClass {
    R0,
    R1,
    R2,
    R3,
}

impl From<ferrum_proto::RollbackClass> for RollbackClass {
    fn from(rc: ferrum_proto::RollbackClass) -> Self {
        match rc {
            ferrum_proto::RollbackClass::R0NativeReversible => RollbackClass::R0,
            ferrum_proto::RollbackClass::R1SnapshotRecoverable => RollbackClass::R1,
            ferrum_proto::RollbackClass::R2Compensatable => RollbackClass::R2,
            ferrum_proto::RollbackClass::R3IrreversibleHighConsequence => RollbackClass::R3,
        }
    }
}

// =============================================================================
// TaintScoringFirewall implementation
// =============================================================================

pub struct TaintScoringFirewall;

impl TaintScoringFirewall {
    pub fn new() -> Self {
        TaintScoringFirewall
    }

    /// Compute taint score (0-100) based on firewall context.
    ///
    /// Scoring rules:
    /// - External source: +30
    /// - Low trust_score (<50): +20
    /// - Empty/null intent: +15
    /// - Known dangerous attributes (e.g., "privileged": "true"): +20
    /// - Cap at 100
    pub fn compute_taint_score(&self, context: &FirewallContext) -> u8 {
        let mut score: u8 = 0;

        // External source: +30
        if context.is_external {
            score += 30;
        }

        // Low trust_score (<50): +20
        if context.trust_score < 50 {
            score += 20;
        }

        // Empty/null intent: +15
        if context.intent.is_none() || context.intent.as_deref() == Some("") {
            score += 15;
        }

        // Known dangerous attributes: +20 each (accumulate)
        let dangerous_attrs = [
            ("privileged", "true"),
            ("admin", "true"),
            ("dangerous", "true"),
            ("unsafe", "true"),
            ("bypass", "true"),
            ("exec", "true"),
        ];

        for (key, danger_value) in dangerous_attrs {
            if context.attributes.get(key).map(|v| v.as_str()) == Some(danger_value) {
                score += 20;
            }
        }

        // Cap at 100
        score.min(100)
    }

    /// Assign trust label based on taint score.
    pub fn label_trust(&self, context: &FirewallContext) -> TrustLabel {
        let taint = self.compute_taint_score(context);
        TrustLabel::from_taint_score(taint)
    }

    /// Detect contradictory attributes in the context.
    ///
    /// Detects:
    /// - If "action" is "read" but "write_target" is set → contradiction
    /// - If "trust_level" is "high" but "source" is "external" → contradiction
    /// - If "requires_approval" is "false" but rollback_class is R3 → contradiction
    pub fn detect_contradiction(&self, context: &FirewallContext) -> Vec<Contradiction> {
        let mut contradictions = Vec::new();

        // Check: action="read" but write_target is set
        if let Some(action) = context.attributes.get("action") {
            if action == "read" && context.attributes.contains_key("write_target") {
                contradictions.push(Contradiction {
                    field_a: "action".to_string(),
                    value_a: "read".to_string(),
                    field_b: "write_target".to_string(),
                    value_b: context
                        .attributes
                        .get("write_target")
                        .cloned()
                        .unwrap_or_default(),
                    reason: "read action specified but write_target is set".to_string(),
                });
            }
        }

        // Check: trust_level="high" but source="external"
        if let Some(trust_level) = context.attributes.get("trust_level") {
            if trust_level == "high" && context.is_external {
                contradictions.push(Contradiction {
                    field_a: "trust_level".to_string(),
                    value_a: "high".to_string(),
                    field_b: "source".to_string(),
                    value_b: context.source.clone(),
                    reason: "trust_level is high but source is external".to_string(),
                });
            }
        }

        // Check: requires_approval="false" but rollback_class is R3
        if let Some(requires_approval) = context.attributes.get("requires_approval") {
            if requires_approval == "false" {
                if let Some(rc_str) = context.attributes.get("rollback_class") {
                    if rc_str == "R3" || rc_str == "R3IrreversibleHighConsequence" {
                        contradictions.push(Contradiction {
                            field_a: "requires_approval".to_string(),
                            value_a: "false".to_string(),
                            field_b: "rollback_class".to_string(),
                            value_b: rc_str.clone(),
                            reason:
                                "requires_approval is false but rollback_class is R3 (irreversible)"
                                    .to_string(),
                        });
                    }
                }
            }
        }

        contradictions
    }

    /// Basic sanitization: strip control characters, normalize whitespace.
    pub fn sanitize(&self, input: &str) -> String {
        // Replace control characters (ASCII 0-31 except tab, newline, carriage return) with space
        let replaced: String = input
            .chars()
            .map(|c| {
                let code = c as u32;
                if (code < 32 && code != 9 && code != 10 && code != 13) || code == 127 {
                    ' '
                } else {
                    c
                }
            })
            .collect();

        // Normalize whitespace: collapse multiple spaces/tabs/newlines into single space
        let mut result = String::with_capacity(replaced.len());
        let mut last_was_ws = false;

        for c in replaced.chars() {
            if c.is_whitespace() {
                if !last_was_ws {
                    result.push(' ');
                    last_was_ws = true;
                }
            } else {
                result.push(c);
                last_was_ws = false;
            }
        }

        Self::redact_secrets(result.trim())
    }

    /// Redact common secret patterns from a string.
    ///
    /// Targets: bearer/authorization tokens, API keys, GitHub tokens, AWS
    /// access-key-like values, PEM private key markers.  Preserves UUIDs and
    /// correlation IDs (high-confidence, low false-positive design).
    fn redact_secrets(input: &str) -> String {
        use regex::Regex;
        use std::sync::LazyLock;

        static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
            vec![
                // PEM private key blocks (match first to avoid partial matches)
                Regex::new(r"(?i)-----BEGIN\s+(?:ENCRYPTED\s+|RSA\s+|EC\s+|OPENSSH\s+|DSA\s+)?PRIVATE\s+KEY-----[A-Za-z0-9\s+/=]*?-----END\s+(?:ENCRYPTED\s+|RSA\s+|EC\s+|OPENSSH\s+|DSA\s+)?PRIVATE\s+KEY-----")
                    .expect("valid static PEM regex"),
                // GitHub tokens
                Regex::new(r"\bgh[opusr]_[A-Za-z0-9]{36,}\b")
                    .expect("valid static GitHub token regex"),
                Regex::new(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b")
                    .expect("valid static GitHub PAT regex"),
                // AWS access keys
                Regex::new(r"\bAKIA[0-9A-Z]{16}\b")
                    .expect("valid static AWS AKIA regex"),
                Regex::new(r"\bASIA[0-9A-Z]{16}\b")
                    .expect("valid static AWS ASIA regex"),
                // Bearer / authorization tokens
                Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9\-_\.=+/]{8,}")
                    .expect("valid static Bearer regex"),
                // API keys (common prefixes)
                Regex::new(r"(?i)\b(?:api[_-]?key|secret[_-]?key|access[_-]?key|auth[_-]?token)\s*[:=]\s*[A-Za-z0-9\-_\.=+/]{8,}")
                    .expect("valid static API key regex"),
            ]
        });

        let mut result = input.to_string();
        for regex in PATTERNS.iter() {
            result = regex.replace_all(&result, "[REDACTED]").into_owned();
        }
        result
    }

    /// Determine if quarantine is needed based on taint score and rollback class.
    ///
    /// Rules:
    /// - R0 never quarantined regardless of taint
    /// - R3 with taint >= 70 → quarantine
    /// - R1/R2 with taint >= 90 → quarantine
    pub fn should_quarantine(&self, taint_score: u8, rollback_class: RollbackClass) -> bool {
        match rollback_class {
            RollbackClass::R0 => false,
            RollbackClass::R3 => taint_score >= 70,
            RollbackClass::R1 | RollbackClass::R2 => taint_score >= 90,
        }
    }
}

impl Default for TaintScoringFirewall {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticFirewall for TaintScoringFirewall {
    fn sanitize_output(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => serde_json::Value::String(self.sanitize(&s)),
            serde_json::Value::Object(mut map) => {
                for (_, v) in map.iter_mut() {
                    *v = self.sanitize_output(std::mem::take(v));
                }
                serde_json::Value::Object(map)
            }
            serde_json::Value::Array(mut arr) => {
                for v in arr.iter_mut() {
                    *v = self.sanitize_output(std::mem::take(v));
                }
                serde_json::Value::Array(arr)
            }
            other => other,
        }
    }
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_empty_context() -> FirewallContext {
        FirewallContext {
            source: "internal".to_string(),
            intent: Some("read data".to_string()),
            trust_score: 80,
            is_external: false,
            attributes: HashMap::new(),
        }
    }

    // Test 1: taint_score_zero_for_trusted_internal
    #[test]
    fn test_taint_score_zero_for_trusted_internal() {
        let fw = TaintScoringFirewall::new();
        let ctx = FirewallContext {
            source: "internal".to_string(),
            intent: Some("read data".to_string()),
            trust_score: 80,
            is_external: false,
            attributes: HashMap::new(),
        };
        let score = fw.compute_taint_score(&ctx);
        assert_eq!(score, 0);
    }

    // Test 2: taint_score_high_for_external_untrusted
    #[test]
    fn test_taint_score_high_for_external_untrusted() {
        let fw = TaintScoringFirewall::new();
        let ctx = FirewallContext {
            source: "external".to_string(),
            intent: None,
            trust_score: 30,
            is_external: true,
            attributes: HashMap::new(),
        };
        let score = fw.compute_taint_score(&ctx);
        // external: +30, low trust: +20, no intent: +15 = 65
        assert_eq!(score, 65);
    }

    // Test 3: taint_score_caps_at_100
    #[test]
    fn test_taint_score_caps_at_100() {
        let fw = TaintScoringFirewall::new();
        let mut ctx = FirewallContext {
            source: "external".to_string(),
            intent: None,
            trust_score: 30,
            is_external: true,
            attributes: HashMap::new(),
        };
        ctx.attributes
            .insert("privileged".to_string(), "true".to_string());
        ctx.attributes
            .insert("admin".to_string(), "true".to_string());

        let score = fw.compute_taint_score(&ctx);
        // external: +30, low trust: +20, no intent: +15, privileged: +20, admin: +20 = 105 -> capped at 100
        assert_eq!(score, 100);
    }

    // Test 4: trust_label_trusted_for_low_taint
    #[test]
    fn test_trust_label_trusted_for_low_taint() {
        let fw = TaintScoringFirewall::new();
        let ctx = FirewallContext {
            source: "internal".to_string(),
            intent: Some("read".to_string()),
            trust_score: 80,
            is_external: false,
            attributes: HashMap::new(),
        };
        let label = fw.label_trust(&ctx);
        assert_eq!(label, TrustLabel::Trusted);
    }

    // Test 5: trust_label_suspicious_for_medium_taint
    #[test]
    fn test_trust_label_suspicious_for_medium_taint() {
        let fw = TaintScoringFirewall::new();
        let ctx = FirewallContext {
            source: "external".to_string(),
            intent: Some("process".to_string()),
            trust_score: 40,
            is_external: true,
            attributes: HashMap::new(),
        };
        let label = fw.label_trust(&ctx);
        // external: +30, low trust: +20 = 50 -> Suspicious (31-69)
        assert_eq!(label, TrustLabel::Suspicious);
    }

    // Test 6: trust_label_untrusted_for_high_taint
    #[test]
    fn test_trust_label_untrusted_for_high_taint() {
        let fw = TaintScoringFirewall::new();
        let ctx = FirewallContext {
            source: "external".to_string(),
            intent: Some("execute".to_string()),
            trust_score: 30,
            is_external: true,
            attributes: HashMap::new(),
        };
        let _label = fw.label_trust(&ctx);
        // external: +30, low trust: +20 = 50... wait that's Suspicious
        // Let's make it higher
        let ctx2 = FirewallContext {
            source: "external".to_string(),
            intent: None,
            trust_score: 30,
            is_external: true,
            attributes: HashMap::new(),
        };
        let _label2 = fw.label_trust(&ctx2);
        // external: +30, low trust: +20, no intent: +15 = 65 -> Suspicious
        // We need 70-89 for Untrusted
        let mut ctx3 = FirewallContext {
            source: "external".to_string(),
            intent: None,
            trust_score: 30,
            is_external: true,
            attributes: HashMap::new(),
        };
        ctx3.attributes
            .insert("privileged".to_string(), "true".to_string());
        let label3 = fw.label_trust(&ctx3);
        // external: +30, low trust: +20, no intent: +15, privileged: +20 = 85 -> Untrusted
        assert_eq!(label3, TrustLabel::Untrusted);
    }

    // Test 7: trust_label_quarantined_for_max_taint
    #[test]
    fn test_trust_label_quarantined_for_max_taint() {
        let fw = TaintScoringFirewall::new();
        let mut ctx = FirewallContext {
            source: "external".to_string(),
            intent: None,
            trust_score: 20,
            is_external: true,
            attributes: HashMap::new(),
        };
        ctx.attributes
            .insert("privileged".to_string(), "true".to_string());
        let _label = fw.label_trust(&ctx);
        // external: +30, low trust: +20, no intent: +15, privileged: +20 = 85... not enough
        // Let me recalculate: to get 90+ we need more
        ctx.attributes
            .insert("admin".to_string(), "true".to_string());
        let label2 = fw.label_trust(&ctx);
        // external: +30, low trust: +20, no intent: +15, privileged: +20, admin: +20 = 105 -> capped 100
        assert_eq!(label2, TrustLabel::Quarantined);
    }

    // Test 8: contradiction_read_with_write_target
    #[test]
    fn test_contradiction_read_with_write_target() {
        let fw = TaintScoringFirewall::new();
        let mut ctx = create_empty_context();
        ctx.attributes
            .insert("action".to_string(), "read".to_string());
        ctx.attributes
            .insert("write_target".to_string(), "/etc/passwd".to_string());

        let contradictions = fw.detect_contradiction(&ctx);
        assert_eq!(contradictions.len(), 1);
        assert_eq!(contradictions[0].field_a, "action");
        assert_eq!(contradictions[0].value_a, "read");
        assert_eq!(contradictions[0].field_b, "write_target");
    }

    // Test 9: contradiction_high_trust_external_source
    #[test]
    fn test_contradiction_high_trust_external_source() {
        let fw = TaintScoringFirewall::new();
        let ctx = FirewallContext {
            source: "external".to_string(),
            intent: Some("read".to_string()),
            trust_score: 80,
            is_external: true,
            attributes: HashMap::new(),
        };
        // Manually set trust_level via attributes since label_trust uses trust_score
        let mut ctx2 = ctx;
        ctx2.attributes
            .insert("trust_level".to_string(), "high".to_string());

        let contradictions = fw.detect_contradiction(&ctx2);
        assert_eq!(contradictions.len(), 1);
        assert_eq!(contradictions[0].field_a, "trust_level");
        assert_eq!(contradictions[0].value_a, "high");
        assert_eq!(contradictions[0].field_b, "source");
    }

    // Test 10: no_contradiction_for_consistent_context
    #[test]
    fn test_no_contradiction_for_consistent_context() {
        let fw = TaintScoringFirewall::new();
        let ctx = FirewallContext {
            source: "internal".to_string(),
            intent: Some("read".to_string()),
            trust_score: 80,
            is_external: false,
            attributes: HashMap::new(),
        };

        let contradictions = fw.detect_contradiction(&ctx);
        assert!(contradictions.is_empty());
    }

    // Test 11: sanitize_strips_control_chars
    #[test]
    fn test_sanitize_strips_control_chars() {
        let fw = TaintScoringFirewall::new();
        let input = "hello\x00world\x1ftest";
        let sanitized = fw.sanitize(input);
        assert_eq!(sanitized, "hello world test");
    }

    // Test 12: should_quarantine_r3_with_high_taint
    #[test]
    fn test_should_quarantine_r3_with_high_taint() {
        let fw = TaintScoringFirewall::new();
        assert!(fw.should_quarantine(70, RollbackClass::R3));
        assert!(fw.should_quarantine(85, RollbackClass::R3));
        assert!(fw.should_quarantine(100, RollbackClass::R3));
    }

    // Test 13: should_not_quarantine_r0
    #[test]
    fn test_should_not_quarantine_r0() {
        let fw = TaintScoringFirewall::new();
        assert!(!fw.should_quarantine(0, RollbackClass::R0));
        assert!(!fw.should_quarantine(50, RollbackClass::R0));
        assert!(!fw.should_quarantine(100, RollbackClass::R0));
    }

    // Test 14: trust_label_from_taint_score_edges
    #[test]
    fn test_trust_label_from_taint_score_edges() {
        assert_eq!(TrustLabel::from_taint_score(0), TrustLabel::Trusted);
        assert_eq!(TrustLabel::from_taint_score(30), TrustLabel::Trusted);
        assert_eq!(TrustLabel::from_taint_score(31), TrustLabel::Suspicious);
        assert_eq!(TrustLabel::from_taint_score(69), TrustLabel::Suspicious);
        assert_eq!(TrustLabel::from_taint_score(70), TrustLabel::Untrusted);
        assert_eq!(TrustLabel::from_taint_score(89), TrustLabel::Untrusted);
        assert_eq!(TrustLabel::from_taint_score(90), TrustLabel::Quarantined);
        assert_eq!(TrustLabel::from_taint_score(100), TrustLabel::Quarantined);
    }

    // Test 15: sanitize_preserves_newlines
    #[test]
    fn test_sanitize_preserves_newlines() {
        let fw = TaintScoringFirewall::new();
        let input = "line1\nline2\r\nline3";
        let sanitized = fw.sanitize(input);
        assert_eq!(sanitized, "line1 line2 line3"); // whitespace normalized to spaces
    }

    // Test 16: rollback_class_from_proto
    #[test]
    fn test_rollback_class_from_proto() {
        use ferrum_proto::RollbackClass as ProtoRC;
        assert_eq!(
            RollbackClass::R0,
            RollbackClass::from(ProtoRC::R0NativeReversible)
        );
        assert_eq!(
            RollbackClass::R1,
            RollbackClass::from(ProtoRC::R1SnapshotRecoverable)
        );
        assert_eq!(
            RollbackClass::R2,
            RollbackClass::from(ProtoRC::R2Compensatable)
        );
        assert_eq!(
            RollbackClass::R3,
            RollbackClass::from(ProtoRC::R3IrreversibleHighConsequence)
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Invariant 11: Output sanitization (trait-level)
    // ─────────────────────────────────────────────────────────────────────────

    /// Invariant 11: sanitize_output strips control characters from JSON strings.
    #[test]
    fn test_sanitize_output_strips_control_chars() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "message": "hello\x00world\x1ftest"
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        let msg = obj.get("message").unwrap().as_str().unwrap();
        assert_eq!(msg, "hello world test");
    }

    /// Invariant 11: sanitize_output handles nested JSON structures.
    #[test]
    fn test_sanitize_output_nested_json() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "outer": {
                "inner": "text\x00here",
                "array": ["item1\x00", "item2\x1f"]
            }
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        let outer = obj.get("outer").unwrap().as_object().unwrap();
        let inner = outer.get("inner").unwrap().as_str().unwrap();
        assert_eq!(inner, "text here");
        let arr = outer.get("array").unwrap().as_array().unwrap();
        assert_eq!(arr[0].as_str().unwrap(), "item1");
        assert_eq!(arr[1].as_str().unwrap(), "item2");
    }

    /// Invariant 11: sanitize_output preserves JSON structure (non-string values unchanged).
    #[test]
    fn test_sanitize_output_preserves_structure() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "number": 42,
            "bool": true,
            "null": null,
            "string": "clean text",
            "nested": {
                "arr": [1, 2, 3]
            }
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        assert_eq!(obj.get("number").unwrap().as_i64().unwrap(), 42);
        assert!(obj.get("bool").unwrap().as_bool().unwrap());
        assert!(obj.get("null").unwrap().is_null());
        assert_eq!(obj.get("string").unwrap().as_str().unwrap(), "clean text");
        let nested = obj.get("nested").unwrap().as_object().unwrap();
        assert_eq!(nested.get("arr").unwrap().as_array().unwrap().len(), 3);
    }

    /// Invariant 11: sanitize_output handles deeply nested structures.
    #[test]
    fn test_sanitize_output_deeply_nested() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "level1": {
                "level2": {
                    "level3": {
                        "text": "deep\x00text"
                    }
                }
            }
        });
        let sanitized = fw.sanitize_output(input);
        let l1 = sanitized.as_object().unwrap();
        let l2 = l1.get("level1").unwrap().as_object().unwrap();
        let l3 = l2.get("level2").unwrap().as_object().unwrap();
        let l4 = l3.get("level3").unwrap().as_object().unwrap();
        assert_eq!(l4.get("text").unwrap().as_str().unwrap(), "deep text");
    }

    /// Invariant 11: sanitize_output handles arrays at root level.
    #[test]
    fn test_sanitize_output_array_at_root() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!(["item\x001", "item\x002"]);
        let sanitized = fw.sanitize_output(input);
        let arr = sanitized.as_array().unwrap();
        assert_eq!(arr[0].as_str().unwrap(), "item 1");
        assert_eq!(arr[1].as_str().unwrap(), "item 2");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Secret redaction tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_sanitize_redacts_bearer_token() {
        let fw = TaintScoringFirewall::new();
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let sanitized = fw.sanitize(input);
        assert!(
            !sanitized.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"),
            "bearer token should be redacted"
        );
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_api_key() {
        let fw = TaintScoringFirewall::new();
        let input = "api_key=abc1234567890def";
        let sanitized = fw.sanitize(input);
        assert!(
            !sanitized.contains("abc1234567890def"),
            "api key value should be redacted"
        );
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_secret_key() {
        let fw = TaintScoringFirewall::new();
        let input = "secret_key=supersecretvalue123";
        let sanitized = fw.sanitize(input);
        assert!(!sanitized.contains("supersecretvalue123"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_access_key() {
        let fw = TaintScoringFirewall::new();
        let input = "access-key=anothersecretvalue";
        let sanitized = fw.sanitize(input);
        assert!(!sanitized.contains("anothersecretvalue"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_auth_token() {
        let fw = TaintScoringFirewall::new();
        let input = "auth_token=tokensecret123456";
        let sanitized = fw.sanitize(input);
        assert!(!sanitized.contains("tokensecret123456"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_github_token() {
        let fw = TaintScoringFirewall::new();
        let token = format!("ghp_{}", "1".repeat(36));
        let input = format!("token {}", token);
        let sanitized = fw.sanitize(&input);
        assert!(
            !sanitized.contains(&token),
            "GitHub token should be redacted"
        );
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_github_pat() {
        let fw = TaintScoringFirewall::new();
        let input = "github_pat_12345678901234567890_abcdef";
        let sanitized = fw.sanitize(input);
        assert!(!sanitized.contains("github_pat_12345678901234567890_abcdef"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_aws_key() {
        let fw = TaintScoringFirewall::new();
        let input = "AKIAIOSFODNN7EXAMPLE";
        let sanitized = fw.sanitize(input);
        assert!(!sanitized.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_aws_session_key() {
        let fw = TaintScoringFirewall::new();
        let input = "ASIAIOSFODNN7EXAMPLE";
        let sanitized = fw.sanitize(input);
        assert!(!sanitized.contains("ASIAIOSFODNN7EXAMPLE"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_pem_private_key() {
        let fw = TaintScoringFirewall::new();
        let input = "-----BEGIN RSA PRIVATE KEY----- MIIEpAIBAAKCAQEA0Z3VS5JJcds3xfn/ygWyF8PbnGy0AHB7MhgwNRPb -----END RSA PRIVATE KEY-----";
        let sanitized = fw.sanitize(input);
        assert!(!sanitized.contains("MIIEpAIBAAKCAQEA0Z3VS5JJcds3xfn/ygWyF8PbnGy0AHB7MhgwNRPb"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_preserves_uuid() {
        let fw = TaintScoringFirewall::new();
        let input = "550e8400-e29b-41d4-a716-446655440000";
        let sanitized = fw.sanitize(input);
        assert_eq!(sanitized, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_sanitize_preserves_correlation_id() {
        let fw = TaintScoringFirewall::new();
        let input = "correlation_id=550e8400-e29b-41d4-a716-446655440000";
        let sanitized = fw.sanitize(input);
        assert_eq!(
            sanitized,
            "correlation_id=550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_sanitize_preserves_short_api_key() {
        let fw = TaintScoringFirewall::new();
        let input = "api_key=1234";
        let sanitized = fw.sanitize(input);
        assert_eq!(sanitized, "api_key=1234");
    }

    #[test]
    fn test_sanitize_preserves_normal_text() {
        let fw = TaintScoringFirewall::new();
        let input = "hello world";
        let sanitized = fw.sanitize(input);
        assert_eq!(sanitized, "hello world");
    }

    #[test]
    fn test_sanitize_redacts_combined_with_control_chars() {
        let fw = TaintScoringFirewall::new();
        let input = "hello\x00Bearer abcdefgh12345678";
        let sanitized = fw.sanitize(input);
        assert!(!sanitized.contains("abcdefgh12345678"));
        assert!(sanitized.contains("[REDACTED]"));
        assert_eq!(sanitized, "hello [REDACTED]");
    }

    #[test]
    fn test_sanitize_output_redacts_nested_secrets() {
        let fw = TaintScoringFirewall::new();
        let input = serde_json::json!({
            "message": "token is Bearer abcdefgh12345678",
            "nested": {
                "key": "api_key=secretvalue123"
            }
        });
        let sanitized = fw.sanitize_output(input);
        let obj = sanitized.as_object().unwrap();
        let msg = obj.get("message").unwrap().as_str().unwrap();
        assert!(!msg.contains("abcdefgh12345678"));
        assert!(msg.contains("[REDACTED]"));
        let nested = obj.get("nested").unwrap().as_object().unwrap();
        let key = nested.get("key").unwrap().as_str().unwrap();
        assert!(!key.contains("secretvalue123"));
        assert!(key.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_redacts_bearer_lowercase() {
        let fw = TaintScoringFirewall::new();
        let input = "bearer lowercase_token_12345678";
        let sanitized = fw.sanitize(input);
        assert!(!sanitized.contains("lowercase_token_12345678"));
        assert!(sanitized.contains("[REDACTED]"));
    }
}
