# IronLint C2 — `ironlint explain <file>` and `ironlint guide <file>`

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` (or `superpowers:subagent-driven-development`) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec section:** [`specs/2026-05-12-bully-parity-closures.md` §C2](../specs/2026-05-12-bully-parity-closures.md)
**Severity:** 🟡 high UX
**Sequencing:** Independent of every other 0.2.x track. Sits on top of A2 (skip patterns) which has already shipped — `explain` re-uses `SkipMatcher` to surface "this file is skipped, here's the pattern that caught it" before reporting per-rule scope outcomes.

---

**Goal:** Bring two read-only inspection subcommands to IronLint at parity with bully. `ironlint explain <file>` answers "why didn't my rule fire on this file?" by walking every resolved rule and printing one greppable line per rule indicating whether the file is in scope, plus a leading `SKIPPED` line when the file matches a built-in or user skip pattern. `ironlint guide <file>` lists the rules that *do* apply to a file with their description and severity, so an author sees the policy surface for a given path without grepping the YAML. Neither command runs a script, calls an LLM, or writes telemetry — they only read config and report scope/skip resolution.

**Architecture:** Both commands lean on a single shared helper, `ironlint_core::runner::scope_outcomes(&engine, &file) -> ScopeOutcomes`, exported from `ironlint-core`. The helper walks the **already-extends-resolved** `BTreeMap<String, Rule>` on `IronLintEngine` once, records the `SkipMatcher` hit (if any) at the file level, then per rule records `(rule_id, ScopeMatch)` where `ScopeMatch` is either `Match { glob }` (the first scope glob that matched) or `NoMatch { scopes: Vec<String> }` (every scope glob the rule registered). Output formatting lives in the per-command modules (`commands/explain.rs`, `commands/guide.rs`). The library helper deliberately returns structured data — not pre-formatted strings — so the JSON-format path and the human-format path render the same data, and so future callers (e.g. an LSP that wants to highlight in-scope rules) can reuse it without re-parsing text.

**Why one helper in `ironlint-core`, not duplicated in `ironlint-cli`:** `explain` and `guide` are both walks of the resolved rule set against a single path; the only difference is what they emit. Putting the walk in core avoids re-implementing scope/skip logic twice in the CLI, keeps `ScopeMatcher`/`SkipMatcher` private to core (where they live today — `pub(crate)` enforces it), and gives library consumers the same view the CLI shows. The alternative (helper in `ironlint-cli`) would force CLI code to re-construct `ScopeMatcher` per rule and reach into `IronLintEngine`'s private fields, which would either bloat `IronLintEngine`'s public surface or push the matcher constructors into `pub`. Core is the right home.

**Tech Stack:** clap (`Subcommand` variants with positional `<file>` arg), `assert_cmd` for CLI integration tests, `insta` for snapshot tests on the JSON format and the multi-line text format, `serde` + `serde_json` for the `--format json` path, the existing `ironlint_core::config::scope::ScopeMatcher` and `ironlint_core::config::skip::SkipMatcher` for the actual matching.

---

## Risk / rollback

**Verdict-schema impact:** **none**. Neither subcommand emits a `Verdict`. The JSON shape they emit (`ExplainEntry[]` and `GuideEntry[]`) is *new*, not a `Verdict` mutation, and the new types live in their own modules (`commands/explain.rs`, `commands/guide.rs`) so they cannot accidentally pollute `verdict.rs`.

**Exit-code-contract impact:** these commands use **0 (success) and 1 (config error). They never use 2.** Spell this out so a future plan doesn't inadvertently reuse the exit-2 surface for advisory output. The contract that exit-2 means *block* is owned by `ironlint check` and must stay that way; `explain` and `guide` are inspection tools, not gates, and they have no concept of "block." The integration tests assert exit code is exactly 0 on every happy-path invocation (including when zero rules are in scope — empty output is success).

**Telemetry-schema impact:** **none**. Both commands are read-only — they do not append to `.ironlint/log.jsonl`. Confirmed by an integration test that runs `ironlint explain <file>` against a fixture project and asserts `.ironlint/log.jsonl` is absent or unchanged afterwards.

**Performance impact:** **trivial**. Both walk the rule set once (O(rules × scope_globs_per_rule)). No engine dispatch, no I/O beyond reading the config. A 100-rule `.ironlint.yml` resolves in well under 10ms.

**Backwards compatibility:** purely additive. New `Explain` and `Guide` variants on the `Command` enum; existing `check`/`trust`/`validate`/`init`/`migrate`/`baseline`/`session` invocations are untouched.

**Coordination with adjacent work:** none — neither C1 (`doctor`), C3 (`show-resolved-config`), nor any of the D-series (`coverage`, `debt`) is in flight. If they land first, the per-rule scope walk in `scope_outcomes` is reusable by `show-resolved-config`'s "rules sorted by id" path; that's a free win, not a coupling.

---

## File Structure

**MODIFIED:**
- `crates/ironlint-cli/src/cli.rs` — add `Explain { file, format, config }` and `Guide { file, format, config }` variants on `Command`.
- `crates/ironlint-cli/src/main.rs` — dispatch the two new variants to `commands::explain::run` / `commands::guide::run`.
- `crates/ironlint-cli/src/commands/mod.rs` — `pub mod explain;` and `pub mod guide;`.
- `crates/ironlint-core/src/runner.rs` — add `pub fn scope_outcomes(&self, file: &Path) -> ScopeOutcomes` and the public `ScopeOutcomes` / `ScopeMatch` / `SkipHit` types.
- `README.md` — short blurb listing `ironlint explain` and `ironlint guide` under the "Commands" section.

**NEW:**
- `crates/ironlint-cli/src/commands/explain.rs` — formatter for the `explain` subcommand. Owns `ExplainEntry` (serde) and the human-format printer.
- `crates/ironlint-cli/src/commands/guide.rs` — formatter for the `guide` subcommand. Owns `GuideEntry` (serde) and the human-format printer.
- `crates/ironlint-cli/tests/cli_e2e_explain.rs` — assert_cmd + insta integration tests for `explain`.
- `crates/ironlint-cli/tests/cli_e2e_guide.rs` — assert_cmd + insta integration tests for `guide`.
- `crates/ironlint-core/tests/runner_scope_outcomes.rs` — library-level tests for the shared helper (covers the three branches: skipped file, in-scope rule, out-of-scope rule).

**Out-of-scope to TOUCH:**
- `crates/ironlint-core/src/verdict.rs` — locked-but-unstable surface. Neither command emits a `Verdict`.
- `crates/ironlint-core/src/telemetry.rs` — these commands write nothing.
- `crates/ironlint-core/src/llm/` — neither command constructs an LLM client.

---

## Phase 1 — Shared `scope_outcomes` helper in `ironlint-core`

### Task 1.1: Failing core test — helper reports the matching scope glob, the skipping pattern, and out-of-scope rules

**Files:**
- Create: `crates/ironlint-core/tests/runner_scope_outcomes.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! C2 — coverage for the read-only `scope_outcomes` helper used by
//! `ironlint explain` and `ironlint guide`. Verifies scope match reporting,
//! skip-pattern resolution, and out-of-scope listing — all without any
//! engine dispatch.

use ironlint_core::runner::{IronLintEngine, ScopeMatch};
use std::path::PathBuf;
use tempfile::tempdir;

fn write_trusted(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join(".ironlint.yml");
    std::fs::write(&path, body).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let with_trust = ironlint_core::trust::write_trust_block(&raw).unwrap();
    std::fs::write(&path, with_trust).unwrap();
    path
}

const THREE_RULE_BODY: &str = "schema_version: 2\nrules:\n  ts-rule:\n    description: \"avoid foo in ts\"\n    engine: script\n    scope: [\"**/*.ts\"]\n    severity: error\n    script: \"true\"\n  rs-rule:\n    description: \"no panic in rust\"\n    engine: script\n    scope: [\"**/*.rs\"]\n    severity: warning\n    script: \"true\"\n  any-md:\n    description: \"docs lint\"\n    engine: script\n    scope: [\"*.md\"]\n    severity: warning\n    script: \"true\"\n";

#[test]
fn scope_outcomes_marks_in_scope_rules_and_lists_out_of_scope_globs() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let engine = IronLintEngine::load(&cfg).unwrap();
    let file = dir.path().join("docs/intro.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "# hi\n").unwrap();

    let outcomes = engine.scope_outcomes(&file);
    assert!(outcomes.skip.is_none(), "intro.md must not be skipped");
    assert_eq!(outcomes.rules.len(), 3, "every resolved rule appears");

    let any_md = outcomes
        .rules
        .iter()
        .find(|r| r.rule_id == "any-md")
        .expect("any-md present");
    match &any_md.scope_match {
        ScopeMatch::Match { glob } => assert_eq!(glob, "*.md"),
        other => panic!("any-md must be a Match, got {other:?}"),
    }
    let ts_rule = outcomes
        .rules
        .iter()
        .find(|r| r.rule_id == "ts-rule")
        .expect("ts-rule present");
    match &ts_rule.scope_match {
        ScopeMatch::NoMatch { scopes } => assert_eq!(scopes, &vec!["**/*.ts".to_string()]),
        other => panic!("ts-rule must be a NoMatch, got {other:?}"),
    }
}

#[test]
fn scope_outcomes_records_skip_hit_for_built_in_lockfile() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let engine = IronLintEngine::load(&cfg).unwrap();
    let lock = dir.path().join("Cargo.lock");
    std::fs::write(&lock, "# generated\n").unwrap();

    let outcomes = engine.scope_outcomes(&lock);
    let hit = outcomes.skip.expect("Cargo.lock must register a skip hit");
    assert_eq!(hit.pattern, "Cargo.lock", "the matching skip pattern surfaces verbatim");
    // Per-rule rows are still produced — `explain` reports them under the
    // SKIPPED banner so the author sees the full scope picture.
    assert_eq!(outcomes.rules.len(), 3);
}

#[test]
fn scope_outcomes_returns_empty_rules_for_config_with_no_rules() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), "schema_version: 2\nrules: {}\n");
    let engine = IronLintEngine::load(&cfg).unwrap();
    let file = dir.path().join("anything.txt");
    std::fs::write(&file, "x\n").unwrap();
    let outcomes = engine.scope_outcomes(&file);
    assert!(outcomes.skip.is_none());
    assert!(outcomes.rules.is_empty());
}
```

- [ ] **Step 2: Run test, verify failure**

Run: `cargo test -p ironlint-core --test runner_scope_outcomes`
Expected: FAIL — `error[E0599]: no method named `scope_outcomes` found for struct `IronLintEngine`` and `error[E0432]: unresolved import `ironlint_core::runner::ScopeMatch``.

### Task 1.2: Add the helper, types, and re-exports

**Files:**
- Modify: `crates/ironlint-core/src/runner.rs`

- [ ] **Step 3: Add the public types near the top of `runner.rs` (above `IronLintEngine`)**

Insert after the existing `RenderedPrompt` struct (search for `pub struct RenderedPrompt`) and before the `RuleOutcome` struct:

```rust
/// C2: snapshot of which rules are in scope for a given file, plus any
/// skip-pattern hit. Returned by [`IronLintEngine::scope_outcomes`] and
/// rendered by `ironlint explain` / `ironlint guide` in the CLI.
///
/// This is the *read-only* counterpart to `check_inner`'s scope walk. No
/// engine runs, no LLM is constructed, no telemetry is written.
#[derive(Debug, Clone)]
pub struct ScopeOutcomes {
    /// `Some(hit)` if the file matches a built-in or user skip pattern.
    /// `explain` prints a `SKIPPED` banner first and *still* enumerates
    /// per-rule rows so the author sees the full scope picture; `guide`
    /// short-circuits to an empty list (skipped files have no applicable
    /// guidance).
    pub skip: Option<SkipHit>,
    /// One entry per rule in the resolved (extends-merged) config, in
    /// `BTreeMap` key order — same iteration order `check_inner` uses, so
    /// the explain output is deterministic and bisectable against
    /// `ironlint check`.
    pub rules: Vec<RuleScopeEntry>,
}

/// C2: which skip pattern (built-in or user-supplied) matched the file.
/// `pattern` is the *raw* glob string the matcher was built from — what
/// the author would put in `skip:` to reproduce or override the hit.
#[derive(Debug, Clone)]
pub struct SkipHit {
    pub pattern: String,
}

/// C2: per-rule scope outcome. `engine`, `severity`, and `description`
/// are mirrored here (cheap clones of `Copy` enums + a `String`) so
/// `guide` can render its `<rule-id> [<severity>] <description>` line
/// without re-borrowing the engine — that lets the helper be called once
/// and the result rendered out into either format.
#[derive(Debug, Clone)]
pub struct RuleScopeEntry {
    pub rule_id: String,
    pub engine: EngineKind,
    pub severity: crate::config::Severity,
    pub description: String,
    pub scope_match: ScopeMatch,
}

/// C2: scope-match outcome for one rule against one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeMatch {
    /// File matches the rule's scope. `glob` is the *first* scope glob
    /// that matched (deterministic — the rule's `scope:` list is iterated
    /// in author order).
    Match { glob: String },
    /// File does not match any of the rule's scope globs. `scopes` is the
    /// rule's full scope list (verbatim) so `explain` can surface them
    /// in the `skip <rule-id> scope=<globs>` line.
    NoMatch { scopes: Vec<String> },
}
```

- [ ] **Step 4: Add an internal helper that finds which raw skip glob matched**

The existing `SkipMatcher::matches` returns a bool — for `explain` we need to know *which* pattern hit. Rather than expand `SkipMatcher`'s API (a separate concern; `check_inner` only needs the bool), add a small private function in `runner.rs` that walks the union of built-in + user skip globs and returns the first hit. This keeps `SkipMatcher` unchanged and confines the per-pattern walk to the path that needs it.

Insert near the bottom of the existing `runner.rs` `impl IronLintEngine` block (or as a free function near `home_dir`):

```rust
/// C2: identify which raw skip glob matched a path. Mirrors the
/// construction order in `SkipMatcher::with_built_ins` (built-ins first,
/// user extras second) so the reported pattern matches what the author
/// would type to reproduce the skip. Returns `None` when no pattern
/// matches — the caller should treat that as "file is in scope for the
/// usual rule walk." Silently returns `None` on any glob construction
/// error, since the same globs already round-tripped through
/// `SkipMatcher::with_built_ins` at engine load time.
fn first_matching_skip_glob(file: &std::path::Path, extras: &[String]) -> Option<String> {
    use globset::{Glob, GlobSetBuilder};
    let candidates: Vec<String> = crate::config::skip::built_in_skip_globs()
        .iter()
        .map(|s| (*s).to_string())
        .chain(extras.iter().cloned())
        .collect();
    for raw in candidates {
        let mut b = GlobSetBuilder::new();
        let Ok(g) = Glob::new(&raw) else {
            continue;
        };
        b.add(g);
        if !raw.contains('/') {
            let Ok(g2) = Glob::new(&format!("**/{raw}")) else {
                continue;
            };
            b.add(g2);
        } else if let Some(prefix) = raw.strip_suffix("/**") {
            if !prefix.is_empty() && !prefix.contains('*') {
                let Ok(g3) = Glob::new(&format!("**/{prefix}/**")) else {
                    continue;
                };
                b.add(g3);
            }
        }
        let Ok(set) = b.build() else { continue };
        if set.is_match(file) {
            return Some(raw);
        }
    }
    None
}
```

- [ ] **Step 5: Add the public `scope_outcomes` method on `IronLintEngine`**

Insert inside `impl IronLintEngine` (after `config_rule_ids` is a good neighbor):

```rust
impl IronLintEngine {
    /// C2: read-only scope walk. Returns the skip-pattern hit (if any)
    /// and a per-rule scope outcome for every rule in the resolved config.
    /// No engine runs; no LLM is constructed; no telemetry is written.
    ///
    /// Used by `ironlint explain <file>` and `ironlint guide <file>` so they
    /// share one source of truth for "what's in scope for this path?"
    /// with `ironlint check`'s dispatch loop. The path is relativized
    /// against the config dir using the same fallback rules as the
    /// regular check path, so an absolute `/etc/passwd` and a relative
    /// `etc/passwd` produce the same per-rule outcome shape.
    pub fn scope_outcomes(&self, file: &std::path::Path) -> ScopeOutcomes {
        let match_path = relativize(file, &self.config_dir);

        // Skip resolution. Mirror `load_with`'s extras assembly so the
        // helper sees the same union of project + user-global globs.
        let mut extras = self.config.skip.clone();
        if let Some(home) = home_dir() {
            let ignore_path = home.join(USER_GLOBAL_IGNORE_FILENAME);
            if let Ok(raw) = std::fs::read_to_string(&ignore_path) {
                extras.extend(parse_user_global_ignore(&raw));
            }
        }
        let skip = first_matching_skip_glob(&match_path, &extras).map(|pattern| SkipHit { pattern });

        let mut rules: Vec<RuleScopeEntry> = Vec::with_capacity(self.config.rules.len());
        for (rule_id, rule) in &self.config.rules {
            let matched = first_matching_scope_glob(&rule.scope, &match_path);
            let scope_match = match matched {
                Some(glob) => ScopeMatch::Match { glob },
                None => ScopeMatch::NoMatch {
                    scopes: rule.scope.clone(),
                },
            };
            rules.push(RuleScopeEntry {
                rule_id: rule_id.clone(),
                engine: rule.engine,
                severity: rule.severity,
                description: rule.description.clone(),
                scope_match,
            });
        }
        ScopeOutcomes { skip, rules }
    }
}

/// C2: walk a rule's scope list in author order and return the first
/// glob that matches `path`. Returns `None` if no glob matches. Mirrors
/// the right-anchored bare-pattern semantics of
/// `crate::config::scope::ScopeMatcher` (a bare `*.py` also matches at
/// any depth via the `**/<pattern>` form).
fn first_matching_scope_glob(scopes: &[String], path: &std::path::Path) -> Option<String> {
    use globset::{Glob, GlobSetBuilder};
    for raw in scopes {
        let mut b = GlobSetBuilder::new();
        let Ok(g) = Glob::new(raw) else { continue };
        b.add(g);
        if !raw.contains('/') {
            let Ok(g2) = Glob::new(&format!("**/{raw}")) else {
                continue;
            };
            b.add(g2);
        }
        let Ok(set) = b.build() else { continue };
        if set.is_match(path) {
            return Some(raw.clone());
        }
    }
    None
}
```

- [ ] **Step 6: Run the test, verify green**

Run: `cargo test -p ironlint-core --test runner_scope_outcomes`
Expected: PASS — all three tests green.

- [ ] **Step 7: Commit**

```bash
git add crates/ironlint-core/src/runner.rs crates/ironlint-core/tests/runner_scope_outcomes.rs
git commit -m "$(cat <<'EOF'
feat(core): add scope_outcomes helper for explain/guide (C2 phase 1)

`IronLintEngine::scope_outcomes(&file)` is the read-only scope walk
shared by `ironlint explain` and `ironlint guide`. Returns a `ScopeOutcomes`
with the matching skip pattern (if any) plus a per-rule
`RuleScopeEntry` enumerating which scope glob matched, in `BTreeMap`
key order so output is deterministic.

No engine dispatch, no LLM construction, no telemetry write. The
helper lives in core (not the CLI) so library consumers can reuse the
same view the CLI shows without going through `IronLintEngine`'s private
matcher fields.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 2 — `ironlint explain <file>`

### Task 2.1: Failing CLI test — happy-path `explain` lists matches and skips per rule

**Files:**
- Create: `crates/ironlint-cli/tests/cli_e2e_explain.rs`

- [ ] **Step 8: Write the failing CLI test**

```rust
//! C2 — CLI integration tests for `ironlint explain <file>`.

use assert_cmd::Command;
use tempfile::tempdir;

fn write_trusted(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let cfg = dir.join(".ironlint.yml");
    std::fs::write(&cfg, body).unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .assert()
        .success();
    cfg
}

const THREE_RULE_BODY: &str = "schema_version: 2\nrules:\n  ts-rule:\n    description: \"avoid foo in ts\"\n    engine: script\n    scope: [\"**/*.ts\"]\n    severity: error\n    script: \"true\"\n  rs-rule:\n    description: \"no panic in rust\"\n    engine: script\n    scope: [\"**/*.rs\"]\n    severity: warning\n    script: \"true\"\n  any-md:\n    description: \"docs lint\"\n    engine: script\n    scope: [\"*.md\"]\n    severity: warning\n    script: \"true\"\n";

#[test]
fn explain_prints_match_and_skip_lines_for_a_markdown_file() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let file = dir.path().join("docs/intro.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "# hi\n").unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "explain",
            "--config",
            cfg.to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);

    // The match line uses `MATCH` (uppercase) so it greps distinctly.
    assert!(
        stdout.contains("MATCH any-md via *.md"),
        "expected `MATCH any-md via *.md` in stdout, got: {stdout}"
    );
    // Non-matching rules use lowercase `skip` (also distinct under grep).
    assert!(
        stdout.contains("skip ts-rule scope=**/*.ts"),
        "expected `skip ts-rule scope=**/*.ts` in stdout, got: {stdout}"
    );
    assert!(
        stdout.contains("skip rs-rule scope=**/*.rs"),
        "expected `skip rs-rule scope=**/*.rs` in stdout, got: {stdout}"
    );
}

#[test]
fn explain_emits_skipped_banner_for_lockfile() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let lock = dir.path().join("Cargo.lock");
    std::fs::write(&lock, "# generated\n").unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "explain",
            "--config",
            cfg.to_str().unwrap(),
            lock.to_str().unwrap(),
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);

    assert!(
        stdout.contains("SKIPPED"),
        "lockfile must produce a SKIPPED banner: {stdout}"
    );
    assert!(
        stdout.contains("Cargo.lock"),
        "the SKIPPED banner names the matching skip pattern: {stdout}"
    );
    // Per-rule rows still emit so the author sees the full picture.
    assert!(stdout.contains("skip any-md scope=*.md"));
}

#[test]
fn explain_missing_config_exits_one() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("foo.md");
    std::fs::write(&file, "x\n").unwrap();
    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "explain",
            "--config",
            dir.path().join(".ironlint.yml").to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ERROR"),
        "missing config must surface a stderr ERROR hint: {stderr}"
    );
}
```

- [ ] **Step 9: Run test, verify failure**

Run: `cargo test -p ironlint-cli --test cli_e2e_explain`
Expected: FAIL — clap rejects the unknown `explain` subcommand. The expected stderr fragment from clap will include `error: unrecognized subcommand 'explain'`.

### Task 2.2: Add the clap variant

**Files:**
- Modify: `crates/ironlint-cli/src/cli.rs`

- [ ] **Step 10: Add `Explain` variant**

Add inside the `Command` enum, after the `Session { … }` variant:

```rust
    /// Show which rules are in scope for `<file>` and which skip-pattern
    /// (if any) suppresses it. Read-only — no engine runs, no LLM is
    /// called, no telemetry is written.
    Explain {
        /// Path to inspect. Relative to cwd.
        file: PathBuf,
        #[arg(long, default_value = "human")]
        format: OutputFormat,
        #[arg(long, default_value = ".ironlint.yml")]
        config: PathBuf,
    },
    /// List the rules whose scope matches `<file>` with their description
    /// and severity. Read-only — see `explain` for full scope reporting.
    Guide {
        /// Path to inspect. Relative to cwd.
        file: PathBuf,
        #[arg(long, default_value = "human")]
        format: OutputFormat,
        #[arg(long, default_value = ".ironlint.yml")]
        config: PathBuf,
    },
```

(Both variants are added in this step so we don't churn the enum twice. The `Guide` arm is wired up in Phase 3; for now `main.rs` simply needs to dispatch `Explain` and we'll add the `Guide` dispatch in Phase 3.)

### Task 2.3: Add `commands/explain.rs` module

**Files:**
- Modify: `crates/ironlint-cli/src/commands/mod.rs`
- Create: `crates/ironlint-cli/src/commands/explain.rs`
- Modify: `crates/ironlint-cli/src/main.rs`

- [ ] **Step 11: Register the module**

Edit `crates/ironlint-cli/src/commands/mod.rs` to add:

```rust
pub mod baseline;
pub mod check;
pub mod explain;
pub mod guide;
pub mod init;
pub mod migrate;
pub mod session;
pub mod trust;
pub mod validate;
```

(Both `explain` and `guide` are added now so the module list is settled; `guide.rs` lands in Phase 3.)

- [ ] **Step 12: Write `crates/ironlint-cli/src/commands/explain.rs` in full**

```rust
//! C2: `ironlint explain <file>` — read-only scope/skip resolution report.
//!
//! Output contract:
//! * Stdout is greppable. One rule per line. A `MATCH <id> via <glob>`
//!   line for in-scope rules; a `skip <id> scope=<glob,…>` line for
//!   out-of-scope rules. Distinct casing (`MATCH` vs `skip`) lets a user
//!   `grep '^MATCH'` to filter to just the rules that fire.
//! * When the file matches a skip pattern, a leading
//!   `SKIPPED <file> via <skip-pattern>` banner is emitted on stdout
//!   *before* the per-rule rows. The per-rule rows still print so the
//!   author sees the full scope picture.
//! * Errors (missing config, untrusted config) go to stderr; exit 1.
//! * `--format json` emits a single JSON value to stdout instead of the
//!   line-oriented format. Schema below.

use crate::cli::OutputFormat;
use anyhow::Result;
use ironlint_core::runner::{IronLintEngine, ScopeMatch, ScopeOutcomes};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// JSON shape emitted by `--format json`. Stable for tooling.
#[derive(Debug, Serialize)]
pub struct ExplainOutput {
    /// `Some({"pattern": …})` if the file matches a skip pattern.
    pub skip: Option<SkipJson>,
    /// One entry per rule, in `BTreeMap` key order.
    pub rules: Vec<ExplainEntry>,
}

#[derive(Debug, Serialize)]
pub struct SkipJson {
    pub pattern: String,
}

#[derive(Debug, Serialize)]
pub struct ExplainEntry {
    pub rule_id: String,
    /// Either `"match"` or `"skip"`. Stable string value (do not switch
    /// to a structured enum without bumping a doc'd schema version).
    pub status: String,
    /// `Some(glob)` when `status == "match"`; `None` otherwise.
    pub matched_glob: Option<String>,
    /// `Some(scopes)` when `status == "skip"`; `None` when matched.
    pub scopes: Option<Vec<String>>,
}

pub fn run(file: PathBuf, format: OutputFormat, config: &Path) -> Result<i32> {
    let engine = match IronLintEngine::load(config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("ERROR: {:#}", e);
            return Ok(1);
        }
    };
    let outcomes = engine.scope_outcomes(&file);
    match format {
        OutputFormat::Human => emit_human(&file, &outcomes),
        OutputFormat::Json => emit_json(&outcomes)?,
    }
    Ok(0)
}

fn emit_human(file: &Path, outcomes: &ScopeOutcomes) {
    if let Some(hit) = &outcomes.skip {
        println!("SKIPPED {} via {}", file.display(), hit.pattern);
    }
    for entry in &outcomes.rules {
        match &entry.scope_match {
            ScopeMatch::Match { glob } => {
                println!("MATCH {} via {}", entry.rule_id, glob);
            }
            ScopeMatch::NoMatch { scopes } => {
                println!("skip {} scope={}", entry.rule_id, scopes.join(","));
            }
        }
    }
}

fn emit_json(outcomes: &ScopeOutcomes) -> Result<()> {
    let out = ExplainOutput {
        skip: outcomes
            .skip
            .as_ref()
            .map(|h| SkipJson { pattern: h.pattern.clone() }),
        rules: outcomes
            .rules
            .iter()
            .map(|r| match &r.scope_match {
                ScopeMatch::Match { glob } => ExplainEntry {
                    rule_id: r.rule_id.clone(),
                    status: "match".to_string(),
                    matched_glob: Some(glob.clone()),
                    scopes: None,
                },
                ScopeMatch::NoMatch { scopes } => ExplainEntry {
                    rule_id: r.rule_id.clone(),
                    status: "skip".to_string(),
                    matched_glob: None,
                    scopes: Some(scopes.clone()),
                },
            })
            .collect(),
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironlint_core::runner::{RuleScopeEntry, ScopeOutcomes, SkipHit};

    fn match_entry(id: &str, glob: &str) -> RuleScopeEntry {
        RuleScopeEntry {
            rule_id: id.to_string(),
            engine: ironlint_core::config::EngineKind::Script,
            severity: ironlint_core::config::Severity::Warning,
            description: "d".to_string(),
            scope_match: ScopeMatch::Match { glob: glob.to_string() },
        }
    }

    fn skip_entry(id: &str, scopes: &[&str]) -> RuleScopeEntry {
        RuleScopeEntry {
            rule_id: id.to_string(),
            engine: ironlint_core::config::EngineKind::Script,
            severity: ironlint_core::config::Severity::Warning,
            description: "d".to_string(),
            scope_match: ScopeMatch::NoMatch {
                scopes: scopes.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    #[test]
    fn json_shape_distinguishes_match_and_skip_status_strings() {
        let outcomes = ScopeOutcomes {
            skip: Some(SkipHit { pattern: "Cargo.lock".into() }),
            rules: vec![match_entry("a", "*.md"), skip_entry("b", &["**/*.ts"])],
        };
        // Build the value the same way `emit_json` does (we can't capture
        // stdout from a unit test cleanly, so we reconstruct the JSON
        // value and assert on it).
        let out = ExplainOutput {
            skip: outcomes.skip.as_ref().map(|h| SkipJson { pattern: h.pattern.clone() }),
            rules: outcomes
                .rules
                .iter()
                .map(|r| match &r.scope_match {
                    ScopeMatch::Match { glob } => ExplainEntry {
                        rule_id: r.rule_id.clone(),
                        status: "match".into(),
                        matched_glob: Some(glob.clone()),
                        scopes: None,
                    },
                    ScopeMatch::NoMatch { scopes } => ExplainEntry {
                        rule_id: r.rule_id.clone(),
                        status: "skip".into(),
                        matched_glob: None,
                        scopes: Some(scopes.clone()),
                    },
                })
                .collect(),
        };
        let v = serde_json::to_value(&out).unwrap();
        assert_eq!(v["skip"]["pattern"], "Cargo.lock");
        assert_eq!(v["rules"][0]["status"], "match");
        assert_eq!(v["rules"][0]["matched_glob"], "*.md");
        assert!(v["rules"][0]["scopes"].is_null());
        assert_eq!(v["rules"][1]["status"], "skip");
        assert!(v["rules"][1]["matched_glob"].is_null());
        assert_eq!(v["rules"][1]["scopes"][0], "**/*.ts");
    }
}
```

- [ ] **Step 13: Wire `Explain` dispatch in `main.rs`**

Add the `Explain` arm to the `match cli.command` block in `main.rs`:

```rust
        Command::Explain { file, format, config } => {
            commands::explain::run(file, format, &config)?
        }
        Command::Guide { file, format, config } => {
            commands::guide::run(file, format, &config)?
        }
```

(`Guide` is included now even though `commands/guide.rs` doesn't exist yet — the build will fail until Phase 3 lands. We add the dispatch arm here so we touch `main.rs` once.)

**Note:** Because `commands::guide` doesn't compile yet, **Phase 2's task 2.4 can't be split from Phase 3's task 3.1** — Phase 3 must land in the same review cycle. If a reviewer wants smaller commits, comment out the `Guide` arm and `pub mod guide;` line until Phase 3, then uncomment.

### Task 2.4: Run tests, commit

- [ ] **Step 14: Run integration tests**

The `Guide` dispatch arm references a module that doesn't exist. To keep Phase 2 self-contained for `cargo test`, create a stub `commands/guide.rs` now with only a `pub fn run` that returns `Ok(1)` — Phase 3 will overwrite it:

```rust
//! C2 phase 2 stub — replaced wholesale in Phase 3.

use crate::cli::OutputFormat;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn run(_file: PathBuf, _format: OutputFormat, _config: &Path) -> Result<i32> {
    eprintln!("ERROR: ironlint guide is not yet implemented (C2 phase 3)");
    Ok(1)
}
```

Run: `cargo test -p ironlint-cli --test cli_e2e_explain`
Expected: PASS — all three explain tests green.

- [ ] **Step 15: Commit**

```bash
git add crates/ironlint-cli/src/cli.rs crates/ironlint-cli/src/main.rs crates/ironlint-cli/src/commands/mod.rs crates/ironlint-cli/src/commands/explain.rs crates/ironlint-cli/src/commands/guide.rs crates/ironlint-cli/tests/cli_e2e_explain.rs
git commit -m "$(cat <<'EOF'
feat(cli): add ironlint explain subcommand (C2 phase 2)

`ironlint explain <file>` prints one greppable line per resolved rule:
`MATCH <id> via <glob>` for in-scope rules, `skip <id> scope=<globs>`
for out-of-scope rules. When the file hits a skip pattern (built-in or
user-supplied) a leading `SKIPPED <file> via <pattern>` banner names
the offending pattern.

`--format json` emits a stable `{ skip, rules: [{ rule_id, status,
matched_glob?, scopes? }] }` shape. Status string is `"match"` or
`"skip"`.

Read-only: no engine runs, no LLM is constructed, no telemetry is
written. Exit codes are 0 (success) and 1 (config error) — never 2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 3 — `ironlint guide <file>`

### Task 3.1: Failing CLI test — `guide` lists in-scope rules with severity and description

**Files:**
- Create: `crates/ironlint-cli/tests/cli_e2e_guide.rs`

- [ ] **Step 16: Write the failing test**

```rust
//! C2 — CLI integration tests for `ironlint guide <file>`.

use assert_cmd::Command;
use tempfile::tempdir;

fn write_trusted(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let cfg = dir.join(".ironlint.yml");
    std::fs::write(&cfg, body).unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .assert()
        .success();
    cfg
}

const THREE_RULE_BODY: &str = "schema_version: 2\nrules:\n  ts-rule:\n    description: \"avoid foo in ts\"\n    engine: script\n    scope: [\"**/*.ts\"]\n    severity: error\n    script: \"true\"\n  rs-rule:\n    description: \"no panic in rust\"\n    engine: script\n    scope: [\"**/*.rs\"]\n    severity: warning\n    script: \"true\"\n  any-md:\n    description: \"docs lint\"\n    engine: script\n    scope: [\"*.md\"]\n    severity: warning\n    script: \"true\"\n";

#[test]
fn guide_lists_in_scope_rules_only_with_severity_and_description() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let file = dir.path().join("docs/intro.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "# hi\n").unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "guide",
            "--config",
            cfg.to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);

    // Only `any-md` is in scope — rs-rule and ts-rule must not appear.
    assert!(
        stdout.contains("any-md [warning] docs lint"),
        "expected `any-md [warning] docs lint`, got: {stdout}"
    );
    assert!(
        !stdout.contains("ts-rule"),
        "out-of-scope rule must not appear in guide output: {stdout}"
    );
    assert!(
        !stdout.contains("rs-rule"),
        "out-of-scope rule must not appear in guide output: {stdout}"
    );
}

#[test]
fn guide_skipped_file_emits_banner_and_no_rules() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let lock = dir.path().join("Cargo.lock");
    std::fs::write(&lock, "# generated\n").unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "guide",
            "--config",
            cfg.to_str().unwrap(),
            lock.to_str().unwrap(),
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);
    assert!(stdout.contains("SKIPPED"), "expected SKIPPED banner: {stdout}");
    assert!(stdout.contains("Cargo.lock"));
    // No rule lines for skipped files.
    assert!(!stdout.contains("[warning]"), "skipped files have no guidance rows: {stdout}");
    assert!(!stdout.contains("[error]"));
}

#[test]
fn guide_output_is_sorted_by_rule_id() {
    let dir = tempdir().unwrap();
    let body = "schema_version: 2\nrules:\n  zeta:\n    description: \"z\"\n    engine: script\n    scope: [\"*.md\"]\n    severity: warning\n    script: \"true\"\n  alpha:\n    description: \"a\"\n    engine: script\n    scope: [\"*.md\"]\n    severity: error\n    script: \"true\"\n  middle:\n    description: \"m\"\n    engine: script\n    scope: [\"*.md\"]\n    severity: warning\n    script: \"true\"\n";
    let cfg = write_trusted(dir.path(), body);
    let file = dir.path().join("readme.md");
    std::fs::write(&file, "x\n").unwrap();
    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "guide",
            "--config",
            cfg.to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);
    let alpha_at = stdout.find("alpha").expect("alpha row present");
    let middle_at = stdout.find("middle").expect("middle row present");
    let zeta_at = stdout.find("zeta").expect("zeta row present");
    assert!(alpha_at < middle_at, "alpha must precede middle: {stdout}");
    assert!(middle_at < zeta_at, "middle must precede zeta: {stdout}");
}

#[test]
fn guide_missing_config_exits_one() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("foo.md");
    std::fs::write(&file, "x\n").unwrap();
    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "guide",
            "--config",
            dir.path().join(".ironlint.yml").to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ERROR"));
}
```

- [ ] **Step 17: Run, verify failure**

Run: `cargo test -p ironlint-cli --test cli_e2e_guide`
Expected: FAIL — the Phase-2 stub returns exit 1 for every invocation, so all four tests fail (three with code != 0 / wrong stdout, one passing the missing-config exit-1 case but still failing the "stderr contains ERROR" assertion since the stub message mentions "not yet implemented" but starts with "ERROR" so that one might actually pass — that's fine, the stub still fails the others which is enough red signal).

### Task 3.2: Implement `commands/guide.rs` in full

**Files:**
- Modify: `crates/ironlint-cli/src/commands/guide.rs` (replace stub)

- [ ] **Step 18: Write the full module**

Replace the stub with:

```rust
//! C2: `ironlint guide <file>` — list rules whose scope matches `<file>`,
//! with severity and description. Read-only.
//!
//! Output contract:
//! * Stdout is one rule per line: `<rule-id> [<severity>] <description>`.
//!   Severity in lowercase brackets (`[error]` / `[warning]`) so it
//!   matches the `Severity` enum's serialized form. Simple space
//!   separation rather than fixed columns — bully chose this; it keeps
//!   `awk '{print $1}'` working as the basic id-extraction recipe.
//! * Rules are sorted by id (deterministic; the underlying
//!   `BTreeMap<String, Rule>` already iterates in key order, so no
//!   re-sort is needed — the test asserts the property explicitly).
//! * When the file matches a skip pattern, a leading
//!   `SKIPPED <file> via <pattern>` banner is emitted and *no* rule
//!   rows follow (skipped files have no applicable guidance).
//! * Errors (missing/untrusted config) go to stderr; exit 1.
//! * `--format json` emits `{ skip, rules: [{ rule_id, severity,
//!   description }] }` to stdout.

use crate::cli::OutputFormat;
use anyhow::Result;
use ironlint_core::config::Severity;
use ironlint_core::runner::{IronLintEngine, ScopeMatch, ScopeOutcomes};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct GuideOutput {
    pub skip: Option<SkipJson>,
    pub rules: Vec<GuideEntry>,
}

#[derive(Debug, Serialize)]
pub struct SkipJson {
    pub pattern: String,
}

#[derive(Debug, Serialize)]
pub struct GuideEntry {
    pub rule_id: String,
    /// Stable lowercase string: `"error"` or `"warning"`.
    pub severity: String,
    pub description: String,
}

pub fn run(file: PathBuf, format: OutputFormat, config: &Path) -> Result<i32> {
    let engine = match IronLintEngine::load(config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("ERROR: {:#}", e);
            return Ok(1);
        }
    };
    let outcomes = engine.scope_outcomes(&file);
    match format {
        OutputFormat::Human => emit_human(&file, &outcomes),
        OutputFormat::Json => emit_json(&outcomes)?,
    }
    Ok(0)
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

fn emit_human(file: &Path, outcomes: &ScopeOutcomes) {
    if let Some(hit) = &outcomes.skip {
        println!("SKIPPED {} via {}", file.display(), hit.pattern);
        return;
    }
    for entry in &outcomes.rules {
        if matches!(entry.scope_match, ScopeMatch::Match { .. }) {
            println!(
                "{} [{}] {}",
                entry.rule_id,
                severity_str(entry.severity),
                entry.description
            );
        }
    }
}

fn emit_json(outcomes: &ScopeOutcomes) -> Result<()> {
    let out = GuideOutput {
        skip: outcomes
            .skip
            .as_ref()
            .map(|h| SkipJson { pattern: h.pattern.clone() }),
        rules: if outcomes.skip.is_some() {
            Vec::new()
        } else {
            outcomes
                .rules
                .iter()
                .filter(|r| matches!(r.scope_match, ScopeMatch::Match { .. }))
                .map(|r| GuideEntry {
                    rule_id: r.rule_id.clone(),
                    severity: severity_str(r.severity).to_string(),
                    description: r.description.clone(),
                })
                .collect()
        },
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironlint_core::runner::{RuleScopeEntry, ScopeOutcomes, SkipHit};

    fn entry(id: &str, sev: Severity, desc: &str, sm: ScopeMatch) -> RuleScopeEntry {
        RuleScopeEntry {
            rule_id: id.to_string(),
            engine: ironlint_core::config::EngineKind::Script,
            severity: sev,
            description: desc.to_string(),
            scope_match: sm,
        }
    }

    #[test]
    fn json_filters_to_in_scope_rules_and_omits_when_skipped() {
        let outcomes = ScopeOutcomes {
            skip: None,
            rules: vec![
                entry(
                    "a",
                    Severity::Error,
                    "ad",
                    ScopeMatch::Match { glob: "*.md".into() },
                ),
                entry(
                    "b",
                    Severity::Warning,
                    "bd",
                    ScopeMatch::NoMatch { scopes: vec!["*.ts".into()] },
                ),
            ],
        };
        let out = GuideOutput {
            skip: None,
            rules: outcomes
                .rules
                .iter()
                .filter(|r| matches!(r.scope_match, ScopeMatch::Match { .. }))
                .map(|r| GuideEntry {
                    rule_id: r.rule_id.clone(),
                    severity: severity_str(r.severity).to_string(),
                    description: r.description.clone(),
                })
                .collect(),
        };
        let v = serde_json::to_value(&out).unwrap();
        assert!(v["skip"].is_null());
        assert_eq!(v["rules"].as_array().unwrap().len(), 1);
        assert_eq!(v["rules"][0]["rule_id"], "a");
        assert_eq!(v["rules"][0]["severity"], "error");
    }

    #[test]
    fn skipped_file_produces_empty_rules_in_json() {
        let outcomes = ScopeOutcomes {
            skip: Some(SkipHit { pattern: "Cargo.lock".into() }),
            rules: vec![entry(
                "a",
                Severity::Error,
                "ad",
                ScopeMatch::Match { glob: "*.md".into() },
            )],
        };
        let out = GuideOutput {
            skip: Some(SkipJson { pattern: "Cargo.lock".into() }),
            rules: if outcomes.skip.is_some() { Vec::new() } else { vec![] },
        };
        let v = serde_json::to_value(&out).unwrap();
        assert_eq!(v["skip"]["pattern"], "Cargo.lock");
        assert_eq!(v["rules"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn severity_str_renders_lowercase_words() {
        assert_eq!(severity_str(Severity::Error), "error");
        assert_eq!(severity_str(Severity::Warning), "warning");
    }
}
```

- [ ] **Step 19: Run tests, verify green**

Run: `cargo test -p ironlint-cli --test cli_e2e_guide`
Expected: PASS — all four tests green.

- [ ] **Step 20: Commit**

```bash
git add crates/ironlint-cli/src/commands/guide.rs crates/ironlint-cli/tests/cli_e2e_guide.rs
git commit -m "$(cat <<'EOF'
feat(cli): add ironlint guide subcommand (C2 phase 3)

`ironlint guide <file>` lists rules whose scope matches `<file>` as
`<rule-id> [<severity>] <description>`, sorted by id (BTreeMap key
order). Skipped files emit only a `SKIPPED <file> via <pattern>`
banner — no guidance rows.

`--format json` emits a stable `{ skip, rules: [{ rule_id, severity,
description }] }` shape; severity is the lowercase word `"error"` or
`"warning"`. Read-only — no engine runs, no LLM is constructed, no
telemetry is written.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 4 — `--format json` snapshot tests

The JSON shapes for both commands are public-ish surfaces (tooling consumers will parse them). Lock them in with `insta` so an unintentional re-key surfaces as a snapshot diff.

### Task 4.1: Add insta snapshot tests for both commands

**Files:**
- Modify: `crates/ironlint-cli/tests/cli_e2e_explain.rs`
- Modify: `crates/ironlint-cli/tests/cli_e2e_guide.rs`

- [ ] **Step 21: Add insta JSON snapshot to `cli_e2e_explain.rs`**

Append:

```rust
#[test]
fn explain_format_json_shape_is_stable() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let file = dir.path().join("docs/intro.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "# hi\n").unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "explain",
            "--config",
            cfg.to_str().unwrap(),
            "--format",
            "json",
            file.to_str().unwrap(),
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    // Parse-then-re-serialize so the snapshot is canonicalized; raw stdout
    // contains tempdir paths we don't want in the snapshot.
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    insta::assert_json_snapshot!("explain_md_file_json", v);
}

#[test]
fn explain_format_json_skipped_file_shape_is_stable() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let lock = dir.path().join("Cargo.lock");
    std::fs::write(&lock, "# generated\n").unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "explain",
            "--config",
            cfg.to_str().unwrap(),
            "--format",
            "json",
            lock.to_str().unwrap(),
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    insta::assert_json_snapshot!("explain_lockfile_json", v);
}
```

- [ ] **Step 22: Add insta JSON snapshot to `cli_e2e_guide.rs`**

Append:

```rust
#[test]
fn guide_format_json_shape_is_stable() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let file = dir.path().join("docs/intro.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "# hi\n").unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "guide",
            "--config",
            cfg.to_str().unwrap(),
            "--format",
            "json",
            file.to_str().unwrap(),
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    insta::assert_json_snapshot!("guide_md_file_json", v);
}

#[test]
fn guide_format_json_skipped_file_shape_is_stable() {
    let dir = tempdir().unwrap();
    let cfg = write_trusted(dir.path(), THREE_RULE_BODY);
    let lock = dir.path().join("Cargo.lock");
    std::fs::write(&lock, "# generated\n").unwrap();
    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "guide",
            "--config",
            cfg.to_str().unwrap(),
            "--format",
            "json",
            lock.to_str().unwrap(),
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    insta::assert_json_snapshot!("guide_lockfile_json", v);
}
```

- [ ] **Step 23: Run snapshot tests and accept the snapshots**

Run: `cargo test -p ironlint-cli --test cli_e2e_explain explain_format_json`
Expected: FAIL on first run (no snapshot yet). Run `cargo insta review` and accept the four new snapshots:
- `crates/ironlint-cli/tests/snapshots/cli_e2e_explain__explain_md_file_json.snap`
- `crates/ironlint-cli/tests/snapshots/cli_e2e_explain__explain_lockfile_json.snap`
- `crates/ironlint-cli/tests/snapshots/cli_e2e_guide__guide_md_file_json.snap`
- `crates/ironlint-cli/tests/snapshots/cli_e2e_guide__guide_lockfile_json.snap`

Re-run: `cargo test -p ironlint-cli --test cli_e2e_explain && cargo test -p ironlint-cli --test cli_e2e_guide`
Expected: PASS.

The expected explain snapshot for the markdown file looks like:

```json
{
  "rules": [
    {
      "matched_glob": "*.md",
      "rule_id": "any-md",
      "scopes": null,
      "status": "match"
    },
    {
      "matched_glob": null,
      "rule_id": "rs-rule",
      "scopes": ["**/*.rs"],
      "status": "skip"
    },
    {
      "matched_glob": null,
      "rule_id": "ts-rule",
      "scopes": ["**/*.ts"],
      "status": "skip"
    }
  ],
  "skip": null
}
```

The expected guide snapshot for the markdown file looks like:

```json
{
  "rules": [
    {
      "description": "docs lint",
      "rule_id": "any-md",
      "severity": "warning"
    }
  ],
  "skip": null
}
```

If the tempdir-relative path leaks into the JSON (it shouldn't — neither command serializes the file path into its output), use `insta`'s redaction feature to scrub it. The shapes above contain no path strings, so this is unlikely.

- [ ] **Step 24: Commit snapshots and tests**

```bash
git add crates/ironlint-cli/tests/cli_e2e_explain.rs crates/ironlint-cli/tests/cli_e2e_guide.rs crates/ironlint-cli/tests/snapshots/
git commit -m "$(cat <<'EOF'
test(cli): lock explain/guide JSON shapes via insta (C2 phase 4)

Snapshot the `--format json` output for `ironlint explain` and
`ironlint guide` in the happy path and the skipped-file path. Locks
the four shapes (`{ skip, rules: [...] }` for both, with explain's
per-rule `status`/`matched_glob`/`scopes` discriminator and guide's
`severity`/`description`) so an unintentional re-key surfaces as a
snapshot diff under `cargo insta review`.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 5 — README + verification

### Task 5.1: README update

**Files:**
- Modify: `README.md`

- [ ] **Step 25: Add the two commands to the README's Commands listing**

Find the existing Commands section (or wherever subcommands are listed) and add:

```markdown
### Inspection

- `ironlint explain <file>` — show which rules are in scope for a file and which scope glob matched (or which skip pattern suppressed it). Read-only.
- `ironlint guide <file>` — list rules whose scope matches the file with their severity and description. Read-only.

Both honor `--config <path>` (default `.ironlint.yml`) and `--format human|json` (default `human`). Exit 0 on success, 1 on config error. They never run engines, call LLMs, or write telemetry — they only read config and report scope/skip resolution.
```

If the README has no Commands section yet, add the snippet under a new `## Commands` heading. Keep the addition under 12 lines so it stays scannable.

- [ ] **Step 26: Commit README**

```bash
git add README.md
git commit -m "$(cat <<'EOF'
docs(readme): document ironlint explain and ironlint guide (C2 phase 5)

Add a short Inspection block listing the two new read-only
subcommands, their flag surface (`--config`, `--format`), and their
exit-code contract (0 success, 1 config error — never 2).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 5.2: fmt + clippy + workspace tests

- [ ] **Step 27: Run formatters and lints**

Run, in order:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

Each must be clean. Cognitive complexity is capped at 15 per the workspace `clippy.toml`; if any function in `commands/explain.rs`, `commands/guide.rs`, or `runner.rs::scope_outcomes` trips the lint, decompose rather than `#[allow]`.

### Task 5.3: Per-file coverage gate

- [ ] **Step 28: Run the coverage gate**

```bash
bash scripts/ci-coverage.sh
```

Verify every modified or created file under `crates/*/src/` is at ≥80% region coverage. Likely problem files and the targeted backfill if needed:

- `crates/ironlint-cli/src/commands/explain.rs` — `emit_human`, `emit_json`, the `run` dispatch, and the unit-test helpers should already cover both branches of `ScopeMatch` and the `Some/None` skip arm. If still under 80%, add a unit test exercising `emit_human` with a `ScopeOutcomes { skip: None, rules: [Match, NoMatch] }` and another with `skip: Some(_)`.

- `crates/ironlint-cli/src/commands/guide.rs` — same structure as `explain.rs`. The `if outcomes.skip.is_some()` branch in `emit_json` and the `if matches!(.., Match { .. })` filter in `emit_human` both need at least one true and one false invocation; the existing unit tests cover both. Add a focused test for the `severity_str` helper if the macro-region accounting marks it un-hit (it already has a dedicated test).

- `crates/ironlint-core/src/runner.rs` — only the **new** functions need to clear the gate, not the whole file (which is pre-existing). The new `scope_outcomes`, `first_matching_scope_glob`, and `first_matching_skip_glob` should be exercised by the three integration tests in `tests/runner_scope_outcomes.rs`. If `first_matching_skip_glob`'s `else if let Some(prefix) = raw.strip_suffix("/**")` branch is uncovered, add a `dist/`-style test fixture to the existing integration test file: a file at `dist/foo.js` should produce a `SkipHit { pattern: "dist/**" }`.

- [ ] **Step 29: Mutation-test the new functions (optional, local-only)**

Per the repo rules in `CLAUDE.md`, `cargo mutants` is a local investigative tool, not a CI gate. After Phase 1 lands, run:

```bash
cargo mutants --file crates/ironlint-core/src/runner.rs --file crates/ironlint-cli/src/commands/explain.rs --file crates/ironlint-cli/src/commands/guide.rs
```

Surviving mutants in the new code paths (e.g. flipping `Match` to `NoMatch`, dropping the `SKIPPED` banner, swapping `match` for `skip` in the format string) are coverage gaps. Add the tests that kill them. Skip this step if you don't have `cargo-mutants` installed; it's not blocking.

### Task 5.4: Plan archive

- [ ] **Step 30: Move plan to archive on merge**

When the PR for this plan merges, run:

```bash
git mv plans/2026-05-13-ironlint-c2-explain-guide.md plans/archive/2026-05-13-ironlint-c2-explain-guide.md
```

and add an entry to the `plans/README.md` Archive table:

```markdown
- [`2026-05-13-ironlint-c2-explain-guide`](archive/2026-05-13-ironlint-c2-explain-guide.md) — `ironlint explain <file>` and `ironlint guide <file>` read-only inspection subcommands; shared `scope_outcomes` helper in `ironlint-core`; JSON snapshots locked with insta.
```

Also strike any in-flight reference to C2 from the active section if present.

---

## Acceptance criteria checklist

Mapped to the spec's §C2 acceptance bullets:

- [ ] **"Both subcommands work without a `--config` flag (default `.ironlint.yml`)."** → Task 2.2 / 3.2 set `default_value = ".ironlint.yml"` on the clap `--config` arg for both variants. Tests pass `--config` explicitly to use a tempdir; the default applies whenever the flag is omitted.
- [ ] **"Output is greppable: one rule per line."** → Task 2.3 (`emit_human` prints one `MATCH ...` or `skip ...` line per rule via `println!`); Task 3.2 (`emit_human` prints one `<rule-id> [<severity>] <description>` line per in-scope rule). Verified by the integration tests asserting `stdout.contains(...)` on individual lines.
- [ ] **"`--format json` available for both."** → Task 2.3 / Task 3.2 implement `OutputFormat::Json` arms; Task 4.1 locks the shapes via insta snapshots.
- [ ] **"Both read-only, no execution."** (from the proposed-design body) → guaranteed by construction: neither command constructs an `LlmClient`, neither calls `engine.check`, neither appends to `.ironlint/log.jsonl`. The risk-section exit-code paragraph and the no-telemetry paragraph state this explicitly. Task 1.1 / 2.1 / 3.1 tests assert the file is exit-0 on every successful invocation.
- [ ] **A2 dependency surfaced.** → Task 1.1 includes `scope_outcomes_records_skip_hit_for_built_in_lockfile`; Task 2.1 includes `explain_emits_skipped_banner_for_lockfile`; Task 3.1 includes `guide_skipped_file_emits_banner_and_no_rules`. The shared helper reads `~/.ironlint-ignore` and `config.skip` exactly like `IronLintEngine::load_with` does, so user-supplied skip patterns are surfaced too.
- [ ] **Exit-code contract preserved.** → Risk-section "exit-code-contract impact" paragraph spells out 0/1 only — never 2. Tests assert `code(0)` for happy paths and `code(1)` for missing-config paths.
- [ ] **Documentation.** → Task 5.1 adds a README blurb naming both subcommands.
- [ ] **At least one unit test per non-trivial helper in `explain.rs` and `guide.rs`.** → `commands/explain.rs::tests::json_shape_distinguishes_match_and_skip_status_strings` covers the JSON encoder; `commands/guide.rs::tests::json_filters_to_in_scope_rules_and_omits_when_skipped`, `skipped_file_produces_empty_rules_in_json`, and `severity_str_renders_lowercase_words` cover the JSON encoder, the skipped-path branch, and the severity stringifier respectively.
- [ ] **Per-file ≥80% region coverage.** → Task 5.3 runs `bash scripts/ci-coverage.sh` and lists the targeted backfill if any file is short.

---

## Self-review

- [x] Spec §C2 acceptance criteria are all mapped to specific tasks (above).
- [x] No placeholders. Every task body shows the test code, the implementation code, and the exact `cargo test` invocation with the expected pass/fail signal.
- [x] All types referenced in later tasks (`ScopeOutcomes`, `ScopeMatch`, `RuleScopeEntry`, `SkipHit`, `ExplainOutput`, `ExplainEntry`, `GuideOutput`, `GuideEntry`) are defined in earlier tasks with consistent names.
- [x] The shared helper has a single home (`ironlint_core::runner::scope_outcomes`) with a written justification — duplication-avoidance, reuse by future LSP/`show-resolved-config` callers, and keeping `ScopeMatcher`/`SkipMatcher` private to core.
- [x] Output formats are locked literally: `MATCH <id> via <glob>`, `skip <id> scope=<globs>`, `SKIPPED <file> via <pattern>` for `explain`; `<id> [<severity>] <description>` for `guide`.
- [x] Casing rationale (`MATCH` uppercase, `skip` lowercase) recorded in the explain module doc comment so `grep '^MATCH'` is the documented "show me what fires" recipe.
- [x] Greppable contract documented: one rule per line on stdout; errors to stderr; verified in the integration tests via `stdout.contains(...)` per-line assertions.
- [x] Exit-code contract preserved (0/1 only) and called out in the risk section so a future plan doesn't reuse exit-2 for advisory output.
- [x] No verdict / telemetry / config-schema mutations.
- [x] Cognitive-complexity cap at 15 acknowledged; decomposition recommended over `#[allow]`.
- [x] Coverage gate (≥80% region) acknowledged with a per-file backfill plan.
- [x] Phase order matches TDD: every implementation task is preceded by a failing test in the same phase.
- [x] Phase 2's stub-then-overwrite pattern for `commands/guide.rs` is explicitly called out so a reviewer doesn't see it as cruft.
