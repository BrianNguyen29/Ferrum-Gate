//! Cross-cutting governance macros for the gateway.
//!
//! These macros are used by governance handlers in multiple modules
//! (`server`, `policy`, `approval`, `lineage`) to increment the
//! per-route success/error counters and to format the result.
//!
//! The macros reference `$crate::problem::ApiProblem` and
//! `$crate::response::sanitize_json` via absolute `$crate::` paths so they
//! remain usable from any module in the gateway crate without depending on
//! items in scope at the call site.

/// Macro to increment governance error counter and return an ApiProblem error.
/// Usage (governance route + ApiProblem, increments counter):
///   `governance_err!(state, GovernanceRoute::IntentsCompile, ApiProblem::new(...))`
/// Usage (error code + message, no counter increment, status defaults to BAD_REQUEST):
///   `governance_err!(ApiErrorCode::NotFound, "resource not found")`
///   (use in `ok_or_else(|| governance_err!(...))` or `return Err(governance_err!(...))`)
macro_rules! governance_err {
    ($state:expr, $route:expr, $err:expr) => {{
        $state.metrics.increment_governance_error($route);
        Err($err)
    }};
    ($code:expr, $msg:expr) => {{ $crate::problem::ApiProblem::new(StatusCode::BAD_REQUEST, $code, $msg) }};
}
pub(crate) use governance_err;

/// Macro to increment governance success counter and return an Ok value.
/// Usage: `governance_ok!(state, GovernanceRoute::IntentsCompile, Ok(Json(response)))`
macro_rules! governance_ok {
    ($state:expr, $route:expr, $ok:expr) => {{
        $state.metrics.increment_governance_success($route);
        $ok
    }};
}
pub(crate) use governance_ok;

/// Macro to apply I11 output sanitization, increment the governance success
/// counter for the given route, and return `Ok(axum::Json(value))` where
/// `value` has been re-deserialized from the sanitized JSON form.
///
/// This is the success-path companion of `governance_err!` for handlers that
/// must sanitize their response (the I11 contract: any JSON value that may
/// contain untrusted string content goes through the firewall output
/// sanitizer). For handlers that do not need sanitization, prefer
/// `governance_ok!` to keep the success path explicit.
///
/// On serialization / deserialization failure the error counter is
/// incremented and an `Internal` `ApiProblem` is returned so the success
/// counter is not double-incremented.
#[allow(unused_macros)]
macro_rules! governance_json_ok {
    ($state:expr, $route:expr, $response:expr) => {{
        let json_val = match serde_json::to_value(&$response) {
            Ok(v) => v,
            Err(e) => {
                $state.metrics.increment_governance_error($route);
                return Err($crate::problem::ApiProblem::internal(anyhow::Error::from(
                    e,
                )));
            }
        };
        let sanitized = $crate::response::sanitize_json(&$state.runtime.firewall, json_val);
        let response = match serde_json::from_value(sanitized) {
            Ok(r) => r,
            Err(e) => {
                $state.metrics.increment_governance_error($route);
                return Err($crate::problem::ApiProblem::internal(anyhow::Error::from(
                    e,
                )));
            }
        };
        $state.metrics.increment_governance_success($route);
        Ok(::axum::Json(response))
    }};
}
#[allow(unused_imports)]
pub(crate) use governance_json_ok;

// ---------------------------------------------------------------------------
// I11 Output Sanitization helpers live in `crate::response`. The macro below
// references `$crate::response::sanitize_json` so it remains usable from
// any module without depending on items in scope at the call site.
// ---------------------------------------------------------------------------
