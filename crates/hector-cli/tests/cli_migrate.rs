use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn migrate_renames_bully_to_hector() {
    let dir = tempdir().unwrap();
    let bully = dir.path().join(".bully.yml");
    fs::write(
        &bully,
        "schema_version: 1\nrules:\n  r:\n    description: x\n    engine: script\n    scope: [\"*\"]\n    severity: error\n    script: \"true\"\n",
    ).unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .args(["migrate", "--dir", dir.path().to_str().unwrap()])
        .assert()
        .success();
    let hector = dir.path().join(".hector.yml");
    assert!(hector.exists(), ".hector.yml written");
    assert!(bully.exists(), ".bully.yml preserved by default");
    let content = fs::read_to_string(&hector).unwrap();
    assert!(content.contains("schema_version: 2"));
}

/// P2-8 regression: migration must not rewrite `schema_version: 1` strings
/// that appear inside comments or string values. Only the top-level field
/// should change.
#[test]
fn migrate_does_not_touch_comments_mentioning_schema_version() {
    let dir = tempdir().unwrap();
    let bully = dir.path().join(".bully.yml");
    let original = "\
# Note: see migration from schema_version: 1 doc
schema_version: 1
rules:
  r:
    description: \"schema_version: 1 lives here as part of the description\"
    engine: script
    scope: [\"*\"]
    severity: error
    script: \"true\"
";
    fs::write(&bully, original).unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .args(["migrate", "--dir", dir.path().to_str().unwrap()])
        .assert()
        .success();

    let migrated = fs::read_to_string(dir.path().join(".hector.yml")).unwrap();
    // Top-level schema_version is bumped to 2.
    assert!(
        migrated.contains("schema_version: 2"),
        "schema_version bumped:\n{migrated}"
    );
    assert!(
        !migrated.contains("schema_version: 1\nrules"),
        "no v1 at top level:\n{migrated}"
    );
    // The description string is unchanged — the inner `schema_version: 1` must
    // survive as part of the rule's content.
    assert!(
        migrated.contains("schema_version: 1 lives here as part of the description"),
        "description string preserved:\n{migrated}"
    );
}

#[test]
fn migrate_moves_state_dir() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".bully.yml"),
        "schema_version: 1\nrules: {}\n",
    )
    .unwrap();
    fs::create_dir(dir.path().join(".bully")).unwrap();
    fs::write(dir.path().join(".bully/log.jsonl"), "{\"x\":1}\n").unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .args(["migrate", "--dir", dir.path().to_str().unwrap()])
        .assert()
        .success();
    assert!(dir.path().join(".hector/log.jsonl").exists());
}
