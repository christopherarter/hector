# Remove LLM evaluation (semantic + session engines)

**Date:** 2026-06-11
**Status:** Approved design, pre-implementation
**Decision owner:** Chris Arter

## Why

Hector's entire point is a static blocking gate: deterministic verdicts an
agent harness can trust at PostToolUse time. The LLM-judged paths (the
`semantic` engine, the `session` engine, and everything that exists to feed
them) pulled in a disproportionate share of the system's complexity:

- the `llm/` module (~1,276 LoC: client trait, Anthropic + OpenAI-compat
  clients, prompt rendering with injection sandboxing, retry/backoff),
- API-key/provider configuration and the `doctor` checks for it,
- two adapter dispatch strategies (direct API call vs. deferred
  subagent payload), the `hector-evaluator` subagent, and the
  `--emit-semantic-payload` / `record-verdict` round-trip protocol,
- session edit-recording (`.hector/session.json`, `hector session
  start|record`) whose only consumer was the session engine,
- non-determinism, network dependence, and a fail-open posture (exit 3)
  that a blocking gate should not need.

The tool has not shipped. There is no migration audience to protect.
Removing the feature now is strictly cheaper than carrying it.

## Decisions made

1. **Scope: full removal.** Both LLM engines (`semantic`, `session`) and the
   `llm/` module go. Hector becomes `script` + `ast` only. (The alternative —
   removing only `semantic` — was rejected because `session` has a hard
   `LlmClient` dependency with no fallback, so all the LLM machinery would
   have survived.)
2. **Config compat: hard error with a curated message.** Configs containing
   `type: semantic` or `type: session` fail at load (exit 1) with an
   explanation that the feature was removed, not a generic serde
   unknown-variant error. `hector migrate` strips such rules from bully v1
   configs and prints the dropped rule IDs.
3. **Uncommitted work: salvage the engine-agnostic parts, drop the rest.**
   See "Working-tree salvage" below.
4. **Strategy: one branch, ordered commit series**, each commit compiling and
   green. Deprecation-first and cargo-feature-flag approaches were rejected:
   no shipped users, and a feature flag preserves exactly the maintenance
   burden being removed.

## End state

- Engines: `script`, `ast`. Deterministic, offline, no API keys.
- Exit-code contract unchanged: `0` pass/warn, `1` config/internal error,
  `2` block, `3` engine runtime error (remaining causes: script spawn
  failure, AST refusing a diff). `HECTOR_FAIL_CLOSED_ON_INTERNAL` semantics
  unchanged for adapters.
- `Engine::Trust` rename (pre-0.3 open item) stays out of scope.

## Removal inventory

### hector-core

| Item | Action |
| --- | --- |
| `engine/semantic.rs`, `engine/session.rs` | delete |
| `llm/` (mod.rs, anthropic.rs, openai_compat.rs, prompt.rs) | delete |
| `session_state.rs` | delete (sole consumer was the session engine; `session_init` telemetry has no analytics consumer — verified) |
| `runner.rs`: `try_semantic_skip`, `check_session`, `check_session_with_options`, `append_semantic_verdict`, LLM prompt wiring, `with_llm` builder | delete |
| `EngineKind::{Semantic, Session}` | replaced by curated load-time rejection (see below) |
| `verdict.rs`: `Engine::{Semantic, Session}` | delete variants; **bump verdict `SCHEMA_VERSION`** (declared stability surface) |
| `telemetry.rs`: `SemanticVerdict`, `SemanticSkipped`, `SessionInit` | delete variants (`SessionInit`'s only writers are the deleted `session`/`record-verdict` commands); **bump telemetry `SCHEMA_VERSION`** |
| `reqwest` dep (hector-core), `wiremock` dev-dep (both crates) | remove from Cargo.toml / workspace deps |

### Curated config rejection

Parsing recognizes the removed type names and rejects them with a message of
the shape:

```
rule '<id>': type '<semantic|session>' was removed in hector 0.2 — delete
this rule or rewrite it as a script or ast rule
```

This must surface identically from `hector check` (exit 1) and
`hector validate`. Implementation choice (custom `Deserialize` vs.
parse-then-validate pass) is left to the implementation plan; the
requirement is the message, the exit code, and that the rule ID appears.

`hector migrate` (bully v1 → v2) drops semantic/session rules from the
output and prints a per-rule notice to stderr; migration still succeeds.

### hector-cli

- Remove subcommands: `session` (start/record), `record-verdict`.
- Remove `--emit-semantic-payload` from `check` (and `CheckOptions` field).
- `doctor`: remove LLM provider/API-key checks.

### Adapters

- **claude-code:** `hook.sh` loses provider detection, semantic payload
  emission, subagent dispatch, and `hector session record` calls. Delete
  `agents/hector-evaluator.md`; unregister it from `plugin.json`. `SKILL.md`
  reduces to blocked-stderr interpretation. `synthesize_diff.sh`: keep only
  if it feeds the gating `hector check --diff` call; delete if session
  recording was its sole consumer (verify during implementation).
- **reasonix:** `hook.sh` drops direct-API dispatch; `settings.example.json`
  drops LLM configuration.
- **Skill docs** (`hector-author`, `hector-init`, `hector-review` sources in
  this repo): remove "convert to semantic" guidance and semantic rule-type
  references.

### Docs

- Delete: `docs/reference/emit-semantic-payload.md`,
  `docs/reference/record-verdict.md`, `docs/writing-rules/asking-an-llm.md`,
  `docs/configuring/llm-providers.md`.
- Update: `docs/architecture.md`, `docs/reference/config-schema.md`,
  `docs/reference/cli.md`, `docs/operating/telemetry.md`, adapter docs,
  both READMEs, `CLAUDE.md`/`AGENTS.md` (module list, engine list, LLM
  injection section), `CHANGELOG.md`.
- Dated specs under `specs/` are historical records and stay untouched.

## Working-tree salvage

The pre-existing uncommitted work is split:

**Keep** (engine-agnostic or unrelated):
- `crates/hector-core/src/diff/synthesize.rs` + `tests/diff_synthesize.rs`
  — general unified-diff synthesizer; gives file-content checks real diff
  evidence, which baseline line-fingerprinting needs.
- The runner hunk that populates `diff` via `synthesize_file_diff` for
  resolved file-content inputs.
- `.claude-plugin/`, `adapters/reasonix/install.sh`.
- Non-semantic documentation edits (reviewed hunk-by-hunk).

**Drop** (semantic-specific):
- `crates/hector-core/tests/runner_content_semantic.rs`.
- Runner hunks wiring content into LLM prompt construction.
- Semantic additions to both adapter `hook.sh` files and the
  `adapter_claude_code.rs` / `adapter_reasonix.rs` test additions.
- Semantic hunks in doc/CHANGELOG edits.

## Testing

- Delete the LLM/semantic test files (~11 files, ~1,300 LoC):
  `semantic_engine.rs`, `runner_semantic_prefilter.rs`, `anthropic.rs`,
  `llm_factory.rs`, `llm_config_*.rs`, `llm_api_key_env_present.rs`,
  `llm_provider_subagent.rs`, `cli_e2e_emit_semantic_payload.rs`,
  `cli_e2e_record_verdict.rs`, `cli_session_start.rs`, plus semantic
  snapshot files (`cargo insta review` after verdict-shape changes).
- New regression coverage: curated error from `check` and `validate` for
  both removed types (with rule ID in message), `migrate` stripping with
  stderr notice, telemetry/verdict schema-version bumps.
- Every commit in the series compiles and passes `cargo test`,
  `cargo clippy --all-targets -- -D warnings`, and the per-file ≥80%
  region-coverage gate (`scripts/ci-coverage.sh`); `runner.rs` is the file
  to watch after losing branches.

## Size estimate

Roughly 2,700–3,100 LoC deleted and ~400 modified across 30–40 files
(core ~1,400, tests ~1,300, adapters ~440, docs ~1,000 including rewrites).
