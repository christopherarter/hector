//! Reasonix adapter end-to-end smoke tests.
//!
//! PreToolUse-only — `script-todo` is intentionally omitted (see
//! `specs/2026-05-25-reasonix-adapter.md` §5A: `engine: script` rules
//! cannot observe proposed content on a PreToolUse adapter).
//!
//! Run with: cargo test -p hector-e2e --test reasonix -- --ignored

use hector_e2e::{assertions, build_image, require_e2e_env, run_case};

#[test]
#[ignore = "requires Docker, ANTHROPIC_API_KEY, and a release hector binary — run with --ignored"]
fn ast_eval_blocked() {
    if !require_e2e_env() {
        return;
    }
    build_image("reasonix").expect("docker build");
    let r = run_case("reasonix", "ast-eval").expect("docker run");
    assertions::hook_fired(&r, "src/runner.ts");
    assertions::block_recorded(&r, "js-forbid-eval");
    assertions::pattern_absent(&r, "eval(");
}

#[test]
#[ignore = "requires Docker, ANTHROPIC_API_KEY, and a release hector binary — run with --ignored"]
fn semantic_secrets_blocked() {
    if !require_e2e_env() {
        return;
    }
    build_image("reasonix").expect("docker build");
    let r = run_case("reasonix", "semantic-secrets").expect("docker run");
    assertions::hook_fired(&r, "src/openai-client.ts");
    assertions::block_recorded(&r, "no-hardcoded-secrets");
    assertions::pattern_absent(&r, "sk-test-1234567890abcdef");
}
