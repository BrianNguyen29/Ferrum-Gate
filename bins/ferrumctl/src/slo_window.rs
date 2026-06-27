use anyhow::{Context, Result, bail};
use std::path::PathBuf;

/// Local state for an SLO evidence window.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SloWindowState {
    window_id: String,
    status: String,
    window_started_at: chrono::DateTime<chrono::Utc>,
    window_ended_at: Option<chrono::DateTime<chrono::Utc>>,
    elapsed_duration_seconds: u64,
    target_duration_days: u32,
    minimum_duration_days: u32,
    notes: Option<String>,
    non_claims_notice: String,
    created_by_tool: String,
    finalized_by_tool: Option<String>,
}

impl SloWindowState {
    /// Generate a non-claims notice string.
    pub fn default_non_claims_notice() -> String {
        [
            "Sustained SLO window = NOT COMPLETE",
            "production-ready = NO",
            "Tier 2 = NOT COMPLETE",
            "This record tracks lifecycle only; it does not certify SLO achievement.",
        ]
        .join("\n")
    }

    /// Create a new started window state.
    pub fn start_now(notes: Option<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            window_id: format!("slo-window-{}", now.format("%Y%m%dT%H%M%SZ")),
            status: "started".to_string(),
            window_started_at: now,
            window_ended_at: None,
            elapsed_duration_seconds: 0,
            target_duration_days: 30,
            minimum_duration_days: 7,
            notes,
            non_claims_notice: Self::default_non_claims_notice(),
            created_by_tool: "ferrumctl evidence slo-window start".to_string(),
            finalized_by_tool: None,
        }
    }

    /// Compute elapsed seconds since start.
    pub fn recompute_elapsed(&mut self) {
        let now = chrono::Utc::now();
        let end = self.window_ended_at.unwrap_or(now);
        let dur = end.signed_duration_since(self.window_started_at);
        self.elapsed_duration_seconds = dur.num_seconds().max(0) as u64;
    }

    /// Finalize the window, returning the updated state.
    pub fn finalize(&mut self, notes: Option<String>, allow_early: bool) -> Result<()> {
        if self.status == "finalized" {
            // Idempotent: already finalized
            return Ok(());
        }
        self.recompute_elapsed();
        let min_secs = (self.minimum_duration_days as i64) * 24 * 60 * 60;
        if (self.elapsed_duration_seconds as i64) < min_secs && !allow_early {
            bail!(
                "window has run for {} seconds ({} days); minimum is {} days. Use --allow-early to override",
                self.elapsed_duration_seconds,
                self.elapsed_duration_seconds / 86400,
                self.minimum_duration_days
            );
        }
        self.status = "finalized".to_string();
        self.window_ended_at = Some(chrono::Utc::now());
        self.finalized_by_tool = Some("ferrumctl evidence slo-window finalize".to_string());
        if let Some(n) = notes {
            self.notes = Some(n);
        }
        self.recompute_elapsed();
        Ok(())
    }
}

pub fn slo_window_state_path(window_dir: Option<PathBuf>) -> PathBuf {
    window_dir
        .unwrap_or_else(|| PathBuf::from("."))
        .join("slo-window-state.json")
}

pub fn read_slo_window_state(path: &PathBuf) -> Result<SloWindowState> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read state file {}", path.display()))?;
    let state: SloWindowState = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse state file {}", path.display()))?;
    Ok(state)
}

pub fn write_slo_window_state(path: &PathBuf, state: &SloWindowState) -> Result<()> {
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(path, json)
        .with_context(|| format!("failed to write state file {}", path.display()))?;
    Ok(())
}

pub fn print_slo_window_status(state: &SloWindowState) {
    println!("window_id:    {}", state.window_id);
    println!("status:       {}", state.status);
    println!("started_at:   {}", state.window_started_at.to_rfc3339());
    if let Some(end) = state.window_ended_at {
        println!("ended_at:     {}", end.to_rfc3339());
    }
    println!(
        "elapsed:      {} seconds (~{} days)",
        state.elapsed_duration_seconds,
        state.elapsed_duration_seconds / 86400
    );
    println!("target:       {} days", state.target_duration_days);
    println!("minimum:      {} days", state.minimum_duration_days);
    if let Some(ref n) = state.notes {
        println!("notes:        {}", n);
    }
    println!("created_by:   {}", state.created_by_tool);
    if let Some(ref f) = state.finalized_by_tool {
        println!("finalized_by: {}", f);
    }
    println!("--- non-claims ---");
    for line in state.non_claims_notice.lines() {
        println!("{}", line);
    }
}

/// Run `slo-window start`.
pub fn run_slo_window_start(window_dir: Option<PathBuf>, notes: Option<String>) -> Result<()> {
    let path = slo_window_state_path(window_dir);
    if path.exists() {
        let existing = read_slo_window_state(&path)?;
        if existing.status == "started" {
            bail!(
                "an active window already exists at {} (window_id: {}). Use `finalize` first or choose a different --window-dir.",
                path.display(),
                existing.window_id
            );
        }
    }
    let state = SloWindowState::start_now(notes);
    write_slo_window_state(&path, &state)?;
    println!("SLO window started: {}", state.window_id);
    println!("State file: {}", path.display());
    Ok(())
}

/// Run `slo-window status`.
pub fn run_slo_window_status(window_dir: Option<PathBuf>, json: bool) -> Result<()> {
    let path = slo_window_state_path(window_dir);
    let mut state = read_slo_window_state(&path)?;
    state.recompute_elapsed();
    if json {
        println!("{}", serde_json::to_string_pretty(&state)?);
    } else {
        print_slo_window_status(&state);
    }
    Ok(())
}

/// Run `slo-window finalize`.
pub fn run_slo_window_finalize(
    window_dir: Option<PathBuf>,
    notes: Option<String>,
    allow_early: bool,
) -> Result<()> {
    let path = slo_window_state_path(window_dir);
    let mut state = read_slo_window_state(&path)?;
    state.finalize(notes, allow_early)?;
    write_slo_window_state(&path, &state)?;
    println!("SLO window finalized: {}", state.window_id);
    println!(
        "Elapsed: {} seconds (~{} days)",
        state.elapsed_duration_seconds,
        state.elapsed_duration_seconds / 86400
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // SLO window state serialization / roundtrip tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_slo_window_state_serialization_roundtrip() {
        let state = SloWindowState {
            window_id: "slo-window-test".to_string(),
            status: "started".to_string(),
            window_started_at: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            window_ended_at: None,
            elapsed_duration_seconds: 0,
            target_duration_days: 30,
            minimum_duration_days: 7,
            notes: Some("test note".to_string()),
            non_claims_notice: SloWindowState::default_non_claims_notice(),
            created_by_tool: "ferrumctl evidence slo-window start".to_string(),
            finalized_by_tool: None,
        };
        let json = serde_json::to_string_pretty(&state).unwrap();
        let restored: SloWindowState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, restored);
    }

    #[test]
    fn test_slo_window_state_recompute_elapsed() {
        let past = chrono::Utc::now() - chrono::Duration::hours(5);
        let mut state = SloWindowState {
            window_id: "test".to_string(),
            status: "started".to_string(),
            window_started_at: past,
            window_ended_at: None,
            elapsed_duration_seconds: 0,
            target_duration_days: 30,
            minimum_duration_days: 7,
            notes: None,
            non_claims_notice: SloWindowState::default_non_claims_notice(),
            created_by_tool: "tool".to_string(),
            finalized_by_tool: None,
        };
        state.recompute_elapsed();
        // Should be roughly 5 hours = 18000 seconds, allow +/- 5 seconds for test execution time
        assert!(
            state.elapsed_duration_seconds >= 18000 - 5
                && state.elapsed_duration_seconds <= 18000 + 60
        );
    }

    // -------------------------------------------------------------------------
    // SLO window non-claims tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_slo_window_non_claims_notice_contains_required_lines() {
        let notice = SloWindowState::default_non_claims_notice();
        assert!(notice.contains("Sustained SLO window = NOT COMPLETE"));
        assert!(notice.contains("production-ready = NO"));
        assert!(notice.contains("Tier 2 = NOT COMPLETE"));
        assert!(notice.contains("does not certify SLO achievement"));
    }

    #[test]
    fn test_slo_window_state_contains_non_claims_in_json() {
        let state = SloWindowState::start_now(None);
        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(json.contains("non_claims_notice"));
        assert!(json.contains("NOT COMPLETE"));
        assert!(json.contains("production-ready = NO"));
    }

    #[test]
    fn test_slo_window_no_unqualified_overclaims() {
        let state = SloWindowState::start_now(None);
        let json = serde_json::to_string_pretty(&state).unwrap();
        let forbidden = [
            "\"production_ready\": true",
            "\"tier_2\": true",
            "\"tier2\": true",
            "\"ga\": true",
            "\"enterprise_ready\": true",
            "\"compliance\": true",
            "\"slo_proof\": true",
        ];
        for term in &forbidden {
            assert!(
                !json.to_lowercase().contains(&term.to_lowercase()),
                "state must not contain unqualified overclaim: {}",
                term
            );
        }
    }

    // -------------------------------------------------------------------------
    // SLO window lifecycle tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_slo_window_start_creates_state_file() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), Some("test".to_string())).unwrap();
        let path = dir.path().join("slo-window-state.json");
        assert!(path.exists());
        let state = read_slo_window_state(&path).unwrap();
        assert_eq!(state.status, "started");
        assert_eq!(state.notes, Some("test".to_string()));
        assert!(state.created_by_tool.contains("slo-window start"));
    }

    #[test]
    fn test_slo_window_start_refuses_active_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        let result = run_slo_window_start(Some(dir.path().to_path_buf()), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("active window already exists"));
    }

    #[test]
    fn test_slo_window_status_reads_state() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        run_slo_window_status(Some(dir.path().to_path_buf()), true).unwrap();
    }

    #[test]
    fn test_slo_window_status_missing_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_slo_window_status(Some(dir.path().to_path_buf()), true);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed to read state file"));
    }

    #[test]
    fn test_slo_window_finalize_early_rejected_without_allow_early() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        let result = run_slo_window_finalize(Some(dir.path().to_path_buf()), None, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("minimum is 7 days"));
        assert!(err.contains("--allow-early"));
    }

    #[test]
    fn test_slo_window_finalize_early_allowed_with_flag() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        run_slo_window_finalize(Some(dir.path().to_path_buf()), None, true).unwrap();
        let path = dir.path().join("slo-window-state.json");
        let state = read_slo_window_state(&path).unwrap();
        assert_eq!(state.status, "finalized");
        assert!(
            state
                .finalized_by_tool
                .as_ref()
                .unwrap()
                .contains("slo-window finalize")
        );
    }

    #[test]
    fn test_slo_window_finalize_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        run_slo_window_finalize(Some(dir.path().to_path_buf()), None, true).unwrap();
        // Second finalize should succeed (idempotent)
        run_slo_window_finalize(
            Some(dir.path().to_path_buf()),
            Some("again".to_string()),
            true,
        )
        .unwrap();
        let path = dir.path().join("slo-window-state.json");
        let state = read_slo_window_state(&path).unwrap();
        assert_eq!(state.status, "finalized");
    }

    #[test]
    fn test_slo_window_finalize_updates_notes() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), Some("first".to_string())).unwrap();
        run_slo_window_finalize(
            Some(dir.path().to_path_buf()),
            Some("final note".to_string()),
            true,
        )
        .unwrap();
        let path = dir.path().join("slo-window-state.json");
        let state = read_slo_window_state(&path).unwrap();
        assert_eq!(state.notes, Some("final note".to_string()));
    }
}
