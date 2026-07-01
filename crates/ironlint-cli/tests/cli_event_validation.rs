//! `--event` is restricted to the two ABI values at the clap layer.
//!
//! Regression coverage for the finding that `--event` was an unvalidated
//! `String`: a typo like `--event percommit` propagated verbatim into
//! `$IRONLINT_EVENT`. The ABI enumerates exactly: write, pre-commit.

mod common;

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

const PASSING_CONFIG: &str = "checks:\n  g:\n    files: [\"*.rs\"]\n    run: \"true\"\n";

#[test]
fn event_bogus_is_rejected_and_lists_valid_values() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    fs::write(&cfg, PASSING_CONFIG).unwrap();
    let file = dir.path().join("lib.rs");
    fs::write(&file, "fn main() {}\n").unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--file",
            file.to_str().unwrap(),
            "--event",
            "bogus",
        ])
        .output()
        .unwrap();

    assert_ne!(
        out.status.code(),
        Some(0),
        "an invalid --event must not exit 0"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    for v in ["write", "pre-commit"] {
        assert!(
            stderr.contains(v),
            "rejection must enumerate the valid event `{v}`: {stderr}"
        );
    }
    for v in ["edit", "manual"] {
        assert!(
            !stderr.contains(v),
            "rejection must NOT list retired event `{v}`: {stderr}"
        );
    }
}

#[test]
fn event_write_is_accepted() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    fs::write(&cfg, PASSING_CONFIG).unwrap();
    let file = dir.path().join("lib.rs");
    fs::write(&file, "fn main() {}\n").unwrap();

    let xdg = common::blessed_store(&cfg);

    // `--event write` parses cleanly and the passing gate yields exit 0.
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--file",
            file.to_str().unwrap(),
            "--event",
            "write",
        ])
        .assert()
        .code(0);
}
