//! End-to-end coverage for `ironlint update`'s no-receipt path.
//!
//! A binary with no install receipt (Homebrew / `cargo install` / source build)
//! must defer with a friendly message and exit 1 — never attempt a network
//! update. The successful-update path can't run in CI (it self-replaces against
//! a live GitHub release) and is verified manually.

use assert_cmd::Command;

#[test]
fn update_without_receipt_defers_and_exits_one() {
    // Point every receipt-lookup root at an empty dir so the receipt load fails
    // with NoReceipt regardless of how the test host installed ironlint. This
    // short-circuits before any network call.
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path())
        .env("LOCALAPPDATA", home.path())
        // axoupdater consults this before any home-directory logic, so it makes
        // NoReceipt deterministic regardless of the host's homedir behavior or a
        // stray real receipt on a dev machine.
        .env("AXOUPDATER_CONFIG_PATH", home.path())
        .arg("update")
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("can't self-update"))
        .stderr(predicates::str::contains("ironlint-cli-installer.sh"));
}
