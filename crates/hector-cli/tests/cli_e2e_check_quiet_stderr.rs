//! R7: routine `hector check` invocations must stay quiet on stderr.
//!
//! Pre-R7 the macOS capability sandbox emitted a per-process
//! "capability enforcement is best-effort on this platform" advisory on
//! every script-rule run. Even with the per-process dedup landed in
//! `f47ef82`, every direct `hector check` invocation (and each of the
//! ~3 hector processes the Claude Code adapter hook spawns per edit)
//! restarted the dedup, so the warning still surfaced.
//!
//! The advisory is moved to `hector doctor` (the diagnostic surface).
//! Routine `check` must not write it to stderr.

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn check_stays_quiet_on_stderr_for_passing_script_rule() {
    let dir = tempdir().unwrap();
    let project = dir.path();

    let cfg = project.join(".hector.yml");
    // Default Capabilities (network: false, writes: none) — the exact
    // shape that triggered the macOS advisory pre-R7. Backslash-newline
    // string continuation would strip leading indent and silently
    // produce a top-level YAML map with zero rules (no script runs →
    // no warning even pre-fix). Use a plain literal so indent survives.
    fs::write(
        &cfg,
        "schema_version: 2\nrules:\n  ok:\n    description: \"always passes\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"exit 0\"\n",
    )
    .unwrap();

    Command::cargo_bin("hector")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .assert()
        .success();

    let file = project.join("ok.txt");
    fs::write(&file, "fine\n").unwrap();

    let out = Command::cargo_bin("hector")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--file",
            file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .code(0)
        .get_output()
        .stderr
        .clone();

    let stderr = String::from_utf8_lossy(&out);
    assert!(
        !stderr.contains("capability"),
        "routine `hector check` must not write capability advisories to stderr; got: {stderr:?}"
    );
    assert!(
        stderr.is_empty(),
        "routine `hector check` against a passing script rule must keep stderr empty; got: {stderr:?}"
    );
}
