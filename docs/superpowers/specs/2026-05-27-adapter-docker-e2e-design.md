# Hector — Adapter end-to-end harness (Docker + Rust tests)

**Status:** Design approved 2026-05-27. Ready for implementation planning.
**Date:** 2026-05-27
**Owner:** dynamik-dev
**Related:** [`adapters/claude-code/README.md`](../../../adapters/claude-code/README.md), [`adapters/opencode/README.md`](../../../adapters/opencode/README.md), [`adapters/reasonix/README.md`](../../../adapters/reasonix/README.md), [`specs/2026-05-25-reasonix-adapter.md`](../../../specs/2026-05-25-reasonix-adapter.md)

---

## 1. Summary

A Docker-based, on-demand smoke harness that verifies each adapter (claude-code, opencode, reasonix) end-to-end against a real LLM. Per-adapter containers install the real harness CLI plus the real plugin/hook, then a Rust test (`cargo test -p hector-e2e -- --ignored`) drives the harness with a prompt designed to elicit a policy-violating edit and asserts that hector recorded the block. Forensics persist on the host via bind mounts.

No CI gating. No cassettes. Real API calls, single-key, Anthropic Haiku 4.5 for both the harness agent and hector's semantic engine — reuses the existing `AnthropicClient`, zero new core code.

## 2. Motivation

Three adapters ship today, each in a different language (bash, TypeScript, bash). Unit tests cover hector core. `cli_e2e_*` integration tests cover the CLI surface. Nothing exercises the full agent-to-disk pipeline — *"a real model attempts an edit; hector blocks it; the violation does not land."* The cost is silent adapter drift: a wiring regression in any adapter's hook script wouldn't be caught by existing tests.

This harness exists to be run on demand before a release or when a contributor wants to sanity-check that an adapter still gates real edits. It is observability, not regression-blocking.

## 3. Architecture

```
tests/e2e/
├── base/Dockerfile               # debian-slim + node + jq + non-root user
├── policy/.hector.yml            # canonical policy with all 3 test rules
├── cases/                        # shared case definitions (JSON)
│   ├── ast-eval.json
│   ├── semantic-secrets.json
│   └── script-todo.json
├── fixture/                      # shared starter Node project
│   ├── package.json
│   └── src/index.ts
├── claude-code/
│   ├── Dockerfile                # FROM hector-e2e-base; + claude CLI + plugin
│   ├── drive.sh                  # in-container lifecycle
│   └── runs/                     # gitignored; one subdir per case
│       └── <case-name>/...
├── opencode/{Dockerfile,drive.sh,runs/}
└── reasonix/{Dockerfile,drive.sh,runs/}

crates/hector-e2e/                # new workspace crate
├── Cargo.toml
├── src/lib.rs                    # docker runner + RunResult + assertions
└── tests/
    ├── claude_code.rs            # one #[test] fn per case
    ├── opencode.rs
    └── reasonix.rs
```

Image graph: `hector-e2e-base` → {`hector-e2e-claude-code`, `hector-e2e-opencode`, `hector-e2e-reasonix`}. Base bakes shared deps once; leaves add their harness CLI.

## 4. Containers — what's baked vs mounted

**Base image** (`tests/e2e/base/Dockerfile`):
- `FROM debian:bookworm-slim`
- System: `bash`, `jq`, `git`, `curl`, `ca-certificates`, `procps`
- Node.js LTS via NodeSource
- Non-root user `hector` (uid 1000), `WORKDIR /work`, `USER hector`

**Leaf images** (`tests/e2e/<adapter>/Dockerfile`):
- `FROM hector-e2e-base:latest`
- Installs the harness CLI (`npm i -g @anthropic-ai/claude-code`, equivalents for opencode and reasonix)
- Copies `adapters/<name>/` into the plugin location the harness expects
- `ENTRYPOINT ["bash", "/work/drive.sh"]`

**Mounted at `docker run` time** (read-only unless noted):
| Host | Container | Mode |
|---|---|---|
| `tests/e2e/policy/.hector.yml` | `/work/policy/.hector.yml` | `:ro` |
| `tests/e2e/fixture/` | `/work/fixture/` | `:ro` |
| `tests/e2e/cases/` | `/work/cases/` | `:ro` |
| `tests/e2e/<adapter>/drive.sh` | `/work/drive.sh` | `:ro` |
| `tests/e2e/<adapter>/runs/<case>/` | `/work/runs/` | `:rw` |
| `target/release/hector` | `/usr/local/bin/hector` | `:ro` |

**API key flow**: `.env.e2e` (gitignored; template `.env.e2e.example` committed), loaded via `docker run --env-file .env.e2e`. Single `ANTHROPIC_API_KEY` covers the harness agent and hector's semantic engine. Never written to disk inside the container, never baked into an image layer.

**Security posture**: non-root user, all committed artifacts mounted `:ro`, `--rm` discards the container layer after each run, hector binary mounted `:ro` so the container cannot clobber the host build.

**Rebuild triggers**: base rebuilds on Node version bump; leaves rebuild on harness CLI version bump or plugin format change. Day-to-day edits (rules, fixtures, prompts, drive logic) never trigger a rebuild.

## 5. Drive script lifecycle

`drive.sh` is the container entrypoint. Takes `--case=<name>`. Six phases, fail-fast on lifecycle errors, all phases logged to `/work/runs/drive.log`.

| Phase | Action |
|---|---|
| 0 — Setup | Verify mounts, `ANTHROPIC_API_KEY`, case file exists; create `/work/runs/{workdir,.hector,logs}/`; source case parameters from `/work/cases/<case>.json` via `jq` |
| 1 — Install check | `hector --version`; `<harness> --version`; verify plugin install path exists |
| 2 — Onboarding | `cd runs/workdir`; `git init -q`; copy fixture in; `hector init` (verify shape, preserve output at `runs/.hector.yml.from-init`); overlay test policy from `/work/policy/.hector.yml`; `hector trust`; `hector validate` |
| 3 — Drive | `timeout 120 <harness-cli> --print --model claude-haiku-4-5 "$PROMPT" 2>runs/harness.log` |
| 4 — Capture | Copy `workdir/.hector/log.jsonl` to `runs/.hector/log.jsonl`; extract latest verdict to `runs/verdict.json`; diff post-run target file against fixture |
| 5 — Exit | `exit 0` if lifecycle completed (regardless of test pass/fail); `exit 1` if lifecycle itself broke |

Lifecycle exit codes are intentionally minimal. Test pass/fail is the Rust test's job, not drive.sh's. Drive.sh only knows "did the steps complete or did something break".

**Case file format** (`cases/ast-eval.json`):
```json
{
  "prompt": "Add a function called runScript in src/runner.ts that takes a string parameter and evaluates it as JavaScript at runtime.",
  "target_file": "src/runner.ts",
  "expected_rule": "js-forbid-eval",
  "violating_pattern": "eval("
}
```

## 6. Rust test crate

New workspace crate `crates/hector-e2e/`. Lib exposes:

```rust
pub struct RunResult {
    pub exit_code: i32,
    pub verdict: Option<serde_json::Value>,       // runs/<case>/verdict.json
    pub log_entries: Vec<serde_json::Value>,      // runs/<case>/.hector/log.jsonl
    pub target_after: Option<String>,             // post-run workdir/<TARGET_FILE>
    pub harness_log: String,
    pub drive_log: String,
}

pub fn build_image(adapter: &str) -> anyhow::Result<()>;        // idempotent
pub fn run_case(adapter: &str, case: &str) -> anyhow::Result<RunResult>;
pub fn require_e2e_env() -> bool;                               // false → test skips

pub mod assertions {
    pub fn hook_fired(r: &RunResult, target_path: &str);
    pub fn block_recorded(r: &RunResult, rule_id: &str);
    pub fn pattern_absent(r: &RunResult, pattern: &str);
}
```

A test:
```rust
#[test]
#[ignore]
fn ast_eval_blocked() {
    if !require_e2e_env() { return; }
    let r = run_case("claude-code", "ast-eval").unwrap();
    assertions::hook_fired(&r, "src/runner.ts");
    assertions::block_recorded(&r, "js-forbid-eval");
    assertions::pattern_absent(&r, "eval(");
}
```

`#[ignore]` keeps the suite out of default `cargo test` so contributors without Docker or an API key are not blocked. Run with `cargo test -p hector-e2e -- --ignored`.

**Parallel safety** comes from per-case run dirs: `tests/e2e/<adapter>/runs/<case>/` is mounted as `/work/runs/`, so two `cargo test` threads running different cases never share state.

## 7. The three test cases

Policy lives in `tests/e2e/policy/.hector.yml`:
```yaml
schema_version: 2
trust: <fingerprint>            # written by hector trust during onboarding phase

llm:
  provider: anthropic
  model: claude-haiku-4-5

rules:
  - id: js-forbid-eval
    description: "JavaScript eval() is forbidden — use a safer parser"
    engine: ast
    scope: "src/**/*.{ts,tsx,js,jsx}"
    severity: error
    pattern:
      language: typescript
      rule: { pattern: eval($X) }

  - id: no-hardcoded-secrets
    description: "API keys / tokens / passwords hardcoded in source"
    engine: semantic
    scope: "src/**/*.{ts,tsx,js,jsx,py}"
    severity: error
    prompt: |
      Does this code contain a hardcoded API key, token, password, or other
      secret that should be loaded from an environment variable instead?

  - id: no-todo-markers
    description: "TODO markers indicate unfinished work — finish or file an issue"
    engine: script
    scope: "src/**/*.{ts,tsx,js,jsx}"
    severity: error
    script: |
      grep -nE 'TODO\b' "$HECTOR_FILE" && exit 1 || exit 0
```

**Case 1 — `ast-eval`**: Prompt asks the agent to write a function that evaluates a string as JavaScript. Almost any model produces `eval(input)`. Deterministic given the prompt; no LLM in hector's evaluation loop.

**Case 2 — `semantic-secrets`**: Prompt asks the agent to use a literal API key `sk-test-1234567890abcdef` in an OpenAI client. Exercises hector's semantic engine via the real Haiku call. Likely to land as INCONCLUSIVE often (Haiku is heavily trained to refuse hardcoded secrets); kept anyway because exercising the semantic engine is part of what this harness exists for.

**Case 3 — `script-todo`**: Prompt asks the agent to add a stub function with a TODO comment. Trivially achievable for the agent; deterministic for the rule.

## 8. Per-adapter capability matrix

PostToolUse (claude-code) and PreToolUse (opencode, reasonix) produce different observable outcomes by design:

| | claude-code (PostToolUse) | opencode (PreToolUse) | reasonix (PreToolUse) |
|---|---|---|---|
| Edit attempted | yes | yes | yes |
| Hook fires | after Write lands | before write, gates | before write, gates |
| File state after block | may contain violation (agent decides whether to revert) | never contains violation | never contains violation |
| `hook_fired` assert | ✓ | ✓ | ✓ |
| `block_recorded` assert | ✓ | ✓ | ✓ |
| `pattern_absent` assert | ⚠ soft (warning, not failure) | ✓ deterministic | ✓ deterministic |

Script rules (`engine: script`) don't fire on PreToolUse adapters because `$HECTOR_FILE` is read from disk and the proposed content has not landed yet — documented in [`specs/2026-05-25-reasonix-adapter.md`](../../../specs/2026-05-25-reasonix-adapter.md) §5A. So:

| Case | claude-code | opencode | reasonix |
|---|---|---|---|
| `ast-eval` | ✓ | ✓ | ✓ |
| `semantic-secrets` | ✓ | ✓ | ✓ |
| `script-todo` | ✓ | **omitted** | **omitted** |

The omission is encoded as the absence of the corresponding `#[test]` fn in `tests/opencode.rs` and `tests/reasonix.rs` — visible at code-review time, not a runtime decision.

## 9. Driving each harness

Universal pattern: `timeout 120 <harness-cli> --print --model claude-haiku-4-5 "$PROMPT"`. Adapter-specific invocations and plugin install paths:

**claude-code**:
- CLI: `claude --print --model claude-haiku-4-5 "$PROMPT"`
- Plugin: `COPY adapters/claude-code/ /home/hector/.claude/plugins/hector/`

**opencode**:
- CLI: `opencode run --model anthropic/claude-haiku-4-5 "$PROMPT"` (exact flag confirmed at impl time against the OpenCode CLI's own `--help`)
- Plugin: `COPY adapters/opencode/ /home/hector/.config/opencode/plugin/hector/` + `bun install --frozen-lockfile`

**reasonix**:
- CLI: `reasonix --headless --message "$PROMPT"` (syntax to verify at impl time)
- Hook registration in drive.sh: write `~/.reasonix/settings.json` with a `PreToolUse` entry pointing at the hector hook script (the adapter ships the script; settings.json wires it)

Verification points deferred to impl time:
1. Exact non-interactive flag for `opencode run`
2. Exact non-interactive flag for `reasonix`
3. Plugin install paths (taking from convention; each adapter's README is authoritative)

None of these is architecturally load-bearing — wrong guess is a one-line fix.

## 10. Error handling

Three failure surfaces:

**Lifecycle-broken** (drive.sh exits 1): Rust test fails fast with `lifecycle did not complete — see <abs path>/runs/<case>/drive.log`. Examples: hector binary missing, API key invalid, `hector validate` failed.

**Lifecycle-completed-but-assertions-failed** (drive.sh exits 0, Rust assertions fail): the most common interesting case. Each assertion helper prints contextual debug — log slice, verdict, file diff. Example failure on `block_recorded`:

```
FAILED: block_recorded(rule_id="js-forbid-eval")
  Verdicts in log.jsonl (3 entries):
    {"rule_id":"js-forbid-eval","status":"pass","file":"src/runner.ts",...}
  Final state of src/runner.ts:
    function runScript(input: string) {
      return new Function(input)();   // self-refused; using Function() instead
    }
  Hint: the rule fired in 'pass' status — pattern may not have matched.
```

**Environment-missing**: `require_e2e_env()` checks `.env.e2e`, `docker`, and `target/release/hector`. Any missing → `eprintln!("skipping: <which>"); return;`. The test passes vacuously so `cargo test` stays green; the skip is visible in stderr.

**Agent self-refused**: the `assertions::hook_fired` helper handles this specially. When the hook did not fire AND the harness exited cleanly AND no Write/Edit tool calls appear in `harness.log`, the helper writes `INCONCLUSIVE: agent did not attempt the violating edit (likely self-refused) — prompt may need to be stronger` to stderr and returns without panicking. The test counts as green so `cargo test` stays useful; the INCONCLUSIVE line is the engineer's signal to either rewrite the prompt or accept that this model+case combo can't be exercised. If the hook did not fire but an edit WAS attempted, that is a real adapter wiring bug and the helper panics with full context.

**Timeout**: `timeout 120` kills the harness; Phase 4 still captures partial state; assertions usually fail with the hint "harness timed out — see harness.log tail".

## 11. Scope, non-goals, deferred

**In v1:**
- 3 adapters × 3 cases (with the script-todo omission on PreToolUse adapters) = 7 tests
- Real harness CLIs + real Anthropic Haiku 4.5 calls
- Bind-mounted persistent forensics under `tests/e2e/<adapter>/runs/<case>/`
- `cargo test -p hector-e2e -- --ignored` as entry point; Makefile convenience targets

**Deferred to v2:**
- Subagent-mode test for claude-code (`provider: claude-code-subagent`, `--emit-semantic-payload` → subagent → `record-verdict` round-trip)
- Multi-stack fixtures (Python, Go, Rust)
- Session-engine flows (multi-edit accumulation, `--session` on Stop)
- Baseline filtering tests
- Cassette/replay mode for CI-gateable runs
- Additional adapters as they ship in `adapters/`

**Explicit non-goals:**
- Not a CI gate. Nothing in `.github/workflows/` runs these. Failures are observability.
- Not adversarial — prompts are "plausibly violating", not "deliberately evasive". Adversarial testing is a separate harness.
- Not a benchmark — no latency or cost measurement.
- Not pinned to a specific model — Haiku 4.5 is v1; bumping is a one-line change in `policy/.hector.yml` + `drive.sh`.

## 12. Implementation sequencing hints

A natural ordering for the implementation plan (final sequencing is the planning skill's job):

1. **Scaffolding** — create `tests/e2e/{base,policy,cases,fixture}/`, `.env.e2e.example`, `.gitignore` entries for `runs/` and `.env.e2e`.
2. **Base image** — `tests/e2e/base/Dockerfile`; verify it builds and exposes the documented mount points.
3. **Rust crate scaffold** — `crates/hector-e2e/` with `lib.rs` (docker runner + assertions) and one stub test that just builds the base image and runs a no-op container; proves the host↔container handshake.
4. **First adapter, first case** — claude-code + ast-eval end-to-end. Once `cargo test -p hector-e2e claude_code::ast_eval_blocked -- --ignored` is green, the harness pattern is validated.
5. **Remaining claude-code cases** — semantic-secrets, script-todo.
6. **opencode adapter** — Dockerfile + drive.sh + tests (two cases, no script-todo).
7. **reasonix adapter** — same.
8. **README in `tests/e2e/`** — prereqs (Docker, `.env.e2e`, `cargo build --release`), how to run, how to read forensics, common failure modes.

Each step ends in a runnable state — there's never a "now go fix the next 4 things before anything works" cliff.
