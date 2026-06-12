# Remove LLM Evaluation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the `semantic` and `session` engines and all LLM machinery from hector, leaving a purely static `script` + `ast` blocking gate.

**Architecture:** Top-down removal — first a parse-time gate makes removed-engine configs unloadable (with a curated, rule-ID-bearing error), then consumers are peeled in dependency order (CLI surface → session machinery → semantic/deferred/llm machinery → verdict/telemetry shape → adapters → docs). Every task ends green: `cargo test` + `cargo clippy --all-targets -- -D warnings` pass at every commit.

**Tech Stack:** Rust workspace (hector-core lib + hector-cli bin), serde/serde_yaml, clap, insta snapshots, assert_cmd CLI tests, bash adapter hooks.

**Spec:** `specs/2026-06-11-remove-llm-evaluation.md`. Branch `remove-llm-evaluation` exists; spec is committed; the working tree carries mixed WIP handled in Task 1.

**Conventions that bind every task** (from AGENTS.md):
- Each commit compiles and is green.
- Per-file ≥80% region coverage is CI-enforced (`scripts/ci-coverage.sh`); it cannot run locally (no llvm-tools-preview on Homebrew rustc) — flag coverage-risky deletions in the PR description instead.
- Cognitive complexity ≤15 per function (clippy warns).
- `cargo fmt` before each commit.

---

### Task 1: Working-tree triage (salvage / drop the WIP)

The uncommitted tree mixes engine-agnostic work (keep) with semantic WIP (drop).

**Files:**
- Delete: `crates/hector-core/tests/runner_content_semantic.rs`
- Restore to HEAD: both adapter hooks, reasonix README/settings, claude-code SKILL.md, both adapter test files
- Commit as-is: `crates/hector-core/src/diff/{mod,synthesize}.rs`, `crates/hector-core/tests/diff_synthesize.rs`, `crates/hector-core/src/runner.rs`, `.claude-plugin/`, `adapters/reasonix/install.sh`, doc edits

- [ ] **Step 1: Drop the semantic-specific WIP**

```bash
rm crates/hector-core/tests/runner_content_semantic.rs
git checkout -- \
  adapters/claude-code/hooks/hook.sh \
  adapters/reasonix/hooks/hook.sh \
  adapters/reasonix/hooks/settings.example.json \
  adapters/reasonix/README.md \
  adapters/claude-code/skills/hector/SKILL.md \
  crates/hector-cli/tests/adapter_claude_code.rs \
  crates/hector-cli/tests/adapter_reasonix.rs
```

- [ ] **Step 2: Review the CHANGELOG.md WIP hunk**

Run: `git diff CHANGELOG.md`. Keep lines describing the synthesized-diff gating (commit db431af's feature); delete lines describing semantic/deferred/LLM behavior. If unsure, keep — Task 8 rewrites the CHANGELOG anyway.

- [ ] **Step 3: Verify the remaining tree is green**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: PASS. (The kept runner.rs WIP adds `synthesize_file_diff`; its semantic-path hunk lives inside functions deleted in Task 5b, so it compiles fine meanwhile.)

- [ ] **Step 4: Commit in two slices**

```bash
git add crates/hector-core/src/diff crates/hector-core/src/runner.rs crates/hector-core/tests/diff_synthesize.rs
git commit -m "feat(core): synthesize diff evidence for file-content checks"
git add -A
git commit -m "chore: claude plugin marketplace manifest, reasonix installer, doc WIP"
```

---

### Task 2: Parse-time rejection of removed engines (TDD) + delete LLM-dependent tests

After this task, no config containing `engine: semantic` or `engine: session` can load anywhere (check, validate, extends-inherited — all route through `parse_str`). Every test that loads such a config is deleted in the same commit (all are spec-listed deletions anyway).

**Files:**
- Create: `crates/hector-core/tests/config_removed_engines.rs`
- Modify: `crates/hector-core/src/config/parser.rs:6-16`, `crates/hector-core/src/config/types.rs:118-125`
- Modify: `crates/hector-cli/tests/cli_validate.rs`, `crates/hector-cli/tests/cli_check.rs` (add one test each)
- Modify: `tests/fixtures/valid_v2.hector.yml` (drop `llm:` block + any semantic rule)
- Delete (core tests): `semantic_engine.rs`, `session_engine.rs`, `runner_semantic_prefilter.rs`, `check_session.rs`, `check_session_scope.rs`, `runner_deferred_mode.rs`, `runner_deferred_context_parity.rs`, `runner_deferred_session.rs`, `deferred_envelope_v3.rs`, `anthropic.rs`, `openai_compat.rs`, `prompt_injection.rs`, `llm_factory.rs`, `llm_config_evaluator_model.rs`, `llm_config_subagent_model_optional.rs`, `llm_api_key_env_present.rs`, `llm_provider_subagent.rs`, `context_expansion.rs`
- Delete (CLI tests): `cli_e2e_emit_semantic_payload.rs`
- Trim (fn-level): `runner_check_options.rs` (4 fns), `cli_check_flags.rs` (4 `print_prompt_*` fns), `cli_e2e_doctor.rs` (`doctor_warns_when_semantic_rule_present_without_api_key`), `cli_typed_telemetry.rs` (both fns load semantic configs — rewrite `full_session_emits_every_typed_variant` against a script-only config; delete `semantic_skipped_record_is_emitted_for_pure_deletion_diff`), `adapter_claude_code.rs` (`subagent_clean_file_emits_envelope`, `subagent_deterministic_block_carries_deferred_rules`, `subagent_no_semantic_no_block_is_silent`, `direct_api_mode_emits_no_envelope`), `builder.rs` (any fn using `with_llm` on a semantic config — keep fns exercising `with_options`)

- [ ] **Step 1: Write the failing tests**

Create `crates/hector-core/tests/config_removed_engines.rs` (mirror imports from the existing `tests/parser.rs`):

```rust
use hector_core::config::parser::parse_str;

fn cfg_with_engine(engine: &str) -> String {
    format!(
        r#"
schema_version: 2
rules:
  judge-me:
    description: "llm-judged rule"
    engine: {engine}
    scope: "**/*.ts"
    severity: error
"#
    )
}

#[test]
fn semantic_rule_is_rejected_with_curated_error() {
    let err = parse_str(&cfg_with_engine("semantic")).unwrap_err().to_string();
    assert!(err.contains("rule 'judge-me'"), "got: {err}");
    assert!(err.contains("engine 'semantic' was removed"), "got: {err}");
    assert!(err.contains("script or ast"), "got: {err}");
}

#[test]
fn session_rule_is_rejected_with_curated_error() {
    let err = parse_str(&cfg_with_engine("session")).unwrap_err().to_string();
    assert!(err.contains("rule 'judge-me'"), "got: {err}");
    assert!(err.contains("engine 'session' was removed"), "got: {err}");
}

#[test]
fn script_and_ast_rules_still_parse() {
    let yaml = r#"
schema_version: 2
rules:
  ok-script:
    description: "fine"
    engine: script
    scope: "**/*.ts"
    severity: error
    script: "true"
"#;
    assert!(parse_str(yaml).is_ok());
}
```

Add to `crates/hector-cli/tests/cli_validate.rs` and `cli_check.rs` one test each asserting exit code 1 and the curated message on stderr, using each file's existing trusted-config helper (the config must pass the trust gate so the parse error — not a trust error — surfaces). Note: the spec wrote `type:` in its message sketch, but the config field is `engine:`; the message says `engine` deliberately.

- [ ] **Step 2: Run new tests to verify they fail**

Run: `cargo test --test config_removed_engines`
Expected: FAIL — semantic/session configs currently parse successfully.

- [ ] **Step 3: Implement the gate**

In `crates/hector-core/src/config/types.rs`, mark the variants parse-only:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Script,
    Ast,
    /// Parse-only. LLM evaluation was removed in 0.2. These variants exist
    /// solely so `parse_str` can reject old configs with a curated,
    /// rule-ID-bearing error instead of serde's generic unknown-variant
    /// failure. No `EngineKind` value holding them survives `parse_str`.
    #[doc(hidden)]
    Semantic,
    #[doc(hidden)]
    Session,
}
```

In `crates/hector-core/src/config/parser.rs`, after the schema-version check in `parse_str` (import `EngineKind` from `super::types`):

```rust
for (id, rule) in &cfg.rules {
    let removed = match rule.engine {
        EngineKind::Semantic => Some("semantic"),
        EngineKind::Session => Some("session"),
        EngineKind::Script | EngineKind::Ast => None,
    };
    if let Some(kind) = removed {
        return Err(anyhow!(
            "rule '{id}': engine '{kind}' was removed in hector 0.2 — \
             delete this rule or rewrite it as a script or ast rule"
        ));
    }
}
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test --test config_removed_engines`
Expected: PASS.

- [ ] **Step 5: Delete/trim the now-failing LLM-dependent tests**

Delete and trim the files/fns listed above, plus edit `tests/fixtures/valid_v2.hector.yml` to drop its `llm:` block and any semantic rule (if a trust fingerprint covers this fixture, re-run `hector trust` logic per the fixture's existing setup). Then:

Run: `cargo test`
Expected: any remaining failure is a test loading a removed-engine config — delete that test fn too (it is in the spec's deletion set by definition). Iterate until green.

- [ ] **Step 6: Clippy, fmt, commit**

```bash
cargo clippy --all-targets -- -D warnings && cargo fmt
git add -A && git commit -m "feat(config): reject removed semantic/session engines at parse with curated error"
```

---

### Task 3: `hector migrate` strips removed-engine rules (TDD)

**Files:**
- Modify: `crates/hector-cli/src/commands/migrate.rs` (61 lines, currently zero engine awareness)
- Test: `crates/hector-cli/tests/cli_migrate.rs`

- [ ] **Step 1: Write the failing test** (mirror cli_migrate.rs's existing tempdir/Command style)

```rust
#[test]
fn migrate_strips_semantic_and_session_rules_with_notice() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".bully.yml"),
        r#"
schema_version: 1
rules:
  keep-me:
    description: "script rule"
    engine: script
    scope: "**/*.ts"
    severity: error
    script: "true"
  judge-me:
    description: "llm rule"
    engine: semantic
    scope: "**/*.ts"
    severity: error
"#,
    )
    .unwrap();
    let assert = Command::cargo_bin("hector").unwrap()
        .arg("migrate")
        .current_dir(dir.path())
        .assert()
        .success();
    let migrated = fs::read_to_string(dir.path().join(".hector.yml")).unwrap();
    assert!(migrated.contains("keep-me"));
    assert!(!migrated.contains("judge-me"));
    assert.stderr(predicates::str::contains("dropped rule 'judge-me'"));
}
```

Also add `migrate_drops_top_level_llm_block_with_notice` — same shape, a v1 config with an `llm:` block, asserting the block is absent from the output and stderr mentions it.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test cli_migrate`
Expected: FAIL — `judge-me` survives migration today.

- [ ] **Step 3: Implement in migrate.rs**

After the `schema_version` insert (line 30-33), before re-serializing:

```rust
let mut dropped: Vec<(String, String)> = Vec::new();
if let Some(rules) = map
    .get_mut(serde_yaml::Value::String("rules".into()))
    .and_then(|v| v.as_mapping_mut())
{
    let doomed: Vec<serde_yaml::Value> = rules
        .iter()
        .filter_map(|(k, v)| {
            let engine = v.get("engine").and_then(|e| e.as_str())?;
            if matches!(engine, "semantic" | "session") {
                dropped.push((
                    k.as_str().unwrap_or("<non-string id>").to_string(),
                    engine.to_string(),
                ));
                Some(k.clone())
            } else {
                None
            }
        })
        .collect();
    for k in &doomed {
        rules.remove(k);
    }
}
let dropped_llm_block = map
    .remove(serde_yaml::Value::String("llm".into()))
    .is_some();
```

And after the existing `println!` notices:

```rust
for (id, engine) in &dropped {
    eprintln!(
        "note: dropped rule '{id}' — engine '{engine}' was removed in \
         hector 0.2; rewrite it as a script or ast rule if still needed"
    );
}
if dropped_llm_block {
    eprintln!("note: dropped 'llm:' block — LLM evaluation was removed in hector 0.2");
}
```

(Adjust `get_mut`/`remove` call shapes to the workspace's serde_yaml API if the compiler objects; the logic is the contract.)

- [ ] **Step 4: Run tests, clippy (watch complexity), commit**

Run: `cargo test --test cli_migrate && cargo clippy --all-targets -- -D warnings`
If `run` now exceeds the complexity-15 cap, extract the rule-stripping into a `fn strip_removed_rules(map: &mut serde_yaml::Mapping) -> (Vec<(String, String)>, bool)` helper.

```bash
cargo fmt && git add -A && git commit -m "feat(migrate): strip removed semantic/session rules and llm block with notices"
```

---

### Task 4: Remove the CLI surface (flags, subcommands, doctor checks)

**Files:**
- Modify: `crates/hector-cli/src/cli.rs` (drop `print_prompt` lines 63-66, `emit_semantic_payload` lines 67-76, the `session: bool` flag on Check, `Session` subcommand lines 123-127, `RecordVerdict` lines 174-193, `SessionAction` enum lines 204+)
- Modify: `crates/hector-cli/src/main.rs` (drop the matching destructuring at lines 17-57 and the `Session`/`RecordVerdict` match arms)
- Delete: `crates/hector-cli/src/commands/session.rs`, `crates/hector-cli/src/commands/record_verdict.rs`; remove their `pub mod` lines from `commands/mod.rs`
- Modify: `crates/hector-cli/src/commands/check.rs` (drop `session`, `print_prompt`, `emit_semantic_payload` params from `run`; delete `run_session` (lines 63-85), `run_print_prompt`, `run_diff_deferred`, `emit_deferred`, `should_clear_session`; in the `CheckOptions` literal at line 28-33 set `emit_semantic_payload: false` temporarily — the core field dies in Task 5b)
- Modify: `crates/hector-cli/src/commands/doctor.rs` (delete the `needs_llm` block at lines 306-336 and `llm_block_status` at 339-410+; the engines row no longer has an LLM arm)
- Delete tests: `cli_session.rs`, `cli_session_record.rs`, `cli_session_start.rs`, `cli_e2e_record_verdict.rs`
- Trim tests: `cli_e2e_doctor.rs` (LLM-row assertions; re-snapshot `doctor_json_output_snapshot_for_clean_v2_config` via `cargo insta review` if the engines row changed), `cli_check_flags.rs` (any `--session`/conflict assertions), `cli_typed_telemetry.rs` (drop its `hector session`/`record-verdict` invocations — those subcommands die here; keep the check-path typed-variant assertions)

- [ ] **Step 1: Make the edits above.** Follow the compiler: removing the clap variants surfaces every dead consumer. The `#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]` on `check::run` may become removable once three params drop — remove the allow if clippy no longer requires it.

- [ ] **Step 2: Full test run**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: PASS after the listed test deletions. `cargo insta review` if doctor snapshots changed.

- [ ] **Step 3: Commit**

```bash
cargo fmt && git add -A
git commit -m "feat(cli)!: remove session/record-verdict subcommands and semantic check flags"
```

---

### Task 5a: Remove the session engine and session state

**Files:**
- Delete: `crates/hector-core/src/engine/session.rs`, `crates/hector-core/src/session_state.rs`
- Modify: `crates/hector-core/src/engine/mod.rs` (drop `pub mod session;`)
- Modify: `crates/hector-core/src/lib.rs` (drop `pub mod session_state;`)
- Modify: `crates/hector-core/src/runner.rs` (delete `check_session`, `check_session_with_options`, the session accumulator struct around line 218-230, and every helper only they call — grep `session` in runner.rs and delete top-down)
- Delete tests: `session_state.rs`, `session_state_atomicity.rs` (check_session tests already gone in Task 2)

- [ ] **Step 1: Delete the files/modules, follow compile errors until `cargo build` is clean.**
- [ ] **Step 2: `cargo test && cargo clippy --all-targets -- -D warnings`** — expected PASS.
- [ ] **Step 3: Commit:** `git commit -am "feat(core)!: remove session engine and session state"`

---

### Task 5b: Remove the semantic engine, deferred envelope, and llm module

**Files:**
- Delete: `crates/hector-core/src/engine/semantic.rs`, `crates/hector-core/src/engine/context.rs`, `crates/hector-core/src/llm/` (entire dir), `crates/hector-core/src/verdict_deferred.rs`
- Modify: `crates/hector-core/src/engine/mod.rs` — drop `pub mod semantic;`, `pub mod context;`, the `use crate::llm::LlmClient;` import, the `llm: Option<&'a dyn LlmClient>` field on `RuleContext`, and the "(script, semantic)" phrasing in the `RuleEngine` doc comment
- Modify: `crates/hector-core/src/lib.rs` — drop `pub mod llm;`, `pub mod verdict_deferred;`
- Modify: `crates/hector-core/src/config/types.rs` — delete `LlmConfig` (lines 34-57), the `llm: Option<LlmConfig>` field on `Config` (lines 7-8), `ContextScope` (lines 134-140), and the `context: Option<ContextScope>` field on `Rule`
- Modify: `crates/hector-core/src/runner.rs`:
  - `HectorEngine` struct: drop the `llm` field
  - builder: drop `llm` field, `with_llm` (lines 378-381), the llm arg threading in `load_with` (the `build_from_config` match at lines 603-610)
  - delete `try_semantic_skip` (~lines 1494-1584), `append_semantic_verdict`, `render_semantic_prompts`, the deferred split in `RulePartition` (drop the `deferred` field and the partition logic at line 427's `else if engine == EngineKind::Semantic` branch), the `emit_semantic_payload` field on `CheckOptions` and every read of it (line 26 and the `options.emit_semantic_payload` gating)
  - dispatch arm in `evaluate_one_rule` (~line 740) becomes:

```rust
EngineKind::Semantic | EngineKind::Session => Err(anyhow::anyhow!(
    "engine removed in hector 0.2; configs containing it are rejected at load"
)),
```

  - `engine_for` mapping (lines 14-18) — leave the Semantic/Session arms pointing at `Engine::Semantic`/`Engine::Session` for now; Task 6 retargets them when the verdict enum changes.
- Modify: `crates/hector-cli/src/commands/check.rs` — drop the temporary `emit_semantic_payload: false` from the `CheckOptions` literal
- Modify: `Cargo.toml` (workspace) — remove `reqwest` (line 80) and `wiremock` (line 82); `crates/hector-core/Cargo.toml` — remove `reqwest` (line 18) and `wiremock` (line 36); `crates/hector-cli/Cargo.toml` — remove `wiremock` (line 30)
- Delete test: `deferred_verdict_shape.rs` + its snapshot `deferred_verdict_shape__deferred_verdict_with_two_rules_serializes.snap`
- Trim: `builder.rs` test (with_llm fns), `runner_helpers.rs` / `runner_skip.rs` / `runner_diff.rs` if any fn references llm (grep; delete fn-level)

- [ ] **Step 1: Add a regression test for the defensive dispatch arm** (covers the new region for the coverage gate). In `crates/hector-core/tests/runner.rs`, construct a `Rule` directly with the hidden variant and assert the engine errors — mirror the file's existing direct-construction style; the assertion is that `evaluate` of such a rule yields an `__internal` violation / `Err`, whichever the existing harness exposes.
- [ ] **Step 2: Delete/modify per the list, follow compile errors until clean.**
- [ ] **Step 3: `cargo test && cargo clippy --all-targets -- -D warnings`** — expected PASS.
- [ ] **Step 4: Commit:** `git commit -am "feat(core)!: remove semantic engine, deferred envelope, and llm module"`

---

### Task 6: Verdict + telemetry shape change (schema bumps)

**Files:**
- Modify: `crates/hector-core/src/verdict.rs`:
  - `SCHEMA_VERSION: u32 = 3` (line 15), `MIN_REQUIRED_SCHEMA_VERSION: u32 = 3` (line 22)
  - delete `deferred_rules` field + doc (lines 32-44), `DeferredRuleRef` (lines 47-61), and the `deferred_rules: vec![]` initializers (lines 139, 168)
  - `Engine` enum keeps `Script, Ast, Trust, Internal` — delete `Semantic`, `Session` (lines 114-115)
  - reword doc comments that say "LLM unavailable" / "script, semantic, and session engines" (Status::InternalError at ~line 70, Violation::column at ~line 89, Violation::context at ~line 98) to mention only script/ast
- Modify: `crates/hector-core/src/telemetry.rs`: `SCHEMA_VERSION: u32 = 2` (line 24); delete `SessionInit` (line 48), `SemanticVerdict` (line 60), `SemanticSkipped` (line 67) variants and their manual parse arms (lines 108, 114, plus the session_init arm); legacy entries with those type tags now take the existing unknown-type arm — keep that behavior.
- Modify: `crates/hector-core/src/runner.rs` `engine_for` (lines 14-18):

```rust
EngineKind::Script => crate::verdict::Engine::Script,
EngineKind::Ast => crate::verdict::Engine::Ast,
// Parse-only variants rejected by `parse_str`; Internal is the safe
// fallback rather than a panic if one is ever constructed directly.
EngineKind::Semantic | EngineKind::Session => crate::verdict::Engine::Internal,
```

- Delete snapshots: `telemetry__snapshot_semantic_verdict.snap`, `telemetry__snapshot_semantic_skipped.snap`, `telemetry__snapshot_session_init.snap`, `verdict_snapshot__verdict_block_with_deferred_rules_serializes.snap`
- Update tests: `verdict_schema_version.rs` (expects 2 → 3), `verdict_snapshot.rs` (delete the deferred-rules test fn; `cargo insta review` for the schema_version field change in remaining snapshots), `telemetry.rs` + `telemetry_legacy.rs` (drop deleted-variant tests; assert legacy `semantic_verdict`/`session_init` lines in `crates/hector-core/tests/fixtures/log_legacy.jsonl` now parse via the unknown-type path), `cli_typed_telemetry.rs` (already script-only from Task 2 — drop any remaining session-init expectation)

- [ ] **Step 1: Make the edits; run `cargo test`; `cargo insta review` and accept intentional snapshot changes only** (every accepted snapshot diff should be: `schema_version: 3`, absent `deferred_rules`, absent engine tags).
- [ ] **Step 2: `cargo clippy --all-targets -- -D warnings && cargo fmt`**
- [ ] **Step 3: Commit:** `git commit -am "feat(verdict)!: drop semantic/session/deferred from wire shapes; verdict schema 3, telemetry schema 2"`

---

### Task 7: Adapters

**Files:**
- Modify: `adapters/claude-code/hooks/hook.sh` — delete the `session-start` arm (lines 44-49), the `stop` arm (lines 50-130), and within `post-tool-use`: the `hector session record` call (line 182), provider detection (lines 185-189), and the subagent/`--emit-semantic-payload` dispatch branch (lines ~204-260). The surviving hook handles only `post-tool-use`: extract file/old/new → self-edit short-circuit → `synthesize_diff.sh` → `hector check --diff` → exit-2 blocked stderr / exit-3 fail-open (`HECTOR_FAIL_CLOSED_ON_INTERNAL=1` opt-in). **Keep `synthesize_diff.sh`** — it feeds the gating check, not just session recording. Update the header comment block (lines 5-12).
- Modify: `adapters/claude-code/hooks/hooks.json` (or wherever SessionStart/Stop hook events are registered — grep the adapter dir) — remove those event registrations.
- Delete: `adapters/claude-code/agents/hector-evaluator.md`; remove its registration from `adapters/claude-code/plugin.json`.
- Rewrite: `adapters/claude-code/skills/hector/SKILL.md` — blocked-stderr interpretation only; delete the semantic-payload dispatch instructions.
- Modify: `adapters/claude-code/skills/hector-author/SKILL.md`, `hector-init/SKILL.md`, `hector-review/SKILL.md` — remove "convert to semantic"/semantic-rule-type guidance.
- Modify: `adapters/reasonix/hooks/hook.sh` — comment touch-ups only (HEAD has no semantic code paths; grep `semantic\|llm` and clean). Check `adapters/reasonix/hooks/settings.example.json` and `adapters/reasonix/install.sh` the same way.
- Check: `.claude-plugin/marketplace.json` — remove any hector-evaluator/semantic reference if present.
- Trim tests: `crates/hector-cli/tests/adapter_claude_code.rs` — delete `session_start_clears_stale_state` (line 135) and `stop_with_no_session_is_noop` (line 155); the four subagent/direct-api fns are already gone from Task 2. Keep all `synth_*` and `posttooluse_*`/`self_edit_*`/`deterministic_block_*` fns. `adapter_reasonix.rs` should need nothing.

- [ ] **Step 1: Make the edits.** Validate shell syntax: `bash -n adapters/claude-code/hooks/hook.sh adapters/reasonix/hooks/hook.sh`
- [ ] **Step 2: Run the adapter tests**

Run: `cargo test --test adapter_claude_code --test adapter_reasonix`
Expected: PASS.

- [ ] **Step 3: Sweep the adapter dirs**

Run: `grep -rni "semantic\|session\|evaluator\|llm\|emit-semantic\|record-verdict" adapters/ .claude-plugin/`
Expected: no functional hits (comment references to "agent session" in prose are fine; judge each hit).

- [ ] **Step 4: Commit:** `git commit -am "feat(adapters)!: static-gate-only hooks; drop evaluator subagent and session recording"`

---### Task 8: Docs, READMEs, repo guidance, CHANGELOG

**Files:**
- Delete: `docs/reference/emit-semantic-payload.md`, `docs/reference/record-verdict.md`, `docs/writing-rules/asking-an-llm.md`, `docs/configuring/llm-providers.md`
- Update: `docs/architecture.md` (two engines, no llm module), `docs/reference/config-schema.md` (engine enum: script|ast; drop `llm:` block, `context:` field; document the curated rejection), `docs/reference/cli.md` (drop session/record-verdict/--emit-semantic-payload/--print-prompt/--session; exit-3 examples lose "missing API key"), `docs/operating/telemetry.md` (schema 2; drop semantic/session record types), `docs/operating/diagnostics.md` (doctor report loses the LLM row), `docs/adapters/*.md`, `docs/README.md` (fix any dead links to deleted pages — grep for the four deleted filenames), `README.md`
- Update: `CLAUDE.md` (and `AGENTS.md` if it is a separate file, not a symlink): engine list ("Four engines" → two), module list (drop `llm`, `session_state`, `verdict_deferred`, semantic/session engine bullets), the "LLM injection" section (delete), exit-code examples, the `Engine::Trust` convention note (keep — still pre-0.3)
- Update: `CHANGELOG.md` — add under Unreleased:

```markdown
### Removed
- **LLM evaluation.** The `semantic` and `session` engines, the `llm:` config
  block, `--emit-semantic-payload`, `--print-prompt`, `check --session`,
  `hector session`, and `hector record-verdict` are gone. Hector is a static
  gate: `script` + `ast` only. Configs containing the removed engines fail at
  load with a pointed error naming the rule; `hector migrate` drops them with
  a notice. Verdict schema is now 3 (drops `deferred_rules` and the
  `semantic`/`session` engine tags); telemetry schema is now 2.
```

- [ ] **Step 1: Make the edits; grep docs for dangling links:** `grep -rn "emit-semantic-payload\|record-verdict\|asking-an-llm\|llm-providers" docs/ README.md`
Expected: zero hits.
- [ ] **Step 2: Commit:** `git commit -am "docs: drop LLM evaluation from all docs; changelog entry"`

---

### Task 9: Final sweep and verification

- [ ] **Step 1: Repo-wide sweep**

```bash
grep -rni "semantic\|llmclient\|api_key_env\|deferred\|evaluator" \
  --include="*.rs" --include="*.sh" --include="*.md" --include="*.json" --include="*.toml" \
  crates/ adapters/ docs/ README.md .claude-plugin/ \
  | grep -v "specs/\|plans/"
```

Expected remaining hits, exhaustively: the parse-only `EngineKind` variants + gate in `types.rs`/`parser.rs`, the migrate stripping + its tests, `config_removed_engines.rs`, the defensive runner arms, the CHANGELOG entry, and prose uses of unrelated words (e.g. "agent session"). Anything else is a leak — fix it.

- [ ] **Step 2: Full verification**

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo build --release   # smoke: binary builds; spot-check `hector check --help` has no semantic flags
```

Expected: all PASS. Note: `scripts/ci-coverage.sh` cannot run locally (no llvm-tools-preview on Homebrew rustc) — coverage is validated by CI on the PR. Files that lost branches (`runner.rs`, `parser.rs`, `types.rs`, `migrate.rs`, `check.rs`, `doctor.rs`) are the ones to watch in the CI coverage report.

- [ ] **Step 3: Clean build artifacts** (repo rule): `rm target/release/hector` (keep the iterating debug `target/`).

- [ ] **Step 4: Commit any sweep fixes, then request code review** (repo rule: a separate agent reviews completed coding work) before merging/PR.
