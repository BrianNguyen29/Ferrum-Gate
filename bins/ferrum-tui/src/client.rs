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

    pub async fn list_approvals(&self) -> Result<ListApprovalsResponse> {
        let url = format!("{}/v1/approvals?limit=20", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.json().await?)
    }

    pub async fn metrics(&self) -> Result<String> {
        let url = format!("{}/v1/metrics", self.base_url);
        let resp = self.add_auth(self.http.get(&url)).send().await?;
        resp.error_for_status_ref()?;
        Ok(resp.text().await?)
    }
}
