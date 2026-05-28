use anyhow::{Result, bail};
use ferrum_proto::ActorRef;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct ProvenanceQueryRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    intent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capability_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    until: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReadinessResponse {
    pub status: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ExecutionRecord {
    pub execution_id: String,
    pub proposal_id: String,
    pub intent_id: String,
    pub capability_id: String,
    pub rollback_contract_id: Option<String>,
    pub decision: String,
    pub state: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub result_digest: Option<String>,
}

/// Server wrapper for GET /v1/executions/{id}.
/// The response contains the execution record plus an optional rollback contract.
#[derive(Debug, Deserialize)]
pub struct ExecutionDetailResponse {
    pub execution: ExecutionRecord,
    #[allow(dead_code)]
    pub rollback_contract: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ApprovalRequest {
    pub approval_id: String,
    pub intent_id: String,
    pub proposal_id: String,
    pub execution_id: Option<String>,
    pub requested_by: serde_json::Value,
    pub reason: String,
    pub action_digest: String,
    pub expires_at: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
struct ApprovalListEnvelope {
    items: Vec<ApprovalRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IntentListItem {
    pub intent_id: String,
    pub principal_id: String,
    pub title: String,
    pub status: String,
    pub risk_tier: String,
    pub exec_state: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IntentListEnvelope {
    items: Vec<IntentListItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CancelExecutionResponse {
    pub execution_id: String,
    pub previous_state: String,
    pub current_state: String,
    pub canceled_at: String,
}

// Policy Bundle types
#[derive(Debug, Deserialize, Serialize)]
pub struct PolicyBundleItem {
    pub bundle_id: String,
    pub version: String,
    pub active: bool,
    pub content_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PolicyBundleListResponse {
    pub bundles: Vec<PolicyBundleItem>,
    pub total: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PolicyBundleResponse {
    pub bundle: PolicyBundleItem,
    pub content_hash: String,
}

#[derive(Debug, Serialize)]
struct CreatePolicyBundleRequest {
    yaml_content: String,
}

#[derive(Debug, Serialize)]
struct UpdatePolicyBundleRequest {
    yaml_content: String,
}

#[derive(Debug, Serialize)]
struct SetPolicyBundleActiveRequest {
    active: bool,
}

#[derive(Debug, Serialize)]
pub struct ApprovalResolveRequest {
    pub actor: ActorRef,
    pub approve: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProvenanceEvent {
    pub event_id: String,
    pub kind: String,
    pub occurred_at: String,
    pub actor: serde_json::Value,
    pub object: serde_json::Value,
    pub intent_id: Option<String>,
    pub proposal_id: Option<String>,
    pub execution_id: Option<String>,
    pub capability_id: Option<String>,
    pub rollback_contract_id: Option<String>,
    pub policy_bundle_id: Option<String>,
    pub trust_labels: Vec<String>,
    pub sensitivity_labels: Vec<String>,
    pub parent_edges: Vec<serde_json::Value>,
    pub hash_chain: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ProvenanceQueryResponse {
    events: Vec<ProvenanceEvent>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LineageResponse {
    execution_id: String,
    events: Vec<ProvenanceEvent>,
}

impl LineageResponse {
    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }
    pub fn events(&self) -> &[ProvenanceEvent] {
        &self.events
    }
}

pub struct Client {
    base_url: String,
    bearer_token: Option<String>,
    http: HttpClient,
}

impl Client {
    pub fn new(base_url: String, bearer_token: Option<String>) -> Result<Self> {
        let http = HttpClient::builder().use_rustls_tls().build()?;
        Ok(Self {
            base_url,
            bearer_token,
            http,
        })
    }

    fn add_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ref token) = self.bearer_token {
            req.header("Authorization", format!("Bearer {}", token))
        } else {
            req
        }
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        let url = format!("{}/v1/healthz", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    /// Shallow readiness probe: GET /v1/readyz
    /// No auth required; confirms the HTTP endpoint is reachable.
    pub async fn readiness(&self) -> Result<ReadinessResponse> {
        let url = format!("{}/v1/readyz", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    /// Deep readiness probe: GET /v1/readyz/deep
    /// No auth required; includes store connectivity check.
    pub async fn readiness_deep(&self) -> Result<ReadinessResponse> {
        let url = format!("{}/v1/readyz/deep", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    /// Deep readiness probe returning raw JSON to preserve all server fields.
    pub async fn readiness_deep_json(&self) -> Result<serde_json::Value> {
        let url = format!("{}/v1/readyz/deep", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    /// Functional readiness probe: GET /v1/approvals?limit=1
    /// Requires bearer auth; confirms store, auth, and governance loop are functional.
    pub async fn functional_readiness(&self) -> Result<Vec<ApprovalRequest>> {
        let url = format!("{}/v1/approvals?limit=1", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        let list: ApprovalListEnvelope = resp.json().await?;
        Ok(list.items)
    }

    pub async fn get_execution(&self, execution_id: &str) -> Result<ExecutionRecord> {
        let url = format!("{}/v1/executions/{}", self.base_url, execution_id);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        let detail: ExecutionDetailResponse = resp.json().await?;
        Ok(detail.execution)
    }

    pub async fn list_approvals(&self) -> Result<Vec<ApprovalRequest>> {
        let url = format!("{}/v1/approvals", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        let list: ApprovalListEnvelope = resp.json().await?;
        Ok(list.items)
    }

    pub async fn get_approval(&self, approval_id: &str) -> Result<ApprovalRequest> {
        let url = format!("{}/v1/approvals/{}", self.base_url, approval_id);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn get_lineage(&self, execution_id: &str) -> Result<LineageResponse> {
        let url = format!("{}/v1/provenance/lineage/{}", self.base_url, execution_id);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn query_provenance(
        &self,
        intent_id: Option<&str>,
        execution_id: Option<&str>,
        capability_id: Option<&str>,
    ) -> Result<Vec<ProvenanceEvent>> {
        let url = format!("{}/v1/provenance/query", self.base_url);
        let request = ProvenanceQueryRequest {
            intent_id: intent_id.map(String::from),
            execution_id: execution_id.map(String::from),
            capability_id: capability_id.map(String::from),
            event_kind: None,
            since: None,
            until: None,
        };
        let resp = self
            .add_auth(self.http.post(&url).json(&request))
            .send()
            .await?;
        resp.error_for_status_ref()?;
        let result: ProvenanceQueryResponse = resp.json().await?;
        Ok(result.events)
    }

    pub async fn resolve_approval(
        &self,
        approval_id: &str,
        actor: &ActorRef,
        approve: bool,
        reason: Option<&str>,
    ) -> Result<ApprovalRequest> {
        let url = format!("{}/v1/approvals/{}/resolve", self.base_url, approval_id);
        let request = ApprovalResolveRequest {
            actor: actor.clone(),
            approve,
            reason: reason.map(String::from),
        };
        let resp = self
            .add_auth(self.http.post(&url).json(&request))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if body.is_empty() {
                bail!("HTTP {}: (empty body)", status);
            }
            bail!("HTTP {}: {}", status, body);
        }
        Ok(resp.json().await?)
    }

    pub async fn list_intents(
        &self,
        intent_id: Option<&str>,
        states: &[String],
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Vec<IntentListItem>> {
        let mut url = format!("{}/v1/intents?", self.base_url);
        if let Some(id) = intent_id {
            url.push_str(&format!("intent_id={}&", id));
        }
        for state in states {
            url.push_str(&format!("state={}&", state));
        }
        if let Some(c) = cursor {
            url.push_str(&format!("cursor={}&", c));
        }
        url.push_str(&format!("limit={}", limit));
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        let list: IntentListEnvelope = resp.json().await?;
        Ok(list.items)
    }

    pub async fn cancel_execution(&self, execution_id: &str) -> Result<CancelExecutionResponse> {
        let url = format!("{}/v1/executions/{}/cancel", self.base_url, execution_id);
        let resp = self.add_auth(self.http.post(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    // Policy Bundle client methods
    pub async fn create_policy_bundle(&self, yaml_content: &str) -> Result<PolicyBundleResponse> {
        let url = format!("{}/v1/policy-bundles", self.base_url);
        let request = CreatePolicyBundleRequest {
            yaml_content: yaml_content.to_string(),
        };
        let resp = self
            .add_auth(self.http.post(&url).json(&request))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if body.is_empty() {
                bail!("HTTP {}: (empty body)", status);
            }
            bail!("HTTP {}: {}", status, body);
        }
        Ok(resp.json().await?)
    }

    pub async fn list_policy_bundles(&self) -> Result<PolicyBundleListResponse> {
        let url = format!("{}/v1/policy-bundles", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn get_policy_bundle(&self, bundle_id: &str) -> Result<PolicyBundleResponse> {
        let url = format!("{}/v1/policy-bundles/{}", self.base_url, bundle_id);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn update_policy_bundle(
        &self,
        bundle_id: &str,
        yaml_content: &str,
    ) -> Result<PolicyBundleResponse> {
        let url = format!("{}/v1/policy-bundles/{}", self.base_url, bundle_id);
        let request = UpdatePolicyBundleRequest {
            yaml_content: yaml_content.to_string(),
        };
        let resp = self
            .add_auth(self.http.put(&url).json(&request))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if body.is_empty() {
                bail!("HTTP {}: (empty body)", status);
            }
            bail!("HTTP {}: {}", status, body);
        }
        Ok(resp.json().await?)
    }

    pub async fn delete_policy_bundle(&self, bundle_id: &str) -> Result<serde_json::Value> {
        let url = format!("{}/v1/policy-bundles/{}", self.base_url, bundle_id);
        let resp = self.add_auth(self.http.delete(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn set_policy_bundle_active(
        &self,
        bundle_id: &str,
        active: bool,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/v1/policy-bundles/{}/active", self.base_url, bundle_id);
        let request = SetPolicyBundleActiveRequest { active };
        let resp = self
            .add_auth(self.http.put(&url).json(&request))
            .send()
            .await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    /// Fetch metrics from /v1/metrics endpoint.
    /// Returns the raw Prometheus text format.
    pub async fn metrics(&self) -> Result<String> {
        let url = format!("{}/v1/metrics", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.text().await?)
    }

    /// Simulate a proposal against the active runtime policy.
    /// Side-effect free: no proposal or provenance is persisted.
    pub async fn simulate_runtime_policy(
        &self,
        proposal: &ferrum_proto::ActionProposal,
        intent: Option<&ferrum_proto::IntentEnvelope>,
    ) -> Result<ferrum_proto::EvaluateProposalResponse> {
        let url = format!("{}/v1/policy/simulate", self.base_url);
        let request = ferrum_proto::PolicySimulateRequest {
            proposal: proposal.clone(),
            intent: intent.cloned(),
        };
        let resp = self
            .add_auth(self.http.post(&url).json(&request))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if body.is_empty() {
                bail!("HTTP {}: (empty body)", status);
            }
            bail!("HTTP {}: {}", status, body);
        }
        Ok(resp.json().await?)
    }

    /// Simulate a policy bundle against a sample proposal.
    /// Side-effect free: no proposal, bundle, or provenance is persisted.
    pub async fn simulate_policy_bundle(
        &self,
        bundle_yaml: &str,
        proposal: &ferrum_proto::ActionProposal,
        intent: Option<&ferrum_proto::IntentEnvelope>,
    ) -> Result<ferrum_proto::PolicyBundleSimulateResponse> {
        let url = format!("{}/v1/policy-bundles/simulate", self.base_url);
        let request = ferrum_proto::PolicyBundleSimulateRequest {
            bundle_yaml: bundle_yaml.to_string(),
            proposal: proposal.clone(),
            intent: intent.cloned(),
        };
        let resp = self
            .add_auth(self.http.post(&url).json(&request))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if body.is_empty() {
                bail!("HTTP {}: (empty body)", status);
            }
            bail!("HTTP {}: {}", status, body);
        }
        Ok(resp.json().await?)
    }

    pub async fn list_policy_bundle_versions(
        &self,
        bundle_id: &str,
    ) -> Result<ferrum_proto::ListPolicyBundleVersionsResponse> {
        let url = format!("{}/v1/policy-bundles/{}/versions", self.base_url, bundle_id);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn diff_policy_bundle_versions(
        &self,
        bundle_id: &str,
        from: i64,
        to: i64,
    ) -> Result<ferrum_proto::DiffPolicyBundleVersionsResponse> {
        let url = format!(
            "{}/v1/policy-bundles/{}/diff?from={}&to={}",
            self.base_url, bundle_id, from, to
        );
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn rollback_policy_bundle(
        &self,
        bundle_id: &str,
        target_version: i64,
        actor: Option<&str>,
    ) -> Result<ferrum_proto::RollbackPolicyBundleResponse> {
        let url = format!("{}/v1/policy-bundles/{}/rollback", self.base_url, bundle_id);
        let request = ferrum_proto::RollbackPolicyBundleRequest {
            target_version,
            actor: actor.map(String::from),
        };
        let resp = self
            .add_auth(self.http.post(&url).json(&request))
            .send()
            .await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    // ── Token Admin Methods ──

    pub async fn list_tokens(
        &self,
        actor_id: Option<&str>,
        role: Option<&str>,
        active_only: bool,
        limit: u32,
    ) -> Result<ferrum_proto::TokenListResponse> {
        let mut url = format!("{}/v1/admin/tokens?", self.base_url);
        if let Some(actor_id) = actor_id {
            url.push_str(&format!("actor_id={}&", actor_id));
        }
        if let Some(role) = role {
            url.push_str(&format!("role={}&", role));
        }
        if active_only {
            url.push_str("active_only=true&");
        }
        url.push_str(&format!("limit={}", limit));
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn create_token(
        &self,
        request: &ferrum_proto::CreateTokenRequest,
    ) -> Result<ferrum_proto::CreateTokenResponse> {
        let url = format!("{}/v1/admin/tokens", self.base_url);
        let resp = self
            .add_auth(self.http.post(&url).json(request))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if body.is_empty() {
                bail!("HTTP {}: (empty body)", status);
            }
            bail!("HTTP {}: {}", status, body);
        }
        Ok(resp.json().await?)
    }

    pub async fn revoke_token(&self, token_id: &str, reason: Option<&str>) -> Result<()> {
        let url = format!("{}/v1/admin/tokens/{}", self.base_url, token_id);
        let request = ferrum_proto::RevokeTokenRequest {
            reason: reason.map(String::from),
        };
        let resp = self
            .add_auth(self.http.delete(&url).json(&request))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if body.is_empty() {
                bail!("HTTP {}: (empty body)", status);
            }
            bail!("HTTP {}: {}", status, body);
        }
        Ok(())
    }

    pub async fn rotate_token(
        &self,
        token_id: &str,
        request: &ferrum_proto::RotateTokenRequest,
    ) -> Result<ferrum_proto::CreateTokenResponse> {
        let url = format!("{}/v1/admin/tokens/{}/rotate", self.base_url, token_id);
        let resp = self
            .add_auth(self.http.post(&url).json(request))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if body.is_empty() {
                bail!("HTTP {}: (empty body)", status);
            }
            bail!("HTTP {}: {}", status, body);
        }
        Ok(resp.json().await?)
    }

    // ── Audit Log Methods ──

    pub async fn list_audit_logs(
        &self,
        action: Option<&str>,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<ferrum_proto::AuditLogListResponse> {
        let mut url = format!("{}/v1/admin/audit-logs?", self.base_url);
        if let Some(action) = action {
            url.push_str(&format!("action={}&", action));
        }
        if let Some(resource_type) = resource_type {
            url.push_str(&format!("resource_type={}&", resource_type));
        }
        if let Some(resource_id) = resource_id {
            url.push_str(&format!("resource_id={}&", resource_id));
        }
        if let Some(cursor) = cursor {
            url.push_str(&format!("cursor={}&", cursor));
        }
        url.push_str(&format!("limit={}", limit));
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }
}
