use anyhow::Result;
use reqwest::Client as HttpClient;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ReadinessResponse {
    pub status: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApprovalRequest {
    pub approval_id: String,
    pub proposal_id: String,
    pub requested_by: serde_json::Value,
    pub reason: String,
    pub state: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ListApprovalsResponse {
    pub items: Vec<ApprovalRequest>,
    pub next_cursor: Option<String>,
}

/// Rows shown per approvals page. One extra row is requested (limit + 1) to
/// detect whether a further page exists; the extra row is trimmed.
pub const APPROVALS_PAGE_SIZE: u32 = 50;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AuditVerifyResult {
    pub valid: bool,
    pub total_entries: usize,
    pub hashed_entries: usize,
    pub error: Option<String>,
}

pub struct Client {
    base_url: String,
    bearer_token: Option<String>,
    http: HttpClient,
}

impl Client {
    pub fn new(base_url: String, bearer_token: Option<String>) -> Result<Self> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(8))
            .use_rustls_tls()
            .build()?;
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

    pub async fn readiness(&self) -> Result<ReadinessResponse> {
        let url = format!("{}/v1/readyz", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn readiness_deep(&self) -> Result<ReadinessResponse> {
        let url = format!("{}/v1/readyz/deep", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    /// Fetch one page (0-based) of pending approvals. The server's offset
    /// path never returns `next_cursor`, so paging uses `offset` and detects
    /// more pages by over-fetching one row.
    pub async fn list_approvals_page(&self, page: usize) -> Result<(Vec<ApprovalRequest>, bool)> {
        let offset = APPROVALS_PAGE_SIZE.saturating_mul(page as u32);
        let url = format!(
            "{}/v1/approvals?limit={}&offset={}",
            self.base_url,
            APPROVALS_PAGE_SIZE + 1,
            offset
        );
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        let body: ListApprovalsResponse = resp.json().await?;
        let has_more = body.items.len() > APPROVALS_PAGE_SIZE as usize;
        let mut items = body.items;
        items.truncate(APPROVALS_PAGE_SIZE as usize);
        Ok((items, has_more))
    }

    pub async fn metrics(&self) -> Result<String> {
        let url = format!("{}/v1/metrics", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.text().await?)
    }

    pub async fn verify_audit_chain(&self) -> Result<AuditVerifyResult> {
        let url = format!("{}/v1/admin/audit/verify", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }
}
