//! Lightweight validator: P2.4 split manifest must enumerate every
//! `#[tokio::test] async fn` in the split gateway_flow integration source files
//! exactly once. The parser is intentionally line-oriented: it assumes the
//! `#[tokio::test]` attribute is immediately adjacent to the `async fn`
//! declaration. It does not perform full AST parsing.

use std::collections::HashSet;
use std::path::PathBuf;

/// Load all Rust source files in `src/gateway_flow` and return them concatenated
/// so that the existing line-oriented test parser can discover every
/// `#[tokio::test] async fn` across the split targets.
fn load_split_source() -> String {
    let source_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/gateway_flow");
    let mut source = String::new();
    for entry in std::fs::read_dir(&source_dir)
        .expect("gateway_flow source directory should exist")
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            source
                .push_str(&std::fs::read_to_string(&path).expect("source file should be readable"));
            source.push('\n');
        }
    }
    source
}

/// Validates that `manifest` enumerates every test discovered in `source`
/// exactly once and that the declared `total_tests` matches both counts.
///
/// The source parser is line-oriented: it expects an immediately preceding
/// `#[tokio::test]` attribute on the line before `async fn <name>`. It does not
/// perform AST parsing.
fn validate_manifest(manifest: &serde_json::Value, source: &str) -> Result<(), String> {
    let manifest_tests = manifest
        .get("tests")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "manifest should have a 'tests' array".to_string())?;

    let manifest_names: HashSet<String> = manifest_tests
        .iter()
        .map(|entry| {
            entry
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "every manifest entry must have a string 'name'".to_string())
        })
        .collect::<Result<HashSet<_>, _>>()?;

    // Detect duplicates explicitly before any HashSet comparison or count drift check.
    if manifest_tests.len() != manifest_names.len() {
        return Err(format!(
            "manifest contains {} entries but only {} unique names; duplicate entries are not allowed",
            manifest_tests.len(),
            manifest_names.len()
        ));
    }

    let total_tests = manifest
        .get("total_tests")
        .and_then(|v| v.as_i64())
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| {
            "manifest should declare a non-negative integer 'total_tests'".to_string()
        })?;

    let mut source_names = HashSet::new();
    let mut previous_line = "";
    for line in source.lines() {
        let trimmed = line.trim();
        if previous_line.trim() == "#[tokio::test]"
            && let Some(body) = trimmed.strip_prefix("async fn ")
        {
            let name = body
                .split('(')
                .next()
                .expect("async fn line has a name")
                .trim();
            source_names.insert(name.to_string());
        }
        previous_line = line;
    }

    if total_tests != manifest_tests.len() {
        return Err(format!(
            "manifest 'total_tests' ({}) must equal manifest entry count ({})",
            total_tests,
            manifest_tests.len()
        ));
    }

    if total_tests != source_names.len() {
        return Err(format!(
            "manifest 'total_tests' ({}) must equal split source #[tokio::test] async fn count ({}); \
             the parser assumes immediately adjacent #[tokio::test] and async fn markers",
            total_tests,
            source_names.len()
        ));
    }

    if manifest_names.len() != source_names.len() {
        return Err(format!(
            "manifest count ({}) must equal split source #[tokio::test] async fn count ({}); \
             check for duplicates or missing entries",
            manifest_names.len(),
            source_names.len()
        ));
    }

    let missing_in_manifest: Vec<&String> = source_names.difference(&manifest_names).collect();
    let missing_in_source: Vec<&String> = manifest_names.difference(&source_names).collect();

    if !missing_in_manifest.is_empty() {
        return Err(format!(
            "split source tests missing from manifest: {:?}",
            missing_in_manifest
        ));
    }

    if !missing_in_source.is_empty() {
        return Err(format!(
            "manifest entries missing from source: {:?}",
            missing_in_source
        ));
    }

    Ok(())
}

#[test]
fn p2_4_manifest_matches_monolith_test_names() {
    let manifest_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baselines/p2.4-split-manifest.json");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("manifest should exist"),
    )
    .expect("manifest should be valid JSON");

    let source = load_split_source();
    validate_manifest(&manifest, &source).expect("manifest should be consistent");
}

#[test]
fn p2_4_manifest_duplicate_entry_fails() {
    let manifest_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baselines/p2.4-split-manifest.json");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("manifest should exist"),
    )
    .expect("manifest should be valid JSON");

    let source = load_split_source();

    let mut dup_manifest = manifest.clone();
    let mut dup_tests = dup_manifest["tests"]
        .as_array()
        .expect("manifest should have a 'tests' array")
        .clone();
    dup_tests.push(dup_tests[0].clone());
    dup_manifest["tests"] = serde_json::Value::Array(dup_tests);

    let err = validate_manifest(&dup_manifest, &source).expect_err("duplicate entry should fail");
    assert!(
        err.contains("duplicate"),
        "error should mention duplicate entries: {}",
        err
    );
}

#[test]
fn p2_4_manifest_total_tests_mismatch_fails() {
    let manifest_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baselines/p2.4-split-manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("manifest should exist"),
    )
    .expect("manifest should be valid JSON");

    let source = load_split_source();

    // Intentionally mismatch total_tests from the actual count.
    manifest["total_tests"] = serde_json::json!(99);

    let err = validate_manifest(&manifest, &source).expect_err("total_tests mismatch should fail");
    assert!(
        err.contains("total_tests") && err.contains("entry count"),
        "error should mention total_tests mismatch: {}",
        err
    );
}
