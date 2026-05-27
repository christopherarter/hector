use hector_core::runner::{CheckOptions, HectorEngine};
use hector_core::session_state::{EditRecord, SessionState};
use std::collections::HashSet;
use std::fs;
use tempfile::tempdir;

const CFG: &str = r#"
schema_version: 2
trust:
  fingerprint: PLACEHOLDER
llm:
  provider: claude-code-subagent
rules:
  cross-edit-check:
    description: aggregate review across the session
    engine: session
    scope: ["src/**"]
    severity: error
"#;

fn write_cfg(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join(".hector.yml");
    fs::write(&p, CFG).unwrap();
    let signed = hector_core::trust::write_trust_block(&fs::read_to_string(&p).unwrap()).unwrap();
    fs::write(&p, signed).unwrap();
    p
}

#[test]
fn subagent_session_stop_emits_deferred_envelope() {
    let tmp = tempdir().unwrap();
    let cfg = write_cfg(tmp.path());
    // Create the files so canonicalize() succeeds for scope-matching.
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    let file_a = tmp.path().join("src/a.rs");
    let file_b = tmp.path().join("src/b.rs");
    fs::write(&file_a, "let MARKER_A = 1;\n").unwrap();
    fs::write(&file_b, "let MARKER_B = 2;\n").unwrap();
    let engine = HectorEngine::builder()
        .with_options(CheckOptions {
            rules: HashSet::new(),
            explain: false,
            emit_semantic_payload: true,
            allow_external_paths: false,
        })
        .load(&cfg)
        .unwrap();
    let state = SessionState {
        session_id: "s-test".into(),
        started_at: "2026-05-26T00:00:00Z".into(),
        edits: vec![
            EditRecord {
                file: file_a.to_string_lossy().into(),
                diff: "+let MARKER_A = 1;\n".into(),
                timestamp: "2026-05-26T00:00:01Z".into(),
            },
            EditRecord {
                file: file_b.to_string_lossy().into(),
                diff: "+let MARKER_B = 2;\n".into(),
                timestamp: "2026-05-26T00:00:02Z".into(),
            },
        ],
    };
    let report = engine.check_session_with_options(&state).expect("ok");
    let deferred = report.deferred.expect("deferred envelope for session stop");
    assert_eq!(deferred.payload.evaluate.len(), 1);
    assert_eq!(deferred.payload.evaluate[0].id, "cross-edit-check");
    assert!(
        deferred.payload.diff.contains("src/a.rs") && deferred.payload.diff.contains("MARKER_A"),
        "session-aggregate framing must reference each edit and its diff"
    );
    assert!(
        deferred.payload.diff.contains("src/b.rs") && deferred.payload.diff.contains("MARKER_B"),
    );
    assert_eq!(
        deferred.payload.file, "",
        "session-level deferred envelope has empty `file`"
    );
}

#[test]
fn subagent_session_with_no_in_scope_rules_returns_pass_no_envelope() {
    // When no session rule matches any edit, the CheckReport should be
    // a clean pass — not a deferred envelope. Mirrors the per-file
    // semantic of "no rule → no envelope."
    let tmp = tempdir().unwrap();
    let cfg = write_cfg(tmp.path());
    let engine = HectorEngine::builder()
        .with_options(CheckOptions {
            rules: HashSet::new(),
            explain: false,
            emit_semantic_payload: true,
            allow_external_paths: false,
        })
        .load(&cfg)
        .unwrap();
    let state = SessionState {
        session_id: "s-empty".into(),
        started_at: "2026-05-26T00:00:00Z".into(),
        edits: vec![EditRecord {
            file: tmp.path().join("docs/readme.md").to_string_lossy().into(), // outside src/**
            diff: "+text\n".into(),
            timestamp: "2026-05-26T00:00:01Z".into(),
        }],
    };
    let report = engine.check_session_with_options(&state).expect("ok");
    assert!(
        report.deferred.is_none(),
        "no session rule in scope → no envelope"
    );
}

/// Regression: the deferred path must produce the same per-rule evidence
/// the LLM path would — i.e., the `evaluator_input` for a given rule must
/// only contain edits whose paths match that rule's scope. The previous
/// implementation passed the full unfiltered session aggregate to every
/// rule, leaking out-of-scope edits into the prompt and breaking the
/// "direct-API and subagent see the same evidence" invariant.
#[test]
fn subagent_session_evaluator_input_is_per_rule_scoped() {
    let tmp = tempdir().unwrap();
    // Two session rules with disjoint scopes. An edit under `src/**`
    // must NOT appear in the `docs-rule`'s evaluator window, and vice
    // versa. The doc body must still carry the full session aggregate
    // (payload.diff is the union for operator visibility) — only the
    // per-rule evaluator slice is scoped.
    let cfg = r#"
schema_version: 2
trust:
  fingerprint: PLACEHOLDER
llm:
  provider: claude-code-subagent
rules:
  src-rule:
    description: review src changes
    engine: session
    scope: ["src/**"]
    severity: error
  docs-rule:
    description: review docs changes
    engine: session
    scope: ["docs/**"]
    severity: error
"#;
    let cfg_path = tmp.path().join(".hector.yml");
    fs::write(&cfg_path, cfg).unwrap();
    let signed =
        hector_core::trust::write_trust_block(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    fs::write(&cfg_path, signed).unwrap();

    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::create_dir_all(tmp.path().join("docs")).unwrap();
    let src_file = tmp.path().join("src/code.rs");
    let docs_file = tmp.path().join("docs/readme.md");
    fs::write(&src_file, "let MARKER_SRC = 1;\n").unwrap();
    fs::write(&docs_file, "MARKER_DOCS\n").unwrap();

    let engine = HectorEngine::builder()
        .with_options(CheckOptions {
            rules: HashSet::new(),
            explain: false,
            emit_semantic_payload: true,
            allow_external_paths: false,
        })
        .load(&cfg_path)
        .unwrap();

    let state = SessionState {
        session_id: "s-scope".into(),
        started_at: "2026-05-26T00:00:00Z".into(),
        edits: vec![
            EditRecord {
                file: src_file.to_string_lossy().into(),
                diff: "+let MARKER_SRC = 1;\n".into(),
                timestamp: "2026-05-26T00:00:01Z".into(),
            },
            EditRecord {
                file: docs_file.to_string_lossy().into(),
                diff: "+MARKER_DOCS\n".into(),
                timestamp: "2026-05-26T00:00:02Z".into(),
            },
        ],
    };
    let report = engine.check_session_with_options(&state).expect("ok");
    let deferred = report.deferred.expect("envelope present");

    // Both rules in the envelope.
    assert_eq!(deferred.payload.evaluate.len(), 2);

    // Per-rule scoping invariant: each marker appears exactly once in
    // evaluator_input — only in the user-evidence block of the rule
    // whose scope matches the edit's path. Before the fix, every rule
    // received the full unfiltered aggregate, so each marker appeared
    // twice (once per rule block).
    let input = &deferred.payload.evaluator_input;
    let src_count = input.matches("MARKER_SRC").count();
    let docs_count = input.matches("MARKER_DOCS").count();
    assert_eq!(
        src_count, 1,
        "MARKER_SRC must appear exactly once (only in src-rule's scoped block); got {src_count}\
         \n=== evaluator_input ===\n{input}"
    );
    assert_eq!(
        docs_count, 1,
        "MARKER_DOCS must appear exactly once (only in docs-rule's scoped block); got {docs_count}\
         \n=== evaluator_input ===\n{input}"
    );

    // payload.diff (the operator-visible session aggregate) still carries
    // both edits — the per-rule scoping applies only to evaluator_input.
    assert!(
        deferred.payload.diff.contains("MARKER_SRC")
            && deferred.payload.diff.contains("MARKER_DOCS"),
        "payload.diff must carry the full unfiltered session aggregate"
    );
}
