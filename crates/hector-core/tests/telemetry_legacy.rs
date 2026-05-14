use hector_core::telemetry::{read_all, LogEntry};
use hector_core::verdict::Status;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn legacy_log_jsonl_loads_and_lifts_to_typed_variants() {
    let entries = read_all(&fixture_path("log_legacy.jsonl")).expect("legacy fixture must load");
    assert_eq!(entries.len(), 5, "all 5 legacy lines must lift, none dropped");

    // Line 1: kind=check → Check{rules:[]}
    match &entries[0] {
        LogEntry::Check {
            file,
            status,
            rules,
            ..
        } => {
            assert_eq!(file, "src/foo.rs");
            assert_eq!(*status, Status::Pass);
            assert!(rules.is_empty(), "legacy check has no per-rule data");
        }
        other => panic!("entry 0 should be Check, got {other:?}"),
    }

    // Line 3: kind=semantic_skipped → SemanticSkipped
    match &entries[2] {
        LogEntry::SemanticSkipped {
            file, rule, reason, ..
        } => {
            assert_eq!(file, "src/lib.rs");
            assert_eq!(rule, "no-unwrap");
            assert_eq!(reason, "whitespace_only");
        }
        other => panic!("entry 2 should be SemanticSkipped, got {other:?}"),
    }

    // Line 4: kind=skipped → Check{rules:[]}
    match &entries[3] {
        LogEntry::Check { file, rules, .. } => {
            assert_eq!(file, "Cargo.lock");
            assert!(rules.is_empty());
        }
        other => panic!("entry 3 should be Check, got {other:?}"),
    }

    // Line 5: kind=check_session → Check{file:"", rules:[]}
    match &entries[4] {
        LogEntry::Check {
            file,
            status,
            rules,
            ..
        } => {
            assert_eq!(file, "");
            assert_eq!(*status, Status::Block);
            assert!(rules.is_empty());
        }
        other => panic!("entry 4 should be Check, got {other:?}"),
    }
}

#[test]
fn malformed_legacy_line_is_dropped_with_warning() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log.jsonl");
    let body = "\
{\"timestamp\":\"t\",\"kind\":\"check\",\"file\":\"a\",\"rule_id\":null,\"status\":\"pass\",\"elapsed_ms\":1}
{not valid json
{\"timestamp\":\"t\",\"kind\":\"check\",\"file\":\"b\",\"rule_id\":null,\"status\":\"pass\",\"elapsed_ms\":2}
";
    std::fs::write(&log, body).unwrap();
    let entries = read_all(&log).expect("read_all must succeed even with a bad line");
    assert_eq!(
        entries.len(),
        2,
        "the malformed line is dropped, the others survive"
    );
}

#[test]
fn read_all_returns_empty_for_missing_log() {
    let dir = tempfile::tempdir().unwrap();
    let entries =
        read_all(&dir.path().join("nope.jsonl")).expect("missing file is empty, not error");
    assert!(entries.is_empty());
}
