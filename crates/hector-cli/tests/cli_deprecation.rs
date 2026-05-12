use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

/// P2-11: schema v1 (legacy bully) is no longer loadable — it is detected
/// before trust verify and rejected with a clear "run `hector migrate`" hint.
///
/// Prior to P2-11 this test asserted that a v1 config loaded with a
/// deprecation warning on stderr. The fix elevated the warning to a hard
/// error: leaving v1 loadable produced repeated debugging cycles for users
/// who signed the config and were then surprised when v1-only features
/// silently no-op'd through v2 evaluation. Migration is now mandatory.
#[test]
fn v1_config_check_fails_with_migrate_hint() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join(".hector.yml");
    fs::write(
        &cfg,
        "schema_version: 1\nrules:\n  r:\n    description: \"x\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"true\"\n",
    )
    .unwrap();
    // Even a trusted v1 config is rejected — schema detection runs before
    // trust verify, by design.
    Command::cargo_bin("hector")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .assert()
        .success();
    let file = dir.path().join("a.txt");
    fs::write(&file, "clean\n").unwrap();
    let output = Command::cargo_bin("hector")
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
        .code(1)
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&output);
    assert!(
        stderr.contains("migrate"),
        "expected `migrate` hint in stderr, got: {stderr}"
    );
}
