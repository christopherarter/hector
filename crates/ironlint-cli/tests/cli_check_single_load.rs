mod common;

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn cli_check_loads_engine_exactly_once() {
    let tmp = tempdir().unwrap();
    let cfg_path = tmp.path().join(".ironlint.yml");
    fs::write(
        &cfg_path,
        "checks:\n  noop:\n    files: [\"*\"]\n    run: \"true\"\n",
    )
    .unwrap();
    let src = tmp.path().join("x.txt");
    fs::write(&src, "x").unwrap();

    let xdg = common::blessed_store(&cfg_path);

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args(["check", "--file"])
        .arg(&src)
        .arg("--config")
        .arg(&cfg_path)
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("IRONLINT_DEBUG_LOAD_COUNT", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Count occurrences of "ironlint_load_count=" — there must be
    // exactly one (the run-scope counter increments, so we need a
    // single emission per check invocation).
    let count = stderr.matches("ironlint_load_count=").count();
    assert_eq!(
        count, 1,
        "expected exactly one engine load per `ironlint check`; \
         saw {count} in stderr:\n{stderr}"
    );
}
