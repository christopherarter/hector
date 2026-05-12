use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn baseline_skips_gitignored_and_target_dirs() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/foo.rs"),
        "fn main() { let _ = x.unwrap(); }\n",
    )
    .unwrap();
    // `target/` is in Hector's built-in skip list, so a walkdir-based impl
    // would still descend into it (reading every file) even though the skip
    // matcher later short-circuits engine.check. A gitignore-aware walker
    // must not descend at all.
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::write(
        root.join("target/debug/junk.rs"),
        "fn main() { let _ = x.unwrap(); }\n",
    )
    .unwrap();
    // A directory that is *not* in the built-in skip globs but *is*
    // gitignored. The current walkdir impl will fingerprint it; a
    // gitignore-aware impl must skip it. This is the canonical P0-10
    // regression: real repos contain large gitignored dirs (caches, vendor
    // mirrors, generated docs) that aren't in the built-in skip list.
    fs::create_dir_all(root.join("myignored")).unwrap();
    fs::write(
        root.join("myignored/junk.rs"),
        "fn main() { let _ = x.unwrap(); }\n",
    )
    .unwrap();
    fs::write(root.join(".gitignore"), "target/\nmyignored/\n").unwrap();
    let cfg = "schema_version: 2\nrules:\n  no-unwrap:\n    description: x\n    engine: ast\n    language: rust\n    scope: [\"**/*.rs\"]\n    severity: warning\n    pattern: $E.unwrap()\n";
    let trusted = hector_core::trust::write_trust_block(cfg).unwrap();
    let cfg_path = root.join(".hector.yml");
    fs::write(&cfg_path, trusted).unwrap();
    let out = Command::cargo_bin("hector")
        .unwrap()
        .args(["baseline", "--config", cfg_path.to_str().unwrap()])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let baseline: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".hector/baseline.json")).unwrap(),
    )
    .unwrap();
    let fps = baseline["fingerprints"].as_array().unwrap();
    let printed: Vec<String> = fps
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        printed.iter().any(|f| f.contains("src/foo.rs")),
        "src/foo.rs must be baselined: {printed:?}"
    );
    assert!(
        !printed.iter().any(|f| f.contains("target/")),
        ".gitignored target/ must be skipped: {printed:?}"
    );
    assert!(
        !printed.iter().any(|f| f.contains("myignored/")),
        ".gitignored myignored/ must be skipped: {printed:?}"
    );
}

#[test]
fn baseline_records_and_then_filters() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "DEBUG marker\n").unwrap();
    let cfg = dir.path().join(".hector.yml");
    fs::write(
        &cfg,
        "schema_version: 2\nrules:\n  no-debug:\n    description: x\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"grep -nE 'DEBUG' {file} && exit 1 || exit 0\"\n",
    ).unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .assert()
        .success();
    Command::cargo_bin("hector")
        .unwrap()
        .args([
            "baseline",
            "--config",
            cfg.to_str().unwrap(),
            "--scan",
            "*.txt",
        ])
        .assert()
        .success();
    assert!(dir.path().join(".hector/baseline.json").exists());

    Command::cargo_bin("hector")
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
        .code(0);
}
