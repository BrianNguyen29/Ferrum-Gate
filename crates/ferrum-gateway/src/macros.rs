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
