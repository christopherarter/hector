use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn cli_check_exit_3_for_missing_api_key() {
    let tmp = tempdir().unwrap();
    // `model:` is required for `anthropic` — without it the runner errors at
    // load time (exit 1) before any rule evaluation. With it, the missing
    // NOPE_NOT_SET env var causes `build_from_config` to return Ok(None)
    // (no LLM), which makes the semantic engine fail at evaluation time with
    // an Engine::Internal violation → Status::InternalError → exit 3.
    let cfg = concat!(
        "schema_version: 2\n",
        "llm:\n",
        "  provider: anthropic\n",
        "  model: claude-3-haiku-20240307\n",
        "  api_key_env: NOPE_NOT_SET\n",
        "rules:\n",
        "  s:\n",
        "    description: x\n",
        "    engine: semantic\n",
        "    scope: [\"*.rs\"]\n",
        "    severity: warning\n",
    );
    let cfg_path = tmp.path().join(".hector.yml");
    fs::write(&cfg_path, cfg).unwrap();
    let signed =
        hector_core::trust::write_trust_block(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    fs::write(&cfg_path, signed).unwrap();
    let src = tmp.path().join("f.rs");
    fs::write(&src, "fn main() {}\n").unwrap();

    let out = Command::cargo_bin("hector")
        .unwrap()
        .args(["check"])
        .arg("--file")
        .arg(&src)
        .arg("--config")
        .arg(&cfg_path)
        .env_remove("NOPE_NOT_SET")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "missing API key → exit 3 (not 2); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
