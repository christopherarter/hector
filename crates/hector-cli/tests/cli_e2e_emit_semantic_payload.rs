//! End-to-end coverage that `hector check --emit-semantic-payload`
//! produces the expected envelope on stdout.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

const CONFIG_YAML: &str = r#"
schema_version: 2
trust:
  fingerprint: PLACEHOLDER
llm:
  provider: claude-code-subagent
rules:
  no-debug:
    description: no DEBUG prints in committed code
    engine: semantic
    scope: ["**/*.rs"]
    severity: error
    context: file
"#;

fn write_trusted_config(dir: &std::path::Path) {
    let path = dir.join(".hector.yml");
    fs::write(&path, CONFIG_YAML).unwrap();
    let yaml = fs::read_to_string(&path).unwrap();
    let new = hector_core::trust::write_trust_block(&yaml).unwrap();
    fs::write(&path, new).unwrap();
}

#[test]
fn flag_emits_deferred_verdict_envelope() {
    let tmp = tempdir().unwrap();
    write_trusted_config(tmp.path());
    let src = tmp.path().join("foo.rs");
    fs::write(&src, "fn main() {}\n").unwrap();

    let out = Command::cargo_bin("hector")
        .unwrap()
        .arg("check")
        .arg("--config")
        .arg(tmp.path().join(".hector.yml"))
        .arg("--file")
        .arg(&src)
        .arg("--emit-semantic-payload")
        .arg("--format")
        .arg("json")
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("stdout must be valid JSON, got: {stdout}"));
    assert_eq!(v["deferred"], serde_json::Value::Bool(true));
    assert_eq!(v["schema_version"], serde_json::Value::Number(3.into()));
    assert_eq!(v["payload"]["evaluate"][0]["id"].as_str(), Some("no-debug"));
    assert!(v["payload"]["_evaluator_input"]
        .as_str()
        .unwrap()
        .contains("no-debug"));
}

#[test]
fn single_file_diff_with_default_context_emits_envelope() {
    // A semantic rule with NO `context:` defaults to `context: diff`. The
    // Claude Code PostToolUse hook records a synthesized diff, so the gate
    // must be able to run `--diff --emit-semantic-payload` and build a
    // deferred envelope whose evaluator_input carries the diff body.
    let tmp = tempdir().unwrap();
    // No `context:` line → defaults to diff.
    let cfg = "schema_version: 2\n\
trust:\n  \
fingerprint: PLACEHOLDER\n\
llm:\n  \
provider: claude-code-subagent\n\
rules:\n  \
no-debug:\n    \
description: no DEBUG prints in committed code\n    \
engine: semantic\n    \
scope: [\"**/*.rs\"]\n    \
severity: error\n";
    let path = tmp.path().join(".hector.yml");
    fs::write(&path, cfg).unwrap();
    let yaml = fs::read_to_string(&path).unwrap();
    let new = hector_core::trust::write_trust_block(&yaml).unwrap();
    fs::write(&path, new).unwrap();

    // Diff mode reads the file from disk to resolve it, so it must exist.
    let src = tmp.path().join("foo.rs");
    fs::write(&src, "fn main() {\n}\n").unwrap();

    // A single-file, repo-relative unified diff.
    let diff =
        "--- a/foo.rs\n+++ b/foo.rs\n@@ -1,2 +1,3 @@\n fn main() {\n+    let MARKER_DIFF = 1;\n }\n";
    let diff_path = tmp.path().join("change.diff");
    fs::write(&diff_path, diff).unwrap();

    let out = Command::cargo_bin("hector")
        .unwrap()
        .arg("check")
        .arg("--config")
        .arg(&path)
        .arg("--diff")
        .arg(&diff_path)
        .arg("--emit-semantic-payload")
        .arg("--format")
        .arg("json")
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("stdout must be the deferred envelope JSON, got: {stdout}"));
    assert_eq!(v["deferred"], serde_json::Value::Bool(true));
    assert_eq!(v["payload"]["evaluate"][0]["id"].as_str(), Some("no-debug"));
    // context: diff (default) → the diff body is the evaluator evidence.
    assert!(
        v["payload"]["_evaluator_input"]
            .as_str()
            .unwrap()
            .contains("MARKER_DIFF"),
        "default context: diff must put the diff body in evaluator_input; got: {stdout}"
    );
}

#[test]
fn single_file_diff_deterministic_block_suppresses_envelope() {
    // Diff-path analog of `deterministic_block_suppresses_deferred_envelope`:
    // a script rule that blocks plus a deferred semantic rule, run via
    // `--diff --emit-semantic-payload`. The deterministic block wins —
    // exit 2, no envelope on stdout.
    let tmp = tempdir().unwrap();
    let cfg = r#"
schema_version: 2
trust:
  fingerprint: PLACEHOLDER
llm:
  provider: claude-code-subagent
rules:
  no-debug-script:
    description: no DEBUG via grep
    engine: script
    scope: ["**/*.rs"]
    severity: error
    script: "grep -n 'DEBUG' {file} && exit 1 || exit 0"
    capabilities:
      network: false
      writes: none
  no-debug-semantic:
    description: no DEBUG prints in committed code
    engine: semantic
    scope: ["**/*.rs"]
    severity: error
"#;
    let path = tmp.path().join(".hector.yml");
    fs::write(&path, cfg).unwrap();
    let yaml = fs::read_to_string(&path).unwrap();
    let new = hector_core::trust::write_trust_block(&yaml).unwrap();
    fs::write(&path, new).unwrap();
    let src = tmp.path().join("foo.rs");
    fs::write(&src, "fn main() { println!(\"DEBUG\"); }\n").unwrap();
    let diff = "--- a/foo.rs\n+++ b/foo.rs\n@@ -1 +1 @@\n+fn main() { println!(\"DEBUG\"); }\n";
    let diff_path = tmp.path().join("change.diff");
    fs::write(&diff_path, diff).unwrap();

    let out = Command::cargo_bin("hector")
        .unwrap()
        .arg("check")
        .arg("--config")
        .arg(&path)
        .arg("--diff")
        .arg(&diff_path)
        .arg("--emit-semantic-payload")
        .arg("--format")
        .arg("json")
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        v.get("deferred").is_none(),
        "deterministic block must suppress the deferred envelope: {stdout}"
    );
    assert_eq!(v["status"].as_str(), Some("block"));
}

#[test]
fn multi_file_diff_with_emit_semantic_payload_is_rejected() {
    // Merging several files' deferred rules into one envelope is a follow-up.
    // A diff touching two files plus --emit-semantic-payload must reject
    // explicitly rather than silently dropping all-but-one file's rules.
    let tmp = tempdir().unwrap();
    write_trusted_config(tmp.path());
    let diff = "--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-x\n+fn a() {}\n\
--- a/b.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-x\n+fn b() {}\n";
    let diff_path = tmp.path().join("multi.diff");
    fs::write(&diff_path, diff).unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .arg("check")
        .arg("--config")
        .arg(tmp.path().join(".hector.yml"))
        .arg("--diff")
        .arg(&diff_path)
        .arg("--emit-semantic-payload")
        .arg("--format")
        .arg("json")
        .assert()
        .code(1)
        .stderr(contains("single-file"));
}

#[test]
fn flag_rejects_combined_with_session() {
    let tmp = tempdir().unwrap();
    write_trusted_config(tmp.path());
    Command::cargo_bin("hector")
        .unwrap()
        .arg("check")
        .arg("--config")
        .arg(tmp.path().join(".hector.yml"))
        .arg("--session")
        .arg("--emit-semantic-payload")
        .assert()
        .failure()
        .stderr(contains("cannot be used with"));
}

#[test]
fn flag_omitted_means_no_envelope() {
    // Sanity: without the flag, the CLI emits the standard Verdict shape,
    // not the DeferredVerdict envelope. Asserts the additive nature of the
    // change — no behaviour drift for existing call-sites.
    let tmp = tempdir().unwrap();
    // Use a non-subagent provider so direct-dispatch is attempted but the
    // missing API key makes semantic skip silently. `model:` is mandatory
    // for direct-API providers (the subagent stanza omits it), so splice a
    // model field back in here.
    let cfg = CONFIG_YAML
        .replace("claude-code-subagent", "anthropic")
        .replace(
            "provider: anthropic",
            "provider: anthropic\n  model: claude-sonnet-4-6",
        );
    let path = tmp.path().join(".hector.yml");
    fs::write(&path, cfg).unwrap();
    let yaml = fs::read_to_string(&path).unwrap();
    let new = hector_core::trust::write_trust_block(&yaml).unwrap();
    fs::write(&path, new).unwrap();
    let src = tmp.path().join("foo.rs");
    fs::write(&src, "fn main() {}\n").unwrap();

    let out = Command::cargo_bin("hector")
        .unwrap()
        .env_remove("ANTHROPIC_API_KEY")
        .arg("check")
        .arg("--config")
        .arg(&path)
        .arg("--file")
        .arg(&src)
        .arg("--format")
        .arg("json")
        .assert()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(v.get("deferred").is_none(), "no flag, no envelope");
    assert!(
        v.get("status").is_some(),
        "standard Verdict has status field"
    );
}

#[test]
fn deterministic_block_suppresses_deferred_envelope() {
    // A script rule that exits non-zero (block) plus a semantic rule
    // that would be deferred. The expected behaviour: the script
    // violation is the verdict, exit 2; no DeferredVerdict on stdout.
    let tmp = tempdir().unwrap();
    let cfg = r#"
schema_version: 2
trust:
  fingerprint: PLACEHOLDER
llm:
  provider: claude-code-subagent
rules:
  no-debug-script:
    description: no DEBUG via grep
    engine: script
    scope: ["**/*.rs"]
    severity: error
    script: "grep -n 'DEBUG' {file} && exit 1 || exit 0"
    capabilities:
      network: false
      writes: none
  no-debug-semantic:
    description: no DEBUG prints in committed code
    engine: semantic
    scope: ["**/*.rs"]
    severity: error
    context: file
"#;
    let path = tmp.path().join(".hector.yml");
    fs::write(&path, cfg).unwrap();
    let yaml = fs::read_to_string(&path).unwrap();
    let new = hector_core::trust::write_trust_block(&yaml).unwrap();
    fs::write(&path, new).unwrap();
    let src = tmp.path().join("foo.rs");
    fs::write(&src, "fn main() { println!(\"DEBUG\"); }\n").unwrap();

    let out = Command::cargo_bin("hector")
        .unwrap()
        .arg("check")
        .arg("--config")
        .arg(&path)
        .arg("--file")
        .arg(&src)
        .arg("--emit-semantic-payload")
        .arg("--format")
        .arg("json")
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        v.get("deferred").is_none(),
        "block suppresses deferred envelope"
    );
    assert_eq!(v["status"].as_str(), Some("block"));

    // The deferred semantic rule must surface as a `deferred_rules` entry on
    // the verdict so the interpreter skill can show the user the rule was
    // configured (just not evaluated this turn) rather than vanishing
    // silently — the worst failure mode for a policy tool.
    let deferred = v["deferred_rules"]
        .as_array()
        .expect("deferred_rules must be present and an array on a blocked verdict");
    assert_eq!(deferred.len(), 1);
    assert_eq!(deferred[0]["rule_id"].as_str(), Some("no-debug-semantic"));
    assert_eq!(deferred[0]["severity"].as_str(), Some("error"));
    assert!(deferred[0]["reason"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));

    // Additive fields (skip_serializing_if) do NOT bump SCHEMA_VERSION;
    // schema_version stays 2.
    assert_eq!(v["schema_version"], serde_json::Value::Number(2.into()));
}
