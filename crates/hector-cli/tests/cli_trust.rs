use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn trust_writes_fingerprint() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join(".hector.yml");
    std::fs::write(
        &cfg,
        "schema_version: 2\nrules:\n  r:\n    description: \"x\"\n    engine: script\n    scope: [\"*\"]\n    severity: error\n    script: \"true\"\n",
    ).unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .assert()
        .success();

    let written = std::fs::read_to_string(&cfg).unwrap();
    assert!(written.contains("trust:"), "trust block written");
    assert!(written.contains("sha256:"), "fingerprint written");
}

#[test]
fn trust_then_verify_round_trip() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join(".hector.yml");
    std::fs::write(
        &cfg,
        "schema_version: 2\nrules:\n  r:\n    description: \"x\"\n    engine: script\n    scope: [\"*\"]\n    severity: error\n    script: \"true\"\n",
    ).unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .assert()
        .success();
    let written = std::fs::read_to_string(&cfg).unwrap();
    hector_core::trust::verify(&written).expect("verify after trust");
}
