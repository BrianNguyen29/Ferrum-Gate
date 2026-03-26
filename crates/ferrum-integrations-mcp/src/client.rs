//! Thin HTTP client wrapper around the gateway external-event ingest endpoint.
//!
//! Uses reqwest for a one-shot POST to `/v1/provenance/events/external`.
//! No retry logic, no connection pooling configuration, no backoff.

use ferrum_proto::{ExternalEventIngestRequest, ExternalEventIngestResponse};
use reqwest::Client;
use url::Url;

use crate::error::Error;

/// Internal HTTP client that posts external events to the gateway.
#[derive(Debug, Clone)]
pub(crate) struct HttpSink {
    client: Client,
    endpoint: Url,
}

impl HttpSink {
    /// Construct a new sink pointing at the given gateway base URL.
    /// The ingest endpoint path `/v1/provenance/events/external` is appended
    /// automatically.
    pub fn new(gateway_base_url: &str) -> Result<Self, Error> {
        let base: Url = gateway_base_url
            .parse()
            .map_err(|e| Error::validation(format!("invalid gateway base URL: {}", e)))?;

        let endpoint = base
            .join("/v1/provenance/events/external")
            .map_err(|e| Error::validation(format!("failed to build ingest URL: {}", e)))?;

        let client = Client::new();

        Ok(Self { client, endpoint })
    }

    /// POST the given request to the gateway ingest endpoint.
    ///
    /// Returns the parsed `ExternalEventIngestResponse` on success.
    ///
    /// Returns `Error::Gateway` for any non-2xx response, including 404
    /// (execution not found) or 422 (validation error).
    pub async fn post(
        &self,
        request: &ExternalEventIngestRequest,
    ) -> Result<ExternalEventIngestResponse, Error> {
        let response = self
            .client
            .post(self.endpoint.as_str())
            .json(request)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(Error::gateway(status, message));
        }

        let body: ExternalEventIngestResponse = response.json().await?;
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sink_endpoint_construction() {
        let sink = HttpSink::new("http://localhost:8080").unwrap();
        assert_eq!(
            sink.endpoint.as_str(),
            "http://localhost:8080/v1/provenance/events/external"
        );

        let sink2 = HttpSink::new("http://localhost:8080/").unwrap();
        assert_eq!(
            sink2.endpoint.as_str(),
            "http://localhost:8080/v1/provenance/events/external"
        );
    }

    #[test]
    fn test_sink_trailing_slash_normalized() {
        // Absolute path in Url::join replaces the entire path component,
        // so /foo/ + /v1/... -> /v1/... (the trailing /foo/ is replaced)
        let sink = HttpSink::new("http://127.0.0.1:9000/foo/").unwrap();
        assert_eq!(
            sink.endpoint.as_str(),
            "http://127.0.0.1:9000/v1/provenance/events/external"
        );
    }
}
