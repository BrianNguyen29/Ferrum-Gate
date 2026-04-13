//! Focused unit tests for H1.1d PolicyBundleRegisterRequest authoring commands.
//!
//! Tests:
//! - `run_author_request_generate` — template generation
//! - `run_author_request_validate` — parsing and validation
//! - `run_author_request_bump` — semantic version bumping
//!
//! These tests use the actual file I/O patterns but don't require a running server.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: invoke ferrumctl via process
// ---------------------------------------------------------------------------

fn ferrumctl() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ferrumctl"));
    cmd.env_remove("FERRUMCTL_SERVER_URL");
    cmd.env_remove("FERRUMCTL_BEARER_TOKEN");
    cmd
}

// ---------------------------------------------------------------------------
// Tests: author request generate
// ---------------------------------------------------------------------------

#[test]
fn test_author_request_generate_yaml_default() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("request.yaml");

    let output = ferrumctl()
        .args(["author", "request", "generate", "--output"])
        .arg(&output_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&output_path).expect("failed to read output");
    assert!(content.contains("name: \"my-policy-bundle\""));
    assert!(content.contains("version: \"0.1.0\""));
    assert!(content.contains("# NOTE: fingerprint is intentionally omitted"));
}

#[test]
fn test_author_request_generate_yaml_with_outcomes() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("request.yaml");

    let output = ferrumctl()
        .args([
            "author",
            "request",
            "generate",
            "--output",
            output_path.to_str().unwrap(),
            "--with-outcomes",
        ])
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&output_path).expect("failed to read output");
    assert!(content.contains("allowed_outcomes:"));
    assert!(content.contains("forbidden_outcomes:"));
}

#[test]
fn test_author_request_generate_yaml_with_supersedes() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("request.yaml");
    let supersedes_id = "550e8400-e29b-41d4-a716-446655440000";

    let output = ferrumctl()
        .args([
            "author",
            "request",
            "generate",
            "--output",
            output_path.to_str().unwrap(),
            "--supersedes",
            supersedes_id,
        ])
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&output_path).expect("failed to read output");
    assert!(content.contains(&format!("supersedes_bundle_id: \"{}\"", supersedes_id)));
}

#[test]
fn test_author_request_generate_json_format() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("request.json");

    let output = ferrumctl()
        .args([
            "author",
            "request",
            "generate",
            "--output",
            output_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&output_path).expect("failed to read output");
    // JSON should parse correctly
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("output is not valid JSON");
    assert_eq!(parsed["name"], "my-policy-bundle");
}

#[test]
fn test_author_request_generate_stdout() {
    let output = ferrumctl()
        .args(["author", "request", "generate"])
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Should write to stdout (stderr should be empty)
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("name:"));
}

// ---------------------------------------------------------------------------
// Tests: author request validate
// ---------------------------------------------------------------------------

#[test]
fn test_author_request_validate_valid_yaml() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.yaml");

    // Write a valid request file
    fs::write(
        &request_path,
        r#"name: "test-bundle"
description: "Test bundle"
version: "1.0.0"
"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "validate"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("validation PASSED"));
}

#[test]
fn test_author_request_validate_valid_json() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.json");

    // Write a valid request file in JSON
    fs::write(
        &request_path,
        r#"{
  "name": "test-bundle",
  "description": "Test bundle",
  "version": "1.0.0"
}"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "validate", "--json"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("validation PASSED"));
}

#[test]
fn test_author_request_validate_missing_name() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.yaml");

    // Write a request file without name
    fs::write(
        &request_path,
        r#"description: "Test bundle"
version: "1.0.0"
"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "validate"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("validation FAILED"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("name is required"));
}

#[test]
fn test_author_request_validate_placeholder_name() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.yaml");

    // Write a request file with placeholder name
    fs::write(
        &request_path,
        r#"name: "<describe what this policy bundle governs>"
description: "Test bundle"
version: "1.0.0"
"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "validate"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("validation FAILED"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("placeholder"));
}

#[test]
fn test_author_request_validate_invalid_effect_type() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.yaml");

    // Write a request file with invalid effect_type
    fs::write(
        &request_path,
        r#"name: "test-bundle"
description: "Test bundle"
version: "1.0.0"
allowed_outcomes:
  - id: "test"
    description: "Test"
    effect_type: "InvalidEffect"
    required: true
"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "validate"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("validation FAILED"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("effect_type"));
}

#[test]
fn test_author_request_validate_invalid_supersedes_uuid() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.yaml");

    // Write a request file with invalid supersedes_bundle_id
    fs::write(
        &request_path,
        r#"name: "test-bundle"
description: "Test bundle"
version: "1.0.0"
supersedes_bundle_id: "not-a-valid-uuid"
"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "validate"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("validation FAILED"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("supersedes_bundle_id"));
}

// ---------------------------------------------------------------------------
// Tests: author request bump
// ---------------------------------------------------------------------------

#[test]
fn test_author_request_bump_patch() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.yaml");

    fs::write(
        &request_path,
        r#"name: "test-bundle"
description: "Test bundle"
version: "1.2.3"
"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "bump", "--part", "patch"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&request_path).expect("failed to read file");
    assert!(content.contains("version: 1.2.4"));
}

#[test]
fn test_author_request_bump_minor() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.yaml");

    fs::write(
        &request_path,
        r#"name: "test-bundle"
description: "Test bundle"
version: "1.2.3"
"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "bump", "--part", "minor"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&request_path).expect("failed to read file");
    assert!(content.contains("version: 1.3.0"));
}

#[test]
fn test_author_request_bump_major() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.yaml");

    fs::write(
        &request_path,
        r#"name: "test-bundle"
description: "Test bundle"
version: "1.2.3"
"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "bump", "--part", "major"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&request_path).expect("failed to read file");
    assert!(content.contains("version: 2.0.0"));
}

#[test]
fn test_author_request_bump_to_output_file() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("input.yaml");
    let output_path = temp_dir.path().join("output.yaml");

    fs::write(
        &input_path,
        r#"name: test-bundle
description: Test bundle
version: 1.2.3
"#,
    )
    .expect("failed to write input file");

    let output = ferrumctl()
        .args([
            "author",
            "request",
            "bump",
            "--part",
            "patch",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .arg(&input_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(
        output.status.success(),
        "ferrumctl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Input file should be unchanged
    let input_content = fs::read_to_string(&input_path).expect("failed to read input");
    assert!(input_content.contains("version: 1.2.3"));

    // Output file should have bumped version
    let output_content = fs::read_to_string(&output_path).expect("failed to read output");
    assert!(output_content.contains("version: 1.2.4"));
}

#[test]
fn test_author_request_bump_invalid_semver() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let request_path = temp_dir.path().join("request.yaml");

    fs::write(
        &request_path,
        r#"name: "test-bundle"
description: "Test bundle"
version: "not-semver"
"#,
    )
    .expect("failed to write request file");

    let output = ferrumctl()
        .args(["author", "request", "bump", "--part", "patch"])
        .arg(&request_path)
        .output()
        .expect("failed to spawn ferrumctl");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("semver"));
}

// ---------------------------------------------------------------------------
// Tests: register-policy-bundle --request-file
// ---------------------------------------------------------------------------

#[test]
fn test_register_policy_bundle_request_file_flag_exists() {
    // Just verify the --request-file flag is recognized
    let output = ferrumctl()
        .args([
            "server",
            "register-policy-bundle",
            "--request-file",
            "/nonexistent/path.yaml",
        ])
        .output()
        .expect("failed to spawn ferrumctl");

    // Should fail with parse error or file not found, not "unrecognized argument"
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unrecognized argument") && !stderr.contains("unknown flag"),
        "--request-file flag not recognized: {}",
        stderr
    );
}
