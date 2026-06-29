//! E2E for `hector watch`. In the test harness stdout is piped (not a TTY),
//! so `watch` hits the no-TTY branch: exit 1 with a guidance message. This
//! also exercises `run()`'s entry path for coverage.
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn watch_without_tty_exits_one_with_message() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .arg("watch")
        .arg("--dir")
        .arg(dir.path())
        .assert()
        .failure()
        .code(1)
        .stderr(contains("requires an interactive terminal"));
}
