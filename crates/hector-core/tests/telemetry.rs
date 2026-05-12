use hector_core::telemetry::{append, LogEntry};
use tempfile::tempdir;

#[test]
fn append_creates_log_and_writes_jsonl() {
    let dir = tempdir().unwrap();
    let log = dir.path().join(".hector/log.jsonl");
    let entry = LogEntry {
        timestamp: "2026-05-11T18:00:00Z".into(),
        kind: "check".into(),
        file: "src/foo.rs".into(),
        rule_id: None,
        status: "pass".into(),
        elapsed_ms: 12,
    };
    append(&log, &entry).unwrap();
    let content = std::fs::read_to_string(&log).unwrap();
    assert!(content.contains("\"kind\":\"check\""));
    assert!(content.contains("\"src/foo.rs\""));

    let entry2 = LogEntry {
        timestamp: "2026-05-11T18:00:05Z".into(),
        kind: "check".into(),
        file: "src/bar.rs".into(),
        rule_id: None,
        status: "block".into(),
        elapsed_ms: 22,
    };
    append(&log, &entry2).unwrap();
    let content = std::fs::read_to_string(&log).unwrap();
    let lines: Vec<_> = content.lines().collect();
    assert_eq!(lines.len(), 2);
}
