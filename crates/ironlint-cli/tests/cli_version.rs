use assert_cmd::Command;

#[test]
fn version_flag_prints_version() {
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("ironlint "));
}

#[test]
fn help_lists_subcommands() {
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("check"))
        .stdout(predicates::str::contains("trust"))
        .stdout(predicates::str::contains("validate"));
}
