use assert_cmd::Command;
use std::path::Path;

fn hector(home: &Path, project: &Path) -> Command {
    let mut c = Command::cargo_bin("hector").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .current_dir(project);
    c
}

#[test]
fn init_installs_reasonix_hook_with_yes() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(home.join(".reasonix")).unwrap();

    hector(&home, &project)
        .args(["init", "--harness", "reasonix", "--yes"])
        .assert()
        .success();

    let hook = home.join(".config/hector/adapters/reasonix/hook.sh");
    assert!(hook.exists(), "hook artifact materialized");
    let settings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".reasonix/settings.json")).unwrap(),
    )
    .unwrap();
    assert!(settings["hooks"]["PreToolUse"][0]["command"]
        .as_str()
        .unwrap()
        .contains("adapters/reasonix/hook.sh"));
}

#[test]
fn reinstall_reports_already_present() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    let run = || {
        hector(&home, &project)
            .args(["init", "--hook-only", "--harness", "reasonix", "--yes"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    run();
    let out = String::from_utf8(run()).unwrap();
    assert!(
        out.contains("already present"),
        "second run idempotent: {out}"
    );
}

#[test]
fn dry_run_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    hector(&home, &project)
        .args([
            "init",
            "--hook-only",
            "--harness",
            "reasonix",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success();
    assert!(!home.join(".reasonix/settings.json").exists());
    assert!(!home
        .join(".config/hector/adapters/reasonix/hook.sh")
        .exists());
}

#[test]
fn uninstall_removes_hook() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    hector(&home, &project)
        .args(["init", "--hook-only", "--harness", "reasonix", "--yes"])
        .assert()
        .success();
    hector(&home, &project)
        .args(["init", "--uninstall", "--harness", "reasonix"])
        .assert()
        .success();
    assert!(!home
        .join(".config/hector/adapters/reasonix/hook.sh")
        .exists());
    let settings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".reasonix/settings.json")).unwrap(),
    )
    .unwrap();
    let arr = settings["hooks"]["PreToolUse"].as_array().unwrap();
    assert!(
        arr.iter().all(|e| !e["command"]
            .as_str()
            .unwrap_or("")
            .contains("adapters/reasonix/hook.sh")),
        "uninstall must remove the hector PreToolUse entry"
    );
}

#[test]
fn no_tty_without_yes_or_harness_skips_hooks() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(home.join(".reasonix")).unwrap();

    // assert_cmd pipes stdin (non-TTY); bare init must not install.
    let out = hector(&home, &project)
        .args(["init", "--hook-only"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        String::from_utf8(out).unwrap().contains("re-run with"),
        "non-TTY path must print the re-run hint"
    );
    assert!(!home.join(".reasonix/settings.json").exists());
}
