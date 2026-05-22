//! H2 — end-to-end coverage that `hector record-verdict` appends a
//! `SemanticVerdict` line to `.hector/log.jsonl`.

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn record_verdict_subcommand_is_recognised() {
    // Phase 1: just confirm the subcommand exists. Real append is verified
    // in Phase 2's test (overrides this minimal check).
    let tmp = tempdir().unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .arg("record-verdict")
        .arg("--rule")
        .arg("no-debug")
        .arg("--verdict")
        .arg("pass")
        .arg("--dir")
        .arg(tmp.path())
        .assert()
        .code(0);
}

#[test]
fn record_verdict_rejects_invalid_verdict_value() {
    // clap-enforced. Anything other than `pass` or `violation` errors at
    // parse time. We do NOT use code 1 here — clap exits with its own code
    // (2 on most platforms) for parse errors. The body of `run` is never
    // entered.
    let tmp = tempdir().unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .arg("record-verdict")
        .arg("--rule")
        .arg("no-debug")
        .arg("--verdict")
        .arg("fail") // not in the enum
        .arg("--dir")
        .arg(tmp.path())
        .assert()
        .failure();
}
