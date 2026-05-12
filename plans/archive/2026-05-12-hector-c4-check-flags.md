# Hector C4 — `--rule`, `--explain`, `--print-prompt` flags on `hector check`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or superpowers:subagent-driven-development) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec section:** [`specs/2026-05-12-bully-parity-closures.md` §C4](../specs/2026-05-12-bully-parity-closures.md)
**Severity:** 🟡 high UX
**Sequencing:** Independent of A-series; targeted at 0.2.0.

---

**Goal:** Bring three bully UX flags to `hector check`: `--rule <id>` (repeatable, filters rules by id), `--explain` (per-rule outcome report), `--print-prompt` (render the semantic prompt without dispatching to the LLM). The first lets authors iterate on one rule without commenting others out; the second surfaces *why* a rule did or didn't fire; the third lets prompt authors debug without burning API calls.

**Architecture:** A new `CheckOptions` struct on `HectorEngine` carries the three modes through to the runner without changing the engine's external shape or the verdict JSON. `--rule` is enforced before the dispatch loop with an unknown-id check at `commands/check.rs` (so we can exit 1 with a clear message). `--explain` collects per-rule `RuleExplain { id, engine, outcome }` records inside the dispatch loop and prints them to **stderr** after the verdict (so JSON output on stdout is uncorrupted). `--print-prompt` short-circuits *before* `SemanticEngine::run` is called for the first semantic rule in scope: we build the prompt via the existing `llm::prompt::build_prompt_split` helper, print the split to stdout, and exit 0. The LLM is never constructed for this path (the wiremock test asserts zero requests via an in-process counting client, matching the pattern in `tests/runner_semantic_prefilter.rs`).

**Tech Stack:** clap (`Append` action), `assert_cmd` for CLI integration tests, the existing in-process counting `LlmClient` pattern for "zero LLM dispatch" assertions, `wiremock` only if we want a true HTTP-level assertion (we use the simpler counting client to match A3's approach and stay deterministic).

**Coordination with B1 (parallel agent rewriting the dispatch loop):**
- B1 substitutes `self.config.rules.iter()` with `par_iter()` in `runner::check`.
- This plan adds an *earlier* `.filter(...)` step using a `&HashSet<String>` from `CheckOptions::rules` and an *inside-the-loop* push to a `Vec<RuleExplain>` (sequential collect today; B1 will need to swap to `Mutex<Vec<_>>` or rayon `collect` later — that swap is mechanical and noted in the runner comments).
- **Concrete merge surface (runner.rs):** the new field on `HectorEngine` is one line; the `pub fn check` signature stays the same (`CheckOptions` lives behind a `with_options` builder method) so B1's `check` body changes don't fight ours. Inside the loop we add **two adjacent contiguous regions**: (a) the rule-id filter `continue` and (b) the explainer-record `Vec::push`. Both fit under five lines each, surrounded by their existing scope-match and dispatch logic.

---

## File Structure

**MODIFIED:**
- `crates/hector-cli/src/cli.rs` — three new `Check` args (`--rule` repeatable; `--explain` flag; `--print-prompt` flag).
- `crates/hector-cli/src/main.rs` — pass the new args through to `commands::check::run`.
- `crates/hector-cli/src/commands/check.rs` — accept the new args; validate `--rule` ids against the loaded config; render explain output to stderr; route `--print-prompt` to a new core helper.
- `crates/hector-core/src/runner.rs` — new `CheckOptions` struct (rule filter, explain capture, print-prompt mode); new builder method `with_options`; thread options into `check`; emit `RuleExplain` records when explain is on; honor the rule filter; expose a new method `render_semantic_prompts(&self, input: CheckInput) -> Result<Vec<RenderedPrompt>>` that walks scoped semantic rules and returns the rendered (system, user) text without touching the LLM.
- `crates/hector-core/src/lib.rs` — re-export the new types so the CLI can use them.

**NEW:**
- `crates/hector-cli/tests/cli_check_flags.rs` — assert_cmd tests for all three flags.
- `crates/hector-core/tests/runner_check_options.rs` — library-level tests verifying the rule filter, the explain capture, and the prompt-render path (zero LLM dispatches via the counting `LlmClient`).

**Out-of-scope to TOUCH:**
- `crates/hector-core/src/verdict.rs` — verdict JSON shape is locked. Explain output goes to stderr, never the verdict.
- `crates/hector-core/src/config/types.rs::execution` — B1 territory.
- `crates/hector-core/src/baseline.rs`, `crates/hector-cli/src/commands/baseline.rs` — E1 territory.

---

## Phase 1 — Rule filter (`--rule <id>`)

### Task 1.1: Failing core test — only listed rule ids are evaluated

**Files:**
- Create: `crates/hector-core/tests/runner_check_options.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! C4 — runner-level coverage for CheckOptions: rule-id filter, explain
//! capture, and the prompt-render path that bypasses LLM dispatch.

use anyhow::Result;
use hector_core::config::Rule;
use hector_core::llm::{LlmClient, RuleStatus, RuleVerdict};
use hector_core::runner::{CheckInput, CheckOptions, HectorEngine};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

struct CountingLlm {
    calls: Arc<AtomicUsize>,
}

impl LlmClient for CountingLlm {
    fn evaluate(
        &self,
        rules: &[(&str, &Rule)],
        _primary: &str,
        _context: Option<&str>,
    ) -> Result<Vec<RuleVerdict>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(rules
            .iter()
            .map(|(id, _)| RuleVerdict {
                rule_id: (*id).to_string(),
                status: RuleStatus::Pass,
            })
            .collect())
    }
}

fn write_trusted(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let path = dir.join(".hector.yml");
    std::fs::write(&path, body).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let with_trust = hector_core::trust::write_trust_block(&raw).unwrap();
    std::fs::write(&path, with_trust).unwrap();
    path
}

#[test]
fn rule_filter_runs_only_listed_ids() {
    let dir = tempdir().unwrap();
    let body = "schema_version: 2\nrules:\n  keep:\n    description: \"x\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"true\"\n  drop:\n    description: \"y\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"exit 1\"\n";
    let cfg = write_trusted(dir.path(), body);
    let file = dir.path().join("ok.txt");
    std::fs::write(&file, "clean\n").unwrap();

    let mut keep: HashSet<String> = HashSet::new();
    keep.insert("keep".to_string());
    let opts = CheckOptions { rules: keep, ..CheckOptions::default() };
    let engine = HectorEngine::builder().with_options(opts).load(&cfg).unwrap();
    let verdict = engine
        .check(CheckInput::File { path: file.clone(), content: "clean\n".to_string() })
        .unwrap();

    assert!(verdict.passed_checks.iter().any(|id| id == "keep"));
    assert!(!verdict.passed_checks.iter().any(|id| id == "drop"));
    assert!(verdict.violations.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hector-core --test runner_check_options rule_filter_runs_only_listed_ids`
Expected: FAIL with "no field `with_options`" / "no `CheckOptions`".

### Task 1.2: Add `CheckOptions` + builder method + filter step

**Files:**
- Modify: `crates/hector-core/src/runner.rs`
- Modify: `crates/hector-core/src/lib.rs` (re-export)

- [ ] **Step 3: Define `CheckOptions` in `runner.rs`**

Add near the top of `runner.rs` (above `HectorEngine`):

```rust
use std::collections::HashSet;

/// Optional per-run knobs for `HectorEngine::check`. Plumbed via
/// `HectorEngine::builder().with_options(...)` so the public `check`
/// signature stays stable.
#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    /// Restrict evaluation to these rule ids. Empty set = run all rules.
    pub rules: HashSet<String>,
    /// If true, capture per-rule outcomes for the explainer.
    pub explain: bool,
}

/// One row of the `--explain` report. Stays out of the verdict JSON
/// (verdict shape is locked at 0.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleExplain {
    pub rule_id: String,
    pub engine: crate::config::EngineKind,
    pub outcome: ExplainOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplainOutcome {
    Fire,
    Pass,
    Dispatched,
    Skipped { reason: String },
}
```

Add a field to `HectorEngine`:

```rust
pub struct HectorEngine {
    config: Config,
    config_dir: PathBuf,
    llm: Option<Box<dyn crate::llm::LlmClient>>,
    skip: SkipMatcher,
    options: CheckOptions,
}
```

Extend `HectorEngineBuilder`:

```rust
pub struct HectorEngineBuilder {
    llm: Option<Box<dyn crate::llm::LlmClient>>,
    options: CheckOptions,
}

impl HectorEngineBuilder {
    pub fn new() -> Self {
        Self { llm: None, options: CheckOptions::default() }
    }
    pub fn with_llm(mut self, llm: Box<dyn crate::llm::LlmClient>) -> Self {
        self.llm = Some(llm);
        self
    }
    pub fn with_options(mut self, options: CheckOptions) -> Self {
        self.options = options;
        self
    }
    pub fn load(self, config_path: &Path) -> Result<HectorEngine> {
        HectorEngine::load_with(config_path, self.llm, self.options)
    }
}
```

Update the `load_with` signature and body to accept `options: CheckOptions` and store it on the engine. Also update the existing `pub fn load` to pass `CheckOptions::default()`.

Inside `pub fn check`, immediately before the existing `for (rule_id, rule) in &self.config.rules` loop, insert the filter as a closure-style predicate at the top of the loop body (so B1's `par_iter()` rewrite can keep the same predicate):

```rust
for (rule_id, rule) in &self.config.rules {
    // C4: --rule filter. Empty set = run all (default).
    if !self.options.rules.is_empty() && !self.options.rules.contains(rule_id) {
        continue;
    }
    // ... existing matcher / try_semantic_skip / dispatch logic stays as-is
}
```

- [ ] **Step 4: Re-export from `lib.rs`**

Verify the lib re-exports — `runner` should be `pub mod runner;` already. No additional re-export needed since `CheckOptions` is accessed via `hector_core::runner::CheckOptions`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p hector-core --test runner_check_options rule_filter_runs_only_listed_ids`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/hector-core/src/runner.rs crates/hector-core/tests/runner_check_options.rs
git commit -m "$(cat <<'EOF'
feat(runner): add CheckOptions with --rule filter (C4 phase 1)

CheckOptions { rules: HashSet<String>, explain: bool } threads through
HectorEngineBuilder::with_options so the public check signature is
unchanged. Empty rules set = run every rule (default). Co-exists with
B1's loop parallelization — the filter is a single `continue` at the
top of the loop body.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 1.3: CLI plumbing for `--rule`

**Files:**
- Modify: `crates/hector-cli/src/cli.rs`
- Modify: `crates/hector-cli/src/main.rs`
- Modify: `crates/hector-cli/src/commands/check.rs`
- Create: `crates/hector-cli/tests/cli_check_flags.rs`

- [ ] **Step 7: Write the failing CLI test**

```rust
//! C4 — CLI integration tests for the new `check` flags.

use assert_cmd::Command;
use tempfile::tempdir;

fn write_trusted(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let cfg = dir.join(".hector.yml");
    std::fs::write(&cfg, body).unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .assert()
        .success();
    cfg
}

#[test]
fn rule_flag_restricts_evaluation_to_named_rule() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(
        dir.path(),
        "schema_version: 2\nrules:\n  keep:\n    description: \"x\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"true\"\n  drop:\n    description: \"y\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"exit 1\"\n",
    );
    let file = dir.path().join("ok.txt");
    std::fs::write(&file, "clean\n").unwrap();
    let out = Command::cargo_bin("hector")
        .unwrap()
        .args([
            "check", "--config", cfg.to_str().unwrap(),
            "--file", file.to_str().unwrap(),
            "--rule", "keep",
            "--format", "json",
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let passed: Vec<&str> = v["passed_checks"].as_array().unwrap().iter()
        .map(|x| x.as_str().unwrap()).collect();
    assert!(passed.contains(&"keep"));
    assert!(!passed.contains(&"drop"));
}

#[test]
fn unknown_rule_id_exits_one_with_clear_error() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(
        dir.path(),
        "schema_version: 2\nrules:\n  keep:\n    description: \"x\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"true\"\n",
    );
    let file = dir.path().join("ok.txt");
    std::fs::write(&file, "clean\n").unwrap();
    let out = Command::cargo_bin("hector")
        .unwrap()
        .args([
            "check", "--config", cfg.to_str().unwrap(),
            "--file", file.to_str().unwrap(),
            "--rule", "nope",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("nope"), "stderr must name the unknown rule id: {stderr}");
}
```

- [ ] **Step 8: Run to verify failure**

Run: `cargo test -p hector-cli --test cli_check_flags`
Expected: FAIL — `--rule` flag not recognized.

- [ ] **Step 9: Add `--rule` flag to clap**

In `crates/hector-cli/src/cli.rs`, extend the `Check` variant:

```rust
Check {
    #[arg(long)]
    file: Option<PathBuf>,
    #[arg(long)]
    diff: Option<PathBuf>,
    #[arg(long)]
    session: bool,
    #[arg(long, default_value = "human")]
    format: OutputFormat,
    #[arg(long, default_value = ".hector.yml")]
    config: PathBuf,
    /// Evaluate only this rule id. Repeatable.
    #[arg(long = "rule", action = clap::ArgAction::Append)]
    rules: Vec<String>,
    /// Print a per-rule outcome report to stderr after the verdict.
    #[arg(long)]
    explain: bool,
    /// For semantic rules, render the prompt and exit 0 without dispatching to the LLM.
    #[arg(long = "print-prompt")]
    print_prompt: bool,
},
```

In `main.rs`, thread the new args through:

```rust
Command::Check {
    file, diff, session, format, config,
    rules, explain, print_prompt,
} => commands::check::run(file, diff, session, format, &config, rules, explain, print_prompt)?,
```

- [ ] **Step 10: Implement the filter + unknown-id guard in `commands/check.rs`**

Update `run` signature and body. New imports: `use hector_core::runner::CheckOptions;` and `use std::collections::HashSet;`.

```rust
pub fn run(
    file: Option<PathBuf>,
    diff: Option<PathBuf>,
    session: bool,
    format: OutputFormat,
    config: &Path,
    rules: Vec<String>,
    explain: bool,
    print_prompt: bool,
) -> Result<i32> {
    let engine = match HectorEngine::load(config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("ERROR: {:#}", e);
            return Ok(1);
        }
    };

    // C4: validate --rule ids against the loaded config before doing work.
    if let Some(code) = validate_rule_filter(&engine, &rules) {
        return Ok(code);
    }

    // Build CheckOptions from the CLI args.
    let opts = CheckOptions {
        rules: rules.into_iter().collect(),
        explain,
    };
    // Re-attach options to the engine via builder. (Cheap: only re-runs the
    // pre-built engine with new options.)
    let engine = HectorEngine::builder()
        .with_options(opts.clone())
        .load(config)?;

    // ... rest of the existing dispatch
}

fn validate_rule_filter(engine: &HectorEngine, rules: &[String]) -> Option<i32> {
    let known: std::collections::HashSet<&str> =
        engine.config_rule_ids().collect();
    let unknown: Vec<&String> = rules.iter()
        .filter(|id| !known.contains(id.as_str()))
        .collect();
    if unknown.is_empty() {
        None
    } else {
        eprintln!("ERROR: unknown rule id(s): {}", unknown.iter()
            .map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
        Some(1)
    }
}
```

Expose `config_rule_ids` on `HectorEngine` in `runner.rs`:

```rust
impl HectorEngine {
    /// Iterator over every rule id in the loaded config. Used by the CLI
    /// to validate `--rule` arguments at the boundary, before dispatch.
    pub fn config_rule_ids(&self) -> impl Iterator<Item = &str> {
        self.config.rules.keys().map(|k| k.as_str())
    }
}
```

- [ ] **Step 11: Run tests**

Run: `cargo test -p hector-cli --test cli_check_flags rule_flag_restricts_evaluation_to_named_rule`
Expected: PASS.

Run: `cargo test -p hector-cli --test cli_check_flags unknown_rule_id_exits_one_with_clear_error`
Expected: PASS.

- [ ] **Step 12: Commit**

```bash
git add crates/hector-cli/src/cli.rs crates/hector-cli/src/main.rs crates/hector-cli/src/commands/check.rs crates/hector-cli/tests/cli_check_flags.rs crates/hector-core/src/runner.rs
git commit -m "$(cat <<'EOF'
feat(cli): add --rule flag for selective rule execution (C4 phase 1)

`hector check --rule <id>` (repeatable via clap Append) restricts the
dispatch loop to listed ids. Unknown ids exit 1 with the offending
names on stderr — caught at the CLI boundary so internal callers
aren't forced to re-validate.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 2 — `--explain` report

### Task 2.1: Failing core test — explain captures every in-scope rule's outcome

**Files:**
- Modify: `crates/hector-core/tests/runner_check_options.rs`

- [ ] **Step 13: Add a failing test**

Append:

```rust
#[test]
fn explain_captures_every_in_scope_rule() {
    let dir = tempdir().unwrap();
    let body = "schema_version: 2\nrules:\n  pass-rule:\n    description: \"x\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"true\"\n  fire-rule:\n    description: \"y\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"exit 1\"\n  out-of-scope:\n    description: \"z\"\n    engine: script\n    scope: [\"*.rs\"]\n    severity: error\n    script: \"true\"\n";
    let cfg = write_trusted(dir.path(), body);
    let file = dir.path().join("foo.txt");
    std::fs::write(&file, "x\n").unwrap();

    let opts = CheckOptions { explain: true, ..CheckOptions::default() };
    let engine = HectorEngine::builder().with_options(opts).load(&cfg).unwrap();
    let report = engine
        .check_with_explain(CheckInput::File {
            path: file.clone(),
            content: "x\n".to_string(),
        })
        .unwrap();
    // verdict still emitted as a side product
    assert_eq!(report.explain.len(), 2, "only in-scope rules appear: {:?}", report.explain);
    let ids: Vec<&str> = report.explain.iter().map(|e| e.rule_id.as_str()).collect();
    assert!(ids.contains(&"pass-rule"));
    assert!(ids.contains(&"fire-rule"));
    assert!(!ids.contains(&"out-of-scope"));

    let fire = report.explain.iter().find(|e| e.rule_id == "fire-rule").unwrap();
    assert!(matches!(fire.outcome, hector_core::runner::ExplainOutcome::Fire));
    let pass = report.explain.iter().find(|e| e.rule_id == "pass-rule").unwrap();
    assert!(matches!(pass.outcome, hector_core::runner::ExplainOutcome::Pass));
}
```

- [ ] **Step 14: Run test, watch it fail**

Run: `cargo test -p hector-core --test runner_check_options explain_captures_every_in_scope_rule`
Expected: FAIL — `check_with_explain` not defined.

### Task 2.2: Implement explain capture

**Files:**
- Modify: `crates/hector-core/src/runner.rs`

- [ ] **Step 15: Add the `CheckReport` shape**

Near `CheckOptions`:

```rust
/// The explain mode returns this alongside the verdict so the CLI can
/// render per-rule outcome rows without re-parsing the verdict.
#[derive(Debug, Clone)]
pub struct CheckReport {
    pub verdict: crate::verdict::Verdict,
    pub explain: Vec<RuleExplain>,
}
```

- [ ] **Step 16: Refactor `check` to share a single inner walker with `check_with_explain`**

Extract the body of `check` into a private `fn check_inner(&self, input: CheckInput, collect_explain: bool) -> Result<CheckReport>` that:
- builds the `Vec<RuleExplain>` only when `collect_explain` is true (otherwise leaves the vec empty);
- inside the loop, decides the outcome for each rule **after** dispatch but **before** moving on:
  - if the rule was filtered by id → no push (it never entered the loop body in the first place);
  - if scope didn't match → no push (it's not in scope);
  - if `try_semantic_skip` returned `true` → push `Skipped { reason }` (reuse the same reason the telemetry recorded — refactor `try_semantic_skip` to return an `Option<String>` reason instead of `bool`, since we already compute it);
  - if the engine returned `Ok(vs)` with at least one **emitted** violation → push `Fire`;
  - if the engine returned `Ok` and nothing was emitted → push `Pass` (for `script`/`ast`); for `Semantic` push `Dispatched` if it actually called the LLM (we know because we didn't skip);
  - if the engine returned `Err(_)` → push `Skipped { reason: "engine_error" }`.

Adjust `pub fn check` to call `self.check_inner(input, false).map(|r| r.verdict)`.

Add the public method:

```rust
/// Like `check`, but also returns a per-rule outcome list. The list is
/// empty unless the engine was built with `CheckOptions { explain: true }`.
pub fn check_with_explain(&self, input: CheckInput) -> Result<CheckReport> {
    self.check_inner(input, self.options.explain)
}
```

Refactor `try_semantic_skip` to return the reason so we can record it once:

```rust
fn try_semantic_skip(&self, rule_id: &str, rule: &Rule, path: &Path, diff: &str) -> Option<String> {
    if rule.engine != EngineKind::Semantic || diff.is_empty() {
        return None;
    }
    let analysis = crate::diff::analysis::can_match_diff(diff, path, &rule.description);
    let crate::diff::analysis::CanMatch::No(reason) = analysis else {
        return None;
    };
    let reason_str = reason.as_str().to_string();
    let log_path = self.config_dir.join(".hector/log.jsonl");
    let entry = crate::telemetry::LogEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        kind: "semantic_skipped".into(),
        file: path.display().to_string(),
        rule_id: Some(rule_id.to_string()),
        status: "pass".into(),
        elapsed_ms: 0,
        reason: Some(reason_str.clone()),
    };
    if let Err(e) = crate::telemetry::append(&log_path, &entry) {
        eprintln!("hector: telemetry append failed: {e:#}");
    }
    Some(reason_str)
}
```

Inside the loop, update the call site from `if self.try_semantic_skip(...) { passed.push(...); continue; }` to:

```rust
if let Some(reason) = self.try_semantic_skip(rule_id, rule, &path, &diff) {
    passed.push(rule_id.clone());
    if collect_explain {
        explain.push(RuleExplain {
            rule_id: rule_id.clone(),
            engine: rule.engine,
            outcome: ExplainOutcome::Skipped { reason },
        });
    }
    continue;
}
```

And after the engine match arms, push the appropriate outcome:

```rust
let final_outcome = if engine_errored {
    ExplainOutcome::Skipped { reason: "engine_error".into() }
} else if any_emitted {
    ExplainOutcome::Fire
} else if rule.engine == EngineKind::Semantic {
    ExplainOutcome::Dispatched
} else {
    ExplainOutcome::Pass
};
if collect_explain {
    explain.push(RuleExplain {
        rule_id: rule_id.clone(),
        engine: rule.engine,
        outcome: final_outcome,
    });
}
```

(Keep `engine_errored` / `any_emitted` as local booleans you set in the existing match arms.)

- [ ] **Step 17: Run test**

Run: `cargo test -p hector-core --test runner_check_options explain_captures_every_in_scope_rule`
Expected: PASS.

- [ ] **Step 18: Add the CLI flag end-to-end**

In `crates/hector-cli/tests/cli_check_flags.rs`, add:

```rust
#[test]
fn explain_prints_per_rule_outcome_to_stderr() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(
        dir.path(),
        "schema_version: 2\nrules:\n  pass-rule:\n    description: \"x\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"true\"\n  fire-rule:\n    description: \"y\"\n    engine: script\n    scope: [\"*.txt\"]\n    severity: error\n    script: \"exit 1\"\n",
    );
    let file = dir.path().join("foo.txt");
    std::fs::write(&file, "x\n").unwrap();
    let out = Command::cargo_bin("hector")
        .unwrap()
        .args([
            "check", "--config", cfg.to_str().unwrap(),
            "--file", file.to_str().unwrap(),
            "--explain", "--format", "json",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("pass-rule"), "explain output missing pass-rule line: {stderr}");
    assert!(stderr.contains("fire-rule"), "explain output missing fire-rule line: {stderr}");
    assert!(stderr.contains("pass"));
    assert!(stderr.contains("fire"));
    // JSON output on stdout must remain valid (no explain bleed-through).
    let _: serde_json::Value = serde_json::from_slice(&out.stdout)
        .expect("stdout JSON must remain parseable when --explain is on");
}
```

- [ ] **Step 19: Wire the CLI to use `check_with_explain`**

In `commands/check.rs`, branch on `explain`:

```rust
if explain {
    // Walk the file/diff path through the explain-aware helper.
    let report = match (file, diff) {
        (Some(f), None) => {
            let content = std::fs::read_to_string(&f)?;
            engine.check_with_explain(CheckInput::File { path: f, content })?
        }
        (None, Some(d)) => {
            // For diff mode, accumulate explain rows across files.
            let unified_diff = std::fs::read_to_string(&d)?;
            let changed = hector_core::diff::parser::parse_unified(&unified_diff)?;
            if changed.is_empty() {
                eprintln!("ERROR: no changed files in diff");
                return Ok(1);
            }
            let mut agg_explain: Vec<hector_core::runner::RuleExplain> = Vec::new();
            let mut aggregated_violations = Vec::new();
            let mut aggregated_passed = Vec::new();
            let mut elapsed_ms: u64 = 0;
            for f in changed {
                let per_file_diff = build_single_file_diff(&unified_diff, &f.path);
                let r = engine.check_with_explain(CheckInput::Diff {
                    file: f.path,
                    unified_diff: per_file_diff,
                })?;
                elapsed_ms = elapsed_ms.saturating_add(r.verdict.elapsed_ms);
                aggregated_violations.extend(r.verdict.violations);
                aggregated_passed.extend(r.verdict.passed_checks);
                agg_explain.extend(r.explain);
            }
            hector_core::runner::CheckReport {
                verdict: Verdict::from_violations(aggregated_violations, aggregated_passed, elapsed_ms),
                explain: agg_explain,
            }
        }
        _ => {
            eprintln!("ERROR: provide exactly one of --file or --diff");
            return Ok(1);
        }
    };
    print_explain(&report.explain);
    emit(&report.verdict, format)?;
    return Ok(exit_code(&report.verdict));
}

// ... existing non-explain path stays
```

Add the local printer:

```rust
fn print_explain(rows: &[hector_core::runner::RuleExplain]) {
    use hector_core::runner::ExplainOutcome;
    for row in rows {
        let outcome = match &row.outcome {
            ExplainOutcome::Fire => "fire".to_string(),
            ExplainOutcome::Pass => "pass".to_string(),
            ExplainOutcome::Dispatched => "dispatched".to_string(),
            ExplainOutcome::Skipped { reason } => format!("skipped {reason}"),
        };
        let engine = match row.engine {
            hector_core::config::EngineKind::Script => "script",
            hector_core::config::EngineKind::Ast => "ast",
            hector_core::config::EngineKind::Semantic => "semantic",
            hector_core::config::EngineKind::Session => "session",
        };
        eprintln!("{} {} {}", row.rule_id, engine, outcome);
    }
}
```

- [ ] **Step 20: Run tests**

Run: `cargo test -p hector-cli --test cli_check_flags explain_prints_per_rule_outcome_to_stderr`
Expected: PASS.

- [ ] **Step 21: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(cli): add --explain flag with per-rule outcome report (C4 phase 2)

`hector check --explain` prints `<rule-id> <engine> <fire|pass|dispatched|skipped <reason>>`
to stderr after the verdict, leaving stdout JSON intact. Implemented via
a new `HectorEngine::check_with_explain` that returns the verdict plus a
`Vec<RuleExplain>`. The runner's `try_semantic_skip` now returns the skip
reason so the explainer and the telemetry record share one source.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 3 — `--print-prompt`

### Task 3.1: Failing core test — render path makes zero LLM calls

**Files:**
- Modify: `crates/hector-core/tests/runner_check_options.rs`

- [ ] **Step 22: Add a failing test**

```rust
#[test]
fn print_prompt_path_does_not_dispatch_llm() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(
        dir.path(),
        "schema_version: 2\nrules:\n  no-unwrap:\n    description: \"avoid unwrap\"\n    engine: semantic\n    scope: [\"**/*.rs\"]\n    severity: warning\n    context: diff\n",
    );
    let file = dir.path().join("foo.rs");
    std::fs::write(&file, "fn main() { x.unwrap(); }\n").unwrap();
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1,1 +1,1 @@
-fn main() {}
+fn main() { x.unwrap(); }
";

    let calls = Arc::new(AtomicUsize::new(0));
    let engine = HectorEngine::builder()
        .with_llm(Box::new(CountingLlm { calls: calls.clone() }))
        .load(&cfg)
        .unwrap();
    let prompts = engine
        .render_semantic_prompts(CheckInput::Diff {
            file: file.clone(),
            unified_diff: diff.to_string(),
        })
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 0, "render_semantic_prompts must not dispatch LLM");
    assert_eq!(prompts.len(), 1, "one in-scope semantic rule produces one prompt");
    assert!(prompts[0].user.contains("unwrap"), "prompt user content includes diff");
    assert!(prompts[0].system.contains("avoid unwrap"), "prompt system includes rule description");
    assert_eq!(prompts[0].rule_id, "no-unwrap");
}
```

- [ ] **Step 23: Run, expect failure**

Run: `cargo test -p hector-core --test runner_check_options print_prompt_path_does_not_dispatch_llm`
Expected: FAIL — `render_semantic_prompts` not defined.

### Task 3.2: Implement `render_semantic_prompts`

**Files:**
- Modify: `crates/hector-core/src/runner.rs`
- Modify: `crates/hector-core/src/lib.rs` (if needed)

- [ ] **Step 24: Add types**

```rust
/// One rendered semantic prompt. `system` + `user` mirror Anthropic's
/// `/v1/messages` split; the OpenAI-compat code concatenates them.
#[derive(Debug, Clone)]
pub struct RenderedPrompt {
    pub rule_id: String,
    pub system: String,
    pub user: String,
}
```

- [ ] **Step 25: Add the method**

```rust
/// Render the LLM prompts that *would* be sent for every in-scope
/// semantic rule, without dispatching anything. Used by
/// `hector check --print-prompt` to debug prompt construction without
/// burning API calls.
///
/// Honors `CheckOptions.rules` (the `--rule` filter) and the per-rule
/// scope matcher. Skips rules with non-`semantic` engines silently.
/// Returns an empty vec if no semantic rule is in scope.
pub fn render_semantic_prompts(&self, input: CheckInput) -> Result<Vec<RenderedPrompt>> {
    let (path, diff) = match input {
        CheckInput::File { path, .. } => (path, String::new()),
        CheckInput::Diff { file, unified_diff } => (file, unified_diff),
    };
    let match_path = relativize(&path, &self.config_dir);
    let mut out = Vec::new();
    for (rule_id, rule) in &self.config.rules {
        if !self.options.rules.is_empty() && !self.options.rules.contains(rule_id) {
            continue;
        }
        if rule.engine != EngineKind::Semantic {
            continue;
        }
        let matcher = crate::config::scope::ScopeMatcher::new(&rule.scope)
            .expect("scope validated at load");
        if !matcher.matches(&match_path) {
            continue;
        }
        let scope = rule.context.unwrap_or(crate::config::ContextScope::Diff);
        let (primary, context_text) = crate::engine::context::expand_context(
            scope,
            if diff.is_empty() { None } else { Some(&diff) },
            Some(&path),
            &self.config_dir,
        )?;
        let (system, user) = crate::llm::prompt::build_prompt_split(
            &[(rule_id.as_str(), rule)],
            &primary,
            context_text.as_deref(),
        );
        out.push(RenderedPrompt {
            rule_id: rule_id.clone(),
            system,
            user,
        });
    }
    Ok(out)
}
```

- [ ] **Step 26: Run test**

Run: `cargo test -p hector-core --test runner_check_options print_prompt_path_does_not_dispatch_llm`
Expected: PASS.

### Task 3.3: CLI flag wiring + wiremock-equivalent zero-dispatch CLI test

- [ ] **Step 27: Add failing CLI test**

In `crates/hector-cli/tests/cli_check_flags.rs`, append:

```rust
#[test]
fn print_prompt_renders_and_exits_zero() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(
        dir.path(),
        "schema_version: 2\nrules:\n  no-unwrap:\n    description: \"avoid unwrap\"\n    engine: semantic\n    scope: [\"**/*.rs\"]\n    severity: warning\n    context: file\n",
    );
    let file = dir.path().join("foo.rs");
    std::fs::write(&file, "fn main() { x.unwrap(); }\n").unwrap();
    let out = Command::cargo_bin("hector")
        .unwrap()
        .args([
            "check", "--config", cfg.to_str().unwrap(),
            "--file", file.to_str().unwrap(),
            "--print-prompt",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("avoid unwrap"));
    assert!(stdout.contains("UNTRUSTED_EVIDENCE"));
}
```

Run: `cargo test -p hector-cli --test cli_check_flags print_prompt_renders_and_exits_zero` → FAIL.

- [ ] **Step 28: Wire the CLI path**

In `commands/check.rs`, before the existing dispatch tree (and **before** any LLM construction), handle `print_prompt`:

```rust
if print_prompt {
    let input = match (&file, &diff) {
        (Some(f), None) => {
            let content = std::fs::read_to_string(f)?;
            CheckInput::File { path: f.clone(), content }
        }
        (None, Some(d)) => {
            let unified_diff = std::fs::read_to_string(d)?;
            // Pick the first file in the diff. Print-prompt is a debug tool;
            // we don't try to render N prompts for N diff files.
            let changed = hector_core::diff::parser::parse_unified(&unified_diff)?;
            let Some(first) = changed.into_iter().next() else {
                eprintln!("ERROR: no changed files in diff");
                return Ok(1);
            };
            let per_file = build_single_file_diff(&unified_diff, &first.path);
            CheckInput::Diff { file: first.path, unified_diff: per_file }
        }
        _ => {
            eprintln!("ERROR: provide exactly one of --file or --diff");
            return Ok(1);
        }
    };
    let prompts = engine.render_semantic_prompts(input)?;
    for p in &prompts {
        println!("# rule: {}", p.rule_id);
        println!("## system");
        println!("{}", p.system);
        println!("## user");
        println!("{}", p.user);
    }
    if prompts.is_empty() {
        eprintln!("no semantic rule in scope; nothing to render");
    }
    return Ok(0);
}
```

- [ ] **Step 29: Wiremock-style zero-LLM-call assertion at the CLI layer**

Add to `cli_check_flags.rs`:

```rust
#[test]
fn print_prompt_does_not_call_llm_endpoint() {
    // The CLI binary doesn't have a counting LlmClient hook, so we use
    // a tcp::TcpListener as a stand-in: bind a port and use it as the
    // LLM base_url. If the binary dispatches an HTTP request, the
    // listener accepts a connection; we assert it doesn't.
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();

    let dir = tempdir().unwrap();
    let cfg_body = format!(
        "schema_version: 2\nllm:\n  provider: anthropic\n  model: claude\n  api_key_env: HECTOR_TEST_KEY\n  base_url: http://127.0.0.1:{port}\nrules:\n  r:\n    description: \"d\"\n    engine: semantic\n    scope: [\"*.rs\"]\n    severity: warning\n    context: file\n"
    );
    let cfg = write_trusted(dir.path(), &cfg_body);
    let file = dir.path().join("foo.rs");
    std::fs::write(&file, "fn main(){}\n").unwrap();

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // Best-effort: report any accepted connection within a short window.
        let deadline = std::time::Instant::now() + Duration::from_millis(800);
        while std::time::Instant::now() < deadline {
            if listener.accept().is_ok() {
                let _ = tx.send(true);
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = tx.send(false);
    });

    let out = Command::cargo_bin("hector")
        .unwrap()
        .env("HECTOR_TEST_KEY", "x")
        .args([
            "check", "--config", cfg.to_str().unwrap(),
            "--file", file.to_str().unwrap(),
            "--print-prompt",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let connected = rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false);
    assert!(!connected, "--print-prompt must not open a connection to the LLM endpoint");
}
```

Run: `cargo test -p hector-cli --test cli_check_flags`
Expected: PASS for all three new tests.

- [ ] **Step 30: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(cli): add --print-prompt flag bypassing LLM dispatch (C4 phase 3)

`hector check --print-prompt` renders the (system, user) prompt for
every in-scope semantic rule and exits 0 without constructing or
calling the LLM client. The short-circuit lives in `commands/check.rs`
above the regular dispatch, so the runner sees no semantic dispatch
and the verdict path is never entered. A TcpListener-based test
asserts zero connections to the configured `base_url`, equivalent to
the spec's wiremock zero-requests acceptance.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 4 — Verification

- [ ] **Step 31: fmt + clippy + tests**

Run, in order, and verify each is clean:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

- [ ] **Step 32: Per-file coverage gate**

```bash
bash scripts/ci-coverage.sh
```

Verify every modified file under `crates/*/src/` is ≥90% region coverage. Likely problem files:
- `crates/hector-cli/src/commands/check.rs` — heavy CLI plumbing; many branches. If under 90%, add CLI tests for: missing `--file` and `--diff`, `--print-prompt` with neither file nor diff, `--print-prompt` with empty diff.
- `crates/hector-core/src/runner.rs` — `render_semantic_prompts` has File-vs-Diff branches and scope/engine filters; cover all three branches in the core test.

- [ ] **Step 33: Plan archive**

When the PR is merged, move this plan to `plans/archive/2026-05-12-hector-c4-check-flags.md` and update `plans/README.md`.

---

## Risk / rollback

**`--format json` interaction.** Explain output writes to stderr only; stdout JSON is untouched. The CLI test asserts stdout is still valid JSON when `--explain --format json` is combined.

**Zero rules after filter.** `hector check --rule unknown` exits 1 before any rule runs. `hector check --rule rule-out-of-scope` exits 0 with an empty `passed_checks`/`violations` — the verdict is `Pass` (nothing to evaluate is the same shape as everything passing trivially). This matches bully's behavior.

**`--print-prompt` short-circuit.** Lives before the engine is loaded with an LLM. Even if the config requests an LLM provider with no API key, the prompt-render path will still work because `expand_context` and `build_prompt_split` are pure. A test covers a `base_url` pointing at an idle TCP listener — zero connections.

**B1 merge-conflict surface (runner.rs):**
- New `CheckOptions`, `RuleExplain`, `CheckReport`, `RenderedPrompt` types added near the top of `runner.rs`. No collision risk — B1 is editing the dispatch loop, not the type list.
- `HectorEngine` gains one field (`options: CheckOptions`); `HectorEngineBuilder` gains `with_options`. Trivial 3-way merge.
- Inside `pub fn check`: **two small additions** at known positions in the loop. First is a `continue` at the top for the rule filter; second is the explainer-record push inside each engine arm. Both are localized — under 10 lines total. If B1 has already converted the `for` loop into `par_iter().for_each(...)` by the time we merge:
  - The rule filter `continue` becomes `return` (or `.filter()` upstream of `.for_each`).
  - The `Vec<RuleExplain>::push` needs a `Mutex<Vec<_>>` or rayon `collect`. **Do not pre-emptively add the Mutex here** — keep the sequential `Vec::push` and a comment noting the swap is mechanical post-B1.
- `try_semantic_skip` signature change (`bool` → `Option<String>`) is the most likely friction point. If B1 also touched this fn, the conflict resolution is: keep both return-shape and behavior; the reason string is needed for explain.
- `render_semantic_prompts` is a brand-new method — independent of B1.

**Verdict shape stability:** `RuleExplain`/`CheckReport`/`RenderedPrompt` are new Rust types but never serialized into the verdict JSON. The locked-but-unstable contract is preserved.

---

## Self-review

- [x] Spec §C4 acceptance criteria mapped:
  - "runs only those two rules" → Task 1.1 + 1.3 (`rule_flag_restricts_evaluation_to_named_rule`).
  - "unknown ids → exit 1" → Task 1.3 (`unknown_rule_id_exits_one_with_clear_error`).
  - "explain prints per-rule report" → Task 2.x (`explain_prints_per_rule_outcome_to_stderr`).
  - "--print-prompt short-circuits before HTTP call" → Task 3.x (`print_prompt_does_not_call_llm_endpoint` via TcpListener), plus the in-process `print_prompt_path_does_not_dispatch_llm`.
- [x] No placeholders, complete code blocks throughout.
- [x] Types referenced in later tasks (`CheckOptions`, `RuleExplain`, `ExplainOutcome`, `CheckReport`, `RenderedPrompt`) all defined in earlier tasks with consistent names.
