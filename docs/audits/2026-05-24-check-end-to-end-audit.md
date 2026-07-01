# IronLint `check` end-to-end audit — 2026-05-24

**Date:** 2026-05-24
**Auditors:** Claude (Opus 4.7, 1M context) + Codex (separate session). Cross-checked adversarially before consolidation.
**Scope:** `ironlint check` from CLI to verdict. Specifically: `commands::check::run` → `runner::IronLintEngine::{load,check_inner,check_session,build_deferred_envelope,evaluate_one_rule}` → `diff::parser`, `config::extends`, `trust`, `baseline`, `disable`, `telemetry`, and the adapter hook/plugin paths that synthesize diffs (`adapters/claude-code/hooks/{hook,synthesize_diff}.sh`, `adapters/opencode/src/index.ts`).

## How to use this document

Each finding is a checkbox. Sub-tasks under each finding break the work into the *correct* fix (not the cheap one), the regression test that pins it, and any docs/wire-format follow-ups. Tick boxes as you land them. Findings reference file:line at the time of the audit; verify the line numbers still resolve before opening the patch.

Severity:
- **P0** — silent or default-path correctness failure. Ship-blockers.
- **P1** — observable correctness gap or weakened defense. Decide before the 0.3 verdict-shape freeze.
- **P2** — robustness / contract clarification.
- **P3** — latent, style, or perf.

## Findings table

| ID | Severity | Title | Wire impact | Order |
|----|----------|-------|-------------|-------|
| [A1](#a1) | P0 | Baseline silently makes `output: passthrough` rules a permanent per-file disable | baseline JSON | 1 |
| [A2](#a2) | P0 | Diff parser drops `\t<timestamp>` from `+++ b/` headers → entire diff mode no-ops for non-git patches | none | 2 |
| [B1](#b1) | P1 | Diff-mode file reads resolve against process CWD, not `config_dir` | none | 3 |
| [B2](#b2) | P1 | `check_session` scope matching skips relativization → absolute paths from adapters never match pathed scopes | none | 4 |
| [B3](#b3) | P1 | `claude-code-subagent` + `engine: session` has no working stop-time path | deferred env | 5 |
| [B4](#b4) | P1 | Deferred-mode CLI branch drops every deterministic warning from stdout | deferred env | 6 |
| [B5](#b5) | P1 | Deferred envelope skips `expand_context` → prompt drift between direct-API and subagent routes | deferred env | 7 |
| [B6](#b6) | P1 | Linux `unshare(CLONE_NEWNET)` mutates the parent process → per-rule `network: true` opt-in is broken | none | 8 |
| [B7](#b7) | P1 | Engine-internal errors collapse onto the policy-block exit code (2) → adapters can't distinguish | CLI exit codes | 9 |
| [C1](#c1) | P2 | Trust fingerprint depends on `serde_yaml` emitter heuristics → fragile across dep bumps | trust block | 10 |
| [C2](#c2) | P2 | `build_single_file_diff` doesn't verify the recovered `--- a/<path>` matches the target | none | 11 |
| [C3](#c3) | P2 | Pure file-deletion diffs (`+++ /dev/null`) yield exit 1 instead of a clean pass/no-op | none | 12 |
| [C4](#c4) | P2 | `relativize` falls back to canonical absolute path → silently runs rules against files outside `config_dir` | none | 13 |
| [C5](#c5) | P2 | Sentinel-tag neutralization is ASCII-literal → bypassable by zero-width / Unicode lookalikes | LLM prompt | 14 |
| [C6](#c6) | P2 | Schema-version bump policy ambiguous: additive `deferred_rules` field bumped `schema_version` 2 → 3 | verdict schema | 15 |
| [D1](#d1) | P3 | `ChangedFile.added_lines` is dead code; the runner has no "new violations only" filter for diff mode | parser ABI | 16 |
| [D2](#d2) | P3 | Diff parser drops content lines literally starting with `+++` and fails to advance new-line counter | parser ABI | 17 |
| [D3](#d3) | P3 | `SessionState::save` doesn't fsync before rename | none | 18 |
| [D4](#d4) | P3 | `ScopeMatcher` rebuilt per rule per file in dispatch loops | none | 19 |
| [D5](#d5) | P3 | CLI loads the engine twice on every `check` invocation (probe + real) | none | 20 |
| [D6](#d6) | P3 | Multi-parent `extends:` precedence undocumented for `llm:` and `rules:` collisions | none | 21 |

---

## P0 — ships broken or silently wrong

### A1 — Baseline silently makes `output: passthrough` rules a permanent per-file disable {#a1}

**Files:** `crates/ironlint-core/src/baseline.rs:205-219`, `crates/ironlint-core/src/engine/script.rs:148-177`, `crates/ironlint-core/src/config/types.rs:112-118`.

**Evidence.** `OutputMode::Passthrough` is the default since R4 (2026-05-23) — it's now the dominant emission path for script rules. Passthrough emits one `Violation` with `line: None` and the verbatim tool output in `message`. `Baseline::checksum_matches` then short-circuits on `line: None`:

```rust
let Some(n) = line else {
    return true;          // ← any future violation with the same (rule_id, file) matches.
};
```

So once an operator runs `ironlint baseline record` to snapshot current violations, every future violation of that `(rule_id, file)` combo is silenced *forever*, regardless of message content or what changed. A file with `DEBUG_OLD` is baselined; the user edits to `DEBUG_NEW`; ironlint says "pass." The existing test `line_none_violation_baselines_without_checksum` enshrines this behavior, but it was written before passthrough became the default — the implicit contract has shifted out from under the test.

**Why this is P0.** The two new defaults — `passthrough` script output and "baseline is the standard pre-edit hygiene step" — combine to make baseline a per-file disable for the most common rule type. Operators reach for baseline expecting "snapshot current state, surface anything new"; they get a config-edit-shaped silencer instead.

**Fix (the right way):** hash a *normalized message body* when `line` is `None`, store it as `Option<String>` alongside the existing line checksum. Match when both the fingerprint key and the message-body hash agree. Normalization rules:

1. Strip trailing whitespace per line (as `line_checksum` already does for line content).
2. Drop ISO-8601-shaped timestamps from the body (`\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}` and similar) — common in linters that include "as of <time>" preambles.
3. Drop ANSI color escapes.
4. Hash with SHA-256.

This keeps the same matching semantics for line-bearing violations (key + line-checksum) and adds an analogous body-checksum dimension for file-level ones.

**Schema migration:** `BaselineOnDisk` already untag-deserializes v1 and v2. Add a v3 variant whose entries are `{key: {line_sha256: Option<String>, body_sha256: Option<String>}}`. Reader treats `body_sha256: None` as the v2 "match-on-key-only" grace period; writers always emit v3. Bump `Baseline::SCHEMA_VERSION` (currently implicit via the untagged enum — make it explicit).

**Checklist:**
- [x] Add `body_checksum: Option<String>` to baseline entries (new `BaselineOnDisk::V3` variant).
- [x] Implement `Baseline::body_checksum(message: &str)` with timestamp + ANSI stripping.
- [x] `Baseline::add_with_content` captures the body checksum when `violation.line.is_none()`.
- [x] `Baseline::contains_with_content` requires both fingerprint match *and* (body_checksum match OR body_checksum absent — grace period).
- [x] `Baseline::refresh` recomputes body checksums.
- [x] Update existing test `line_none_violation_baselines_without_checksum` to **invert** its assertion — different content of the same rule on the same file must now resurface.
- [x] Add fixture for v3 on-disk format under `crates/ironlint-core/tests/fixtures/`.
- [x] Document the new matching contract in `README.md` baseline section and `CHANGELOG.md`.

**Wire-format impact:** baseline JSON gains a field. Strictly additive on read (v2 still loads under grace period); strictly newer on write. Adapters and `record_verdict` are unaffected.

---

### A2 — Diff parser drops `\t<timestamp>` from `+++ b/` headers → entire diff mode no-ops for non-git patches {#a2}

**Files:** `crates/ironlint-core/src/diff/parser.rs:17-22`, `crates/ironlint-cli/src/commands/check.rs:284-316`.

**Evidence.** Parser:

```rust
if let Some(path) = raw.strip_prefix("+++ b/") {
    let path = path.trim_end_matches('\r');   // ← only \r is trimmed
    ...
    let pb = PathBuf::from(path);
```

POSIX `diff -u` emits `+++ b/<path>\t<timestamp>`. The tab+timestamp survives the trim and ends up in the `PathBuf`. Empirically reproduced:

```bash
$ printf -- '--- a/src/lib.rs\t2026-05-24 14:30:00 +0000\n+++ b/src/lib.rs\t2026-05-24 14:30:00 +0000\n@@ -1,1 +1,2 @@\n fn main() {}\n+// TODO: ship it\n' > t.patch
$ ironlint check --diff t.patch --config .ironlint.yml --format json
{"status":"pass","passed_checks":[],"violations":[]}    # ← zero rules ran
```

Same diff without the `\t<timestamp>` blocks correctly. `git diff` happens to omit timestamps so git-generated patches work; `diff -u`, `patch -p1` round-trips, IDE patch exports, `quilt`, and anything else following the POSIX format does not.

`build_single_file_diff` is also affected: its `format!("+++ b/{}", file.display())` lookup compares against the full `+++ b/path\ttimestamp` line via `trimmed == needle` and silently returns an empty slice.

**Why this is P0.** Production failure mode is **complete silence**. The verdict says pass, the exit code is 0, the operator believes their gate ran. The only signal is the empty `passed_checks` array buried in JSON output that adapters don't surface.

**Fix (the right way):** parse the `+++ b/` header per the unified-diff format: path runs from `b/` until the first tab or end-of-line; the optional timestamp segment is discarded. Apply the same parse in `build_single_file_diff` so its lookup compares path-against-path, not line-against-line.

**Checklist:**
- [x] In `parse_unified`, replace `trim_end_matches('\r')` with: split at first `\t` (or `\r`), take the head.
- [x] In `build_single_file_diff`, split the haystack line and the needle the same way before comparison.
- [x] Add `parse_unified_strips_tab_timestamp_from_path` test (asserts `PathBuf == "myfile.py"` from `+++ b/myfile.py\t2026-05-24 14:30:00 +0000`).
- [x] Add `slice_preserves_each_files_hunks_with_timestamps` to `commands/check.rs` tests.
- [x] Add a CLI-level repro test (`cli_check_diff_with_timestamp_blocks`) that runs the binary against a `diff -u`-style patch and asserts exit 2 + the expected `rule_id`.

**Wire-format impact:** none. Pure parser robustness fix.

---

## P1 — decide before 0.3 verdict freeze

### B1 — Diff-mode file reads resolve against process CWD, not `config_dir` {#b1}

**Files:** `crates/ironlint-core/src/runner.rs:812-825`, `crates/ironlint-cli/src/commands/check.rs:98-127`, `crates/ironlint-core/src/engine/context.rs:19-31`.

**Evidence.** `check_inner` for `CheckInput::Diff`:

```rust
let content = std::fs::read_to_string(&file).unwrap_or_default();
```

`file` is the bare relative path the CLI passed in, sourced from `+++ b/<rel-path>`. The runner never joins it against `self.config_dir`. `read_to_string` resolves against the process working directory; on failure it silently falls back to empty content.

The asymmetry: **script rules** spawn `sh -c <cmd>` with `cwd: &self.config_dir` (`engine/capability.rs::spawn_with_timeout`), so they DO see the file. **AST rules**, **disable directives**, and **semantic rules with `context: file`/`repo`** all use the in-process `content` buffer and silently degrade — AST emits an `__internal` violation, disable directives are never parsed from the file, semantic context can't load.

Verified empirically: running `ironlint check --diff /abs/path/to/t.patch --config /abs/path/to/.ironlint.yml` from `/tmp` produces `rule_id: "no-panic__internal", engine: "internal", message: "ast engine requires file content (CheckInput::File)"`. The existing CLI test `cli_check_diff_processes_every_changed_file` masks the bug with `.current_dir(root)`.

**Why this is P1, not P0.** The bug is visible — the operator sees `__internal` violations and "engine errors." But it conflates "your config is wrong" with "the agent's environment is wrong," and any CI invocation that doesn't `cd $REPO_ROOT` first will quietly skip AST rules entirely.

**Fix (the right way):** introduce a `resolve_input_path(&self, p: &Path) -> PathBuf` helper on `IronLintEngine` that:

1. If `p.is_absolute()`, returns `p` as-is.
2. Else, joins onto `self.config_dir`.

Call it in `check_inner` *before* `read_to_string(&file)`, and pass the resolved absolute path into `ctx.file` so `engine::context::expand_context` consistently sees the same path. Surface a one-line stderr warning when the read fails — silently falling through `unwrap_or_default()` is what let this hide.

**Checklist:**
- [x] Add `IronLintEngine::resolve_input_path`.
- [x] Use it in both arms of the `CheckInput` match in `check_inner`.
- [x] Replace `unwrap_or_default()` with a `match`: on `Err`, eprintln a one-line warning and pass `String::new()`.
- [x] Threading: ensure `ctx.file` carries the resolved absolute path so `engine/context.rs::expand_context` reads the right file.
- [x] New test `crates/ironlint-cli/tests/cli_check_diff_cwd.rs::ast_rule_fires_from_unrelated_cwd`: build a project at `/tmp/A`, invoke `ironlint check --diff` from `/tmp/B` with absolute paths, assert the AST rule produces a real `no-panic` violation (not `no-panic__internal`).
- [x] Equivalent test for `engine: semantic, context: file`.
- [x] Equivalent test for a `ironlint-disable:` directive on the post-edit line.

**Wire-format impact:** none.

---

### B2 — `check_session` skips relativization → absolute paths from adapters never match pathed scopes {#b2}

**Files:** `crates/ironlint-core/src/runner.rs:1272-1280`, `crates/ironlint-cli/src/commands/session.rs:80-84`, `adapters/claude-code/hooks/hook.sh:74-99`, `adapters/opencode/src/index.ts:69-78`.

**Evidence.** `check_inner` relativizes:

```rust
let match_path = relativize(&path, &self.config_dir);
```

`check_session` does not:

```rust
.filter(|e| matcher.matches(std::path::Path::new(&e.file)))
```

Adapter event payloads carry absolute paths. `session record` stores them verbatim via `file.display().to_string()`. The scope `src/auth/**` does not match `/tmp/proj/src/auth/login.ts` because pathed patterns don't get the right-anchored `**/<pattern>` fallback that bare patterns do (`config/scope.rs:19-24`).

**Why this is P1.** Session rules with pathed scopes (the conventional way to scope a rule) silently never fire in production. Bare-pattern scopes happen to work because of the `**/<bare>` fallback, masking the bug for the dev/test loop.

**Fix (the right way):** factor the relativize-and-match logic out of `check_inner` into a shared helper on `IronLintEngine`:

```rust
fn rule_matches_path(&self, rule: &Rule, file: &Path) -> bool {
    let match_path = relativize(file, &self.config_dir);
    ScopeMatcher::new(&rule.scope).expect("validated at load").matches(&match_path)
}
```

Use it from both `evaluate_one_rule` and `check_session`. Bonus: caching the `ScopeMatcher` (D4) becomes a localized refactor of this helper.

**Checklist:**
- [x] Extract `IronLintEngine::rule_matches_path(&self, rule: &Rule, file: &Path) -> bool`.
- [x] Call from `check_session`'s filter.
- [x] Call from `check_inner` (replacing the inline matcher construction).
- [x] Call from `scope_outcomes` and `render_semantic_prompts`.
- [x] New test `tests/check_session.rs::session_rule_matches_absolute_path_for_pathed_scope`: build `SessionState` with `edits[0].file = "<abs path under config_dir>/src/auth/login.ts"`, scope `src/auth/**`, assert the LLM is called.
- [x] Negative test: edits outside the scope don't trigger the LLM, matching today's behavior.

**Wire-format impact:** none.

---

### B3 — `claude-code-subagent` + `engine: session` has no working stop-time path {#b3}

**Files:** `crates/ironlint-core/src/runner.rs:1296-1298`, `crates/ironlint-core/src/llm/mod.rs:46-59`, `adapters/claude-code/hooks/hook.sh:45-69`.

**Evidence.** Per-file checks correctly defer session rules (`should_defer` covers both `Semantic` and `Session` in `runner.rs:28-30`). But `check_session` still hard-requires an `LlmClient`:

```rust
let llm = self.llm.as_deref().ok_or_else(|| {
    anyhow::anyhow!("session check requires LlmClient; build engine with .with_llm()")
})?;
```

`build_from_config` returns `Ok(None)` for `claude-code-subagent` (mod.rs:56). The Claude `stop` hook calls `ironlint check --session`, hits the error, exits non-zero, and the hook prints `ironlint: internal error during session check (exit 1)`. Every stop hook fires this. There is no escape hatch.

**Why this is P1.** Subagent mode is the marquee 0.1c surface, and session rules are the spec's flagship cross-edit primitive. The combination is documented but doesn't work.

**Fix (the right way):** generalize the deferred envelope to a session-aggregate shape. At stop-time:

1. Detect "no LLM but at least one `engine: session` rule with scope-matching edits" — symmetric to per-file detection.
2. Build a `DeferredVerdict` whose `payload.diff` is the aggregated cross-edit framing from `SessionEngine` (each edit framed by the random `session_id`), and whose `payload.evaluate` lists all session rules. `payload.file` becomes `""` (session-level, no anchor file).
3. The stop hook wraps it in `additionalContext` exactly like the per-file path.

This keeps the wire-format invariant: there's one deferred envelope shape; sessions are just a degenerate single-payload case where `file` is empty and `diff` is the aggregated framing.

**Checklist:**
- [x] Extract the per-edit framing from `SessionEngine::evaluate` into a free function `session::framed_aggregate(state: &SessionState) -> String`.
- [x] Add `IronLintEngine::check_session_with_options(&self, state, options)` returning a `CheckReport` with `deferred: Option<DeferredVerdict>` populated when no LLM is wired and a session rule is in scope.
- [x] CLI `--session` path consults `options.emit_semantic_payload` (or auto-detect on `LlmClient: None` for subagent provider) and emits the deferred envelope to stdout.
- [x] Update `hook.sh stop` to wrap the envelope in `hookSpecificOutput.additionalContext` exactly like the post-tool-use branch.
- [x] Add `adapters/claude-code/tests/hook_session_subagent.sh`: configure subagent provider + one session rule, record two edits, run `stop`, assert exit 0 and JSON envelope on stdout containing `evaluator_input` referencing both edits.
- [x] Equivalent opencode plugin test.
- [x] Document in `docs/emit-semantic-payload.md` that session rules under subagent provider use the same deferred shape.

**Wire-format impact:** deferred envelope. `payload.file` may now be `""`, `payload.diff` may carry the session-aggregate framing. Both fields existed already, so no schema bump needed — but document the empty-file case explicitly.

---

### B4 — Deferred-mode CLI branch drops every deterministic warning {#b4}

**Files:** `crates/ironlint-cli/src/commands/check.rs:79-93`, `crates/ironlint-core/src/verdict_deferred.rs:26-71`.

**Evidence.**

```rust
if let Some(d) = &report.deferred {
    if matches!(report.verdict.status, Status::Block) {
        emit(&report.verdict, format)?;
        return Ok(2);
    }
    emit_deferred(d, format)?;          // ← warnings vanish here
    return Ok(0);
}
```

When the deferred envelope is present and the deterministic verdict status is `Warn`, the CLI emits only the envelope. `DeferredVerdict` has no `violations` field. The Warn-severity script/AST violations are gone from stdout. They're still in `.ironlint/log.jsonl`, so the data isn't lost — but the operator and the hook never see them.

**Why this is P1.** The user wrote `severity: warning` deliberately because they want visibility without a block. The CLI removes the visibility. Combined with R6 (deferred-on-block surfacing), the behavior is "you see deterministic violations when they block, and only then" — exactly opposite of "warn is the lighter touch."

**Fix (the right way):** add `warnings: Vec<DeferredWarning>` to `DeferredPayload`. Each `DeferredWarning` carries the same shape as `Violation` minus the `engine`-redundant fields:

```rust
pub struct DeferredWarning {
    pub rule_id: String,
    pub engine: Engine,                  // Script | Ast (Semantic/Session never warn here — they're deferred)
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
}
```

Bump `DEFERRED_SCHEMA_VERSION` to 3. Use `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so envelopes without warnings stay byte-compatible with v2 *readers* (modulo C6).

**Checklist:**
- [x] Add `DeferredWarning` and `DeferredPayload.warnings`.
- [x] In `check_inner`, after the parallel pool collects outcomes and *before* baseline filtering, partition Warn-severity violations into the deferred-bound list when `should_defer`'d rules exist.
- [x] Bump `DEFERRED_SCHEMA_VERSION` to 3 with a doc comment.
- [x] Add `deferred_envelope_carries_deterministic_warnings` test in `runner_deferred_mode.rs`.
- [x] Update `adapters/claude-code/agents/ironlint-evaluator.md` and the interpreter skill to surface `payload.warnings` verbatim.
- [x] Snapshot test: `tests/deferred_verdict_shape.rs` updated for v3.

**Wire-format impact:** deferred envelope schema bump (additive). Together with B5 and C6, plan a single coordinated bump.

---

### B5 — Deferred envelope skips `expand_context` → prompt drift between routes {#b5}

**Files:** `crates/ironlint-core/src/runner.rs:1061-1093`, `crates/ironlint-core/src/engine/semantic.rs:13`, `crates/ironlint-core/src/engine/context.rs:9-35`.

**Evidence.**

```rust
let primary = if diff.is_empty() {
    content.to_string()
} else {
    diff.to_string()
};
let evaluator_input = crate::llm::prompt::build_evaluator_input(&rule_refs, &primary, None);
```

`expand_context` is never called. The direct-API path (`SemanticEngine::run` and `render_semantic_prompts`) honors `context: file` and `context: repo`. The deferred path forces `primary = diff` when a diff exists, ignoring the rule's declared scope.

A rule authored `context: file` therefore gets the file content on the direct API and the diff in the subagent path. Same rule, different prompt depending on routing — the exact contract the H1 wire snapshot was meant to lock down.

**Why this is P1.** Silent prompt drift undermines the spec's claim that subagent and direct-API modes are equivalent surfaces.

**Fix (the right way):** call `expand_context` per rule when building the envelope. Because rules in the same envelope may have different `context` values, group them by `ContextScope` and emit one rule-set evaluator-input per group (or one combined evaluator-input where context appears per-rule via the third argument to `build_evaluator_input`).

The cleanest shape is single-evaluator-input-per-envelope with per-rule context interpolation:

```rust
// Pseudocode
let per_rule_contexts: Vec<(rule, primary, context)> = deferred_rules
    .iter()
    .filter_map(|d| self.config_rule(&d.id).map(|r| (d.id.clone(), r)))
    .map(|(id, rule)| {
        let scope = rule.context.unwrap_or(ContextScope::Diff);
        let (primary, context) = expand_context(scope, ..., path, &self.config_dir)?;
        (id, rule, primary, context)
    })
    ...;
```

Then `build_evaluator_input` learns to take a `Vec<(rule, primary, Option<context>)>` and produce a single user-block per rule. This is the right wire shape because the subagent needs to evaluate each rule against its declared context, not against a flattened primary.

**Checklist:**
- [x] Extend `crate::llm::prompt::build_evaluator_input` to accept per-rule (primary, context) tuples.
- [x] Update `build_deferred_envelope` to thread `expand_context` per rule.
- [x] Snapshot: extend `tests/deferred_verdict_shape.rs` with a `context: file` rule under diff input — assert file content appears in `evaluator_input`.
- [x] Snapshot: `context: repo` round-trip (today emits a stub note via `expand_context`).
- [x] Document the per-rule context semantics in `docs/emit-semantic-payload.md`.

**Wire-format impact:** `payload.evaluator_input` content changes shape (still a string). Bump `DEFERRED_SCHEMA_VERSION` if the string structure is contractual; consult the interpreter skill's parsing.

---

### B6 — Linux `unshare(CLONE_NEWNET)` mutates the parent process → per-rule `network: true` opt-in is broken {#b6}

**Files:** `crates/ironlint-core/src/engine/capability.rs:61-110`.

**Evidence.** The capability sandbox documents its own footgun:

> "a successful unshare here mutates the parent process's namespaces — it's a one-shot, process-wide side effect."

The first script rule with `capabilities.network: false` (the default) calls `libc::unshare(CLONE_NEWNET)` on the parent. Rule iteration is `BTreeMap` key order. The next rule, even one explicitly setting `capabilities.network: true`, inherits the netns-isolated parent and cannot reach the network. There is no diagnostic.

An author who names a default-network-off rule `aaa-no-net` silently disables the network capability of `npm-audit`.

**Why this is P1.** The spec advertises per-rule capability opt-in; the implementation collapses to "most-restrictive-wins across the invocation." This is also a documented security feature that doesn't behave as documented.

**Fix (the right way):** spawn each script subprocess via `clone(2)` with the requested namespace flags directly on the child. The parent never `unshare`s. This requires:

1. Replace `Command::new("sh")` + `spawn()` with a `nix::sched::clone`-based spawn for Linux. The child stack can be allocated on the heap (`Box::pin([0u8; 16384])`).
2. The child's entry function execs `sh -c <cmd>`. Capability flags are local to that child only.
3. Reading stdout/stderr stays identical via `pipe2()` + the child writing to fd 1/2.
4. `wait_timeout` still works on the cloned child via `waitpid` semantics.

This is roughly 100 lines of `unsafe` (a `Vec<u8>` for the child stack + the `clone` flags), guarded by clear `// SAFETY:` comments. The complexity is contained to `capability.rs`; the public `run_with_capabilities_env` signature doesn't change.

**Alternative architectures considered and rejected:**

- *Long-lived helper child that owns all subprocess spawning*: introduces an IPC surface and a lifecycle problem (when does the helper die? what about ironlint being killed?).
- *Document the limitation and reject mixed-network configs at load*: hides the bug rather than fixing it; the spec keeps lying.

**Checklist:**
- [x] Spike: prototype `clone(2)`-per-child spawn in `engine/capability.rs` behind a `#[cfg(target_os = "linux")]` block.
- [x] Audit the `unsafe` boundary; each block carries a `// SAFETY:` comment.
- [x] Add `network_true_rule_keeps_network_after_network_false_rule_runs_first` test in `tests/capability.rs` (Linux-only).
- [x] Add a test that explicitly verifies the parent's netns is unchanged after a `network: false` rule runs (read `/proc/self/ns/net` symlink before and after).
- [x] If `cargo +nightly miri` cannot model `clone(2)`, document that this path is exempt from miri coverage and rely on integration tests.
- [x] Update `docs/security.md` to reflect that capability constraints are now genuinely per-rule.

**Wire-format impact:** none.

---

### B7 — Engine-internal errors collapse onto the policy-block exit code → adapters can't distinguish {#b7}

**Files:** `crates/ironlint-cli/src/commands/check.rs:220-225`, `crates/ironlint-core/src/runner.rs:685-719`, `crates/ironlint-core/src/verdict.rs:97-116`.

**Evidence.** Engine runtime errors are converted to `Engine::Internal` violations with `severity: Error`, regardless of the rule's configured severity. The CLI maps `Status::Block` to exit 2. The wire format distinguishes (`engine: "internal"`, `rule_id` ends in `__internal`) but adapters branch on exit code:

```rust
fn exit_code(v: &Verdict) -> i32 {
    match v.status {
        Status::Pass | Status::Warn => 0,
        Status::Block => 2,
    }
}
```

A user who writes `severity: warning` on a semantic rule and forgets `ANTHROPIC_API_KEY` gets exit 2 — the same code as a real policy violation. Both `hook.sh` and the opencode plugin block the edit.

**Why this is P1.** The exit-code contract is the only signal adapters consume; conflating "config is wrong" with "policy was violated" is a UX correctness failure.

**Fix (the right way):** reserve exit code 3 for "≥1 rule could not be evaluated due to engine-internal error." Exit code contract becomes:

| Code | Meaning |
|------|---------|
| 0 | Pass or Warn (all rules evaluated) |
| 1 | Config error (untrusted, parse failure, missing file) |
| 2 | Block (≥1 error-severity policy violation) |
| 3 | Internal engine error (≥1 rule failed to evaluate; verdict carries `__internal` violations) |

Adapters then choose fail-open or fail-closed per their threat model. The Claude hook and opencode plugin should fail-open on exit 3 with a stderr message; CI workflows typically prefer fail-closed and can map exit 3 → blocking.

**Checklist:**
- [x] In `Verdict::from_violations`, surface a distinct `Status::InternalError` variant when any violation has `engine: Internal`. Today the violation collapses into `Block` via the Error severity check.
- [x] `commands/check.rs::exit_code` returns 3 for the new status.
- [x] Update the exit-code contract in `CLAUDE.md` and `README.md`.
- [x] Update `adapters/claude-code/hooks/hook.sh`: add a case for exit 3, default to allow + stderr message, gated by an opt-in env var (`IRONLINT_FAIL_CLOSED_ON_INTERNAL=1`) for strict CI.
- [x] Update `adapters/opencode/src/index.ts` similarly.
- [x] Add `verdict_status_internal_error_when_engine_fails` runner test.
- [x] Add `cli_check_exit_3_for_missing_api_key` CLI test.
- [x] Update `docs/telemetry.md` (the `Status` enum already serializes via serde — adding `internal_error` is wire-additive).

**Wire-format impact:** `Status` enum gains an `InternalError` variant. Strict consumers that pattern-match on the existing three values will need to handle the new one — flag this in `CHANGELOG.md` and the verdict schema bump.

---

## P2 — robustness and contract clarification

### C1 — Trust fingerprint depends on `serde_yaml` emitter heuristics {#c1}

**Files:** `crates/ironlint-core/src/trust.rs:6-67`.

**Evidence.** Fingerprint is computed as `sha256(serde_yaml::to_string(sort_keys(parsed)))`. The output of `serde_yaml::to_string` is not normative — scalar style (plain vs. quoted), sequence flow (block vs. inline), and indent width are emitter heuristics that have changed across `serde_yaml` 0.8 / 0.9 / 0.10. A `cargo update` that bumps the emitter invalidates every checked-in fingerprint with no actual config change, surfacing as "config changed since last trust" across the user base.

**Fix (the right way):** canonicalize through `serde_json::Value` and hash the JSON byte form. RFC 8259 specifies the literal bytes of JSON tokens (no equivalent ambiguity for scalar/sequence style). Steps:

1. `let yaml: serde_yaml::Value = serde_yaml::from_str(input)`.
2. Drop the `trust` key.
3. Convert to `serde_json::Value` (lossless for our config shape — all values are strings, numbers, sequences, maps).
4. Recursively sort `serde_json::Map` keys.
5. Emit with `serde_json::to_string(&sorted_value)` (no pretty-print; `serde_json` is deterministic).
6. SHA-256 of the byte string.

This makes the fingerprint robust against `serde_yaml` upgrades. The conversion can fail only on YAML features we don't use (binary scalars, anchors-as-values, complex keys); if it does, surface a config-error.

**Checklist:**
- [x] Implement `canonicalize_for_fingerprint` via the JSON route.
- [x] Add `tests/trust.rs::fingerprint_stable_across_emitter_styles`: same semantic content in both block and flow YAML produces identical fingerprints.
- [x] Add `tests/trust.rs::fingerprint_rejects_unsupported_yaml_features`: anchor reference, binary scalar, complex key — each errors at fingerprint time with a clear message.
- [x] On the migration: every existing `.ironlint.yml` will need a one-time `ironlint trust` re-run. Document the migration in `CHANGELOG.md` and surface a friendlier error when a v0.1.x fingerprint doesn't match a v0.2.x recompute ("if you just upgraded ironlint, run `ironlint trust` to re-sign").
- [x] Decision pin: this change is wire-incompatible for the trust block. Schedule it alongside the verdict-schema freeze.

**Wire-format impact:** trust fingerprint value changes for every config in the field. Treat as a coordinated migration.

---

### C2 — `build_single_file_diff` doesn't verify the recovered `--- a/<path>` matches the target {#c2}

**Files:** `crates/ironlint-cli/src/commands/check.rs:284-316`.

**Evidence.** The slice walks back one line to capture the `--- a/...` header preceding `+++ b/<target>`, but never verifies that the captured header is for the same file. A multi-file diff missing the `---` line for one file would include the previous file's `--- a/<other>` header in the slice, and `parse_unified` would then think the slice contains two files (returning the first to the runner).

**Fix (the right way):** verify the recovered header matches `--- a/<target>` (modulo timestamps; reuse the splitter from A2). On mismatch, emit the slice without the foreign header — `parse_unified` already tolerates missing `---` lines.

**Checklist:**
- [x] Verify recovered `--- a/<path>` matches target before inclusion.
- [x] Use the A2 path-from-header splitter.
- [x] Test: `slice_drops_mismatched_minus_header` — a crafted diff where `--- a/src/a.rs` precedes `+++ b/src/b.rs`. The slice for `src/b.rs` must produce exactly one file when re-parsed.

**Wire-format impact:** none.

---

### C3 — Pure file-deletion diffs yield exit 1 instead of a clean pass/no-op {#c3}

**Files:** `crates/ironlint-core/src/diff/parser.rs:11-76`, `crates/ironlint-cli/src/commands/check.rs:100-104`.

**Evidence.** `parse_unified` starts a file on `+++ b/`. A deletion has `+++ /dev/null` and never registers. The CLI errors with `"no changed files in diff"` and exit 1.

**Fix (the right way):** teach the parser to recognize three operations explicitly — `Added` (`--- /dev/null` + `+++ b/<path>`), `Modified` (`--- a/<path>` + `+++ b/<path>`), `Deleted` (`--- a/<path>` + `+++ /dev/null`). `ChangedFile` gains an `op: ChangeOp` field. The runner evaluates only `Added` and `Modified` against rules; `Deleted` is logged to telemetry but produces no policy work (no file to check). The CLI no longer errors on diffs that contain only deletions.

**Checklist:**
- [x] Add `enum ChangeOp { Added, Modified, Deleted }` to `diff::parser`.
- [x] `ChangedFile` gains `pub op: ChangeOp`.
- [x] Parser tracks both `---` and `+++` paths to determine op.
- [x] Runner skips `Deleted` rules (no `read_to_string`).
- [x] CLI's "no changed files" check counts only non-`Deleted` entries.
- [x] Test: `parse_unified_recognizes_deletion`, `parse_unified_recognizes_addition`, `parse_unified_recognizes_modification`.
- [x] CLI test: `cli_check_diff_pure_deletion_passes_clean` (exit 0, no rules ran, telemetry records the deletion).
- [x] Future-prep: a "no deleting X" rule type wants to know about deletions. Defer the rule-side support to a separate ticket but lay the foundation now by recording the op in the deferred envelope payload.

**Wire-format impact:** parser ABI gains a field on `ChangedFile`. Downstream consumers (none in 0.1) get the new variant via `#[non_exhaustive]` on `ChangeOp`.

---

### C4 — `relativize` falls back to canonical absolute path → external files silently get rules run against them {#c4}

**Files:** `crates/ironlint-core/src/runner.rs:239-246`.

**Evidence.** When the input file's canonical path is outside `config_dir`, `relativize` returns the canonical absolute path. The bare-pattern fallback in `ScopeMatcher` makes `**/*.py` match `/etc/passwd.py`. A wrapper script that constructs `--file` arguments from untrusted input can run policy against arbitrary host files.

**Fix (the right way):** make this an explicit policy decision rather than emergent behavior. Add `IronLintEngine::resolve_input_path` (introduced in B1) that errors when the resolved path is outside `config_dir`, gated by a `--allow-external-paths` CLI flag for the rare operator who legitimately wants this. Default: reject.

**Checklist:**
- [x] `IronLintEngine::resolve_input_path` returns `Err` when the canonical path is outside `config_dir`.
- [x] Surface the error as `Engine::Internal` (post-B7 → `Status::InternalError`) with a clear message.
- [x] Add CLI flag `--allow-external-paths` that overrides the gate.
- [x] Test: `external_path_rejected_by_default`, `external_path_allowed_with_flag`.

**Wire-format impact:** none for verdict; CLI flag addition is additive.

---

### C5 — Sentinel-tag neutralization is ASCII-literal → bypassable by zero-width / Unicode lookalikes {#c5}

**Files:** `crates/ironlint-core/src/llm/prompt.rs:200-251`.

**Evidence.** `replace_ci_ascii` only matches literal ASCII. `<TRUSTED_РOLICY>` (Cyrillic Р, U+0420) and `<TRUSTED_POLICY\u{200B}>` (zero-width space inside) survive `neutralize` but read as the tag to most LLMs.

**Fix (the right way):** replace the fixed sentinel with a per-call random delimiter. At each `build_prompt_split` call:

1. Generate a 128-bit hex token (`uuid::Uuid::new_v4().simple()` or a `rand` call).
2. Sentinel becomes `<TP-{token}>` / `</TP-{token}>` and `<UE-{token}>` / `</UE-{token}>`.
3. Pre-scan user-controlled content; if the token appears verbatim (vanishingly unlikely but defensible), regenerate.
4. Adapter contract: the subagent / direct-API consumer reads the delimiters from the rendered prompt (they're in the wire format anyway).

An attacker can't predict the token, so they can't forge boundary tags. The existing ASCII-CI neutralizer becomes obsolete.

**Checklist:**
- [x] Add `rand` dep or use `chrono::Utc::now().timestamp_nanos()` + a counter for testability.
- [x] Per-call token generation in `build_prompt_split`.
- [x] Strip the existing ASCII-CI neutralization (no longer load-bearing).
- [x] Test: `prompt_token_changes_per_call`, `prompt_resists_literal_sentinel_in_user_content` (with both ASCII and Unicode variants).
- [x] Verify subagent contract: the rendered prompt carries enough structure for the LLM to identify the policy vs. evidence boundary.

**Wire-format impact:** `evaluator_input` content shape changes (still a string). Consider as part of the B5 coordinated bump.

---

### C6 — Schema-version bump policy ambiguous {#c6}

**Files:** `crates/ironlint-core/src/verdict.rs:11-17`, `crates/ironlint-core/src/verdict_deferred.rs:17-24`.

**Evidence.** `SCHEMA_VERSION` jumped 2 → 3 for the R6 additive `deferred_rules` field. The field uses `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so verdicts without it are byte-compatible with v2 readers — but the version number bumped anyway. A strict consumer doing `assert v.schema_version == 2` rejects every new verdict despite the additive shape.

**Fix (the right way):** pin a policy in `verdict.rs` and document it:

> `SCHEMA_VERSION` bumps **only** on:
> - field removals or type changes,
> - enum variant removals,
> - semantic re-interpretations of existing fields.
> Additive changes (new optional field, new enum variant marked `#[non_exhaustive]`) do **not** bump.

Then revert `SCHEMA_VERSION` to 2 (R6 was additive). Add a parallel `MIN_REQUIRED_SCHEMA_VERSION` constant the consumer can read as "you support reading anything ≥ this." Or adopt semantic versioning (major.minor) where minor bumps signal additive.

**Checklist:**
- [x] Pick a policy: strict additive-no-bump OR `major.minor` versioning. Document in `verdict.rs` and `docs/telemetry.md`.
- [x] Apply retroactively: revert `SCHEMA_VERSION` to 2 if strict additive, or to "3.0" if semver.
- [x] Update verdict snapshot tests.
- [x] Same review for `DEFERRED_SCHEMA_VERSION` (and re-issue B4/B5 bumps under the new policy).
- [x] Add `version_only_bumps_on_breaking_change` invariant test.

**Wire-format impact:** depends on the policy chosen. Pin before 0.3.

---

## P3 — latent, style, perf

### D1 — `ChangedFile.added_lines` is dead code, no "new violations only" filter for diff mode {#d1}

**Files:** `crates/ironlint-core/src/diff/parser.rs:7,42-65`, no consumer.

**Evidence.** `added_lines` is computed but read nowhere. The runner evaluates AST/script rules against the full post-edit file; an agent fixing bug A in a file with pre-existing bug B sees the gate block on bug B.

**Decision point (the right way):** this is a design call. Two coherent answers:

- **A. Diff mode = "evaluate the whole post-edit file."** Drop `added_lines` entirely (dead code is debt). Current behavior; document as the contract.
- **B. Diff mode = "new violations only."** Consume `added_lines`. Line-bearing violations (AST, parsed-script) filter to added-line numbers; file-level violations (passthrough script, semantic with `context: diff`) compute pre/post diff of the rule output and only surface deltas.

Option B is the operator-friendly choice but is a substantial implementation lift (semantic pre/post comparison needs a clean way to re-run the rule against pre-content). Option A is honest.

**Checklist:**
- [x] Decide which contract ships in 0.2. Lean toward B for the agent-gate use case.
- [x] If A: remove `ChangedFile.added_lines` and its computation; drop one regression test.
- [x] If B:
  - [ ] Plumb `added_lines` through to `evaluate_one_rule`.
  - [ ] AST + parsed-script: filter violations to `added_lines.contains(line)`.
  - [ ] Semantic: introduce pre-content re-run (read the pre-edit content via `git show HEAD:<path>` or from the diff hunks themselves). Defer for 0.3 if too large.
  - [ ] Passthrough script: re-run against pre-edit content, diff stdouts, surface only new lines.
- [x] Pin the decision in `docs/telemetry.md` and `AGENTS.md`.

**Wire-format impact:** depends on decision.

---

### D2 — Diff parser drops content lines literally starting with `+++` {#d2}

**Files:** `crates/ironlint-core/src/diff/parser.rs:60-65`.

**Evidence.** Lines that strip-prefix `+` but `starts_with("+++")` are dropped from `added_lines` *and* do not advance `new_line_no`. A markdown HR (`+++` separator) or TOML front matter (`+++` delimiter) in a diff causes subsequent added lines to record under wrong line numbers.

Latent today because `added_lines` is unused (D1). Becomes a real bug the moment D1 is wired to consume the field.

**Fix (the right way):** restrict the skip to lines matching `"+++ b/"` exactly (the actual file header). A bare `+++ content` is a real added line.

**Checklist:**
- [x] Change the `starts_with("+++")` check to `starts_with("+++ b/")`.
- [x] Add `parse_unified_does_not_drift_line_numbers_after_plus_plus_content` test.
- [x] Coordinate with D1 — fix this before consuming `added_lines`.

**Wire-format impact:** none in 0.1; depends on D1 choice for 0.2+.

---

### D3 — `SessionState::save` doesn't fsync before rename {#d3}

**Files:** `crates/ironlint-core/src/session_state.rs:55-75`.

**Evidence.** Unlike `Baseline::save` (P2-5), `SessionState::save` writes to temp + rename without `sync_all`. A crash between rename and durable flush leaves stale data.

**Fix (the right way):**

```rust
let mut f = std::fs::File::create(&temp)?;
f.write_all(json.as_bytes())?;
f.sync_all()?;
drop(f);
std::fs::rename(&temp, path)?;
```

**Checklist:**
- [x] Apply the fsync + rename pattern.
- [x] Mirror the test `atomic_save_keeps_temp_file_in_parent_dir` from `baseline.rs` for sessions.

**Wire-format impact:** none.

---

### D4 — `ScopeMatcher` rebuilt per rule per file in dispatch loops {#d4}

**Files:** `crates/ironlint-core/src/runner.rs:547-549, 900-902, 1117-1119, 1272-1273`.

**Evidence.** Each call to `evaluate_one_rule`, `check_session`, `scope_outcomes`, and `render_semantic_prompts` constructs a fresh `ScopeMatcher` via `ScopeMatcher::new(&rule.scope).expect("validated at load")`. For a 50-rule config over 5,000 files (baseline record), that's 250,000 `GlobSet` builds.

**Fix (the right way):** memoize at load time. `IronLintEngine` stores `scope_matchers: BTreeMap<String, ScopeMatcher>`. The B2 helper `rule_matches_path` reads from this map.

**Checklist:**
- [x] Add `scope_matchers` to `IronLintEngine`, populated in `load_with` after the validation pass.
- [x] Replace per-call construction with map lookups.
- [x] Verify `cargo bench` (if any) or hand-rolled timing on a real repo demonstrates the win.

**Wire-format impact:** none.

---

### D5 — CLI loads the engine twice on every `check` invocation {#d5}

**Files:** `crates/ironlint-cli/src/commands/check.rs:29-51`.

**Evidence.** First load (probe) for `--rule` validation; second load with options. Repeats trust verify + extends DFS + YAML parse.

**Fix (the right way):** expose a `IronLintEngine::config_rule_ids` accessor that doesn't require a fully constructed engine — or thread `--rule` validation into `with_options` so the load runs once and the validation happens post-load.

The cleanest path: keep one load. After `IronLintEngine::load`, call `engine.config_rule_ids()` (already exists) for validation; if unknown, exit 1 *before* the dispatch.

**Checklist:**
- [x] Single load. Validate rules after load against `engine.config_rule_ids()`.
- [x] Re-thread `CheckOptions.rules` through a setter on the constructed engine (or build with options first, validate after).
- [x] Remove the probe.

**Wire-format impact:** none.

---

### D6 — Multi-parent `extends:` precedence undocumented {#d6}

**Files:** `crates/ironlint-core/src/config/extends.rs:54-58`.

**Evidence.** If a child extends `[A.yml, B.yml]` and both define `llm:`, A wins (first listed). For `rules:`, both fill in any not already claimed, and "filled in" happens A-first. No test covers conflicting `llm:` blocks from two parents.

**Fix (the right way):** document the precedence in `docs/extends.md` (or `README.md` if no such doc exists) and pin it with tests.

**Checklist:**
- [x] Add `extends_first_parent_llm_wins_on_conflict` test.
- [x] Add `extends_first_parent_rule_wins_on_conflict` test.
- [x] Document the precedence rule.
- [x] Decision: keep first-parent-wins (current) or move to last-parent-wins (more conventional in include-style systems). Pin the choice with a CHANGELOG entry.

**Wire-format impact:** none, but a semantics change if the team prefers last-wins.

---

## Out of scope / verified not bugs

These earned a closer look but checked out. Recording them so the next audit doesn't redo the work.

- **TOCTOU between trust verify and parse.** `extends::resolve_inner` reads the file once and passes the same buffer to both `verify` and `parse_str`. Pinned by `p2_3_load_rejects_when_body_diverges_from_trust_fingerprint`.
- **Telemetry append concurrency.** Owner-only mode (0o600), single-`write_all`, advisory `flock` on Unix. Inter-process serialization works because distinct open file descriptions compete on `flock(LOCK_EX)`.
- **Baseline `save` atomicity.** Temp + fsync + rename. Pinned by `save_replaces_corrupt_target_atomically`.
- **`+++ b/` path traversal / absolute / empty rejection.** All three rejected at parse time with explicit errors; tests in place.
- **Disable-directive `/` handling for namespaced rule IDs.** P2-4 fix is sound; `python/no-print` round-trips intact.
- **macOS capability sandbox.** Correctly degraded to advisory. R7 moved the warning to `doctor` so routine checks stay quiet.
- **Session state edits truncation.** `MAX_EDITS = 1000` with drain-from-front works as designed.

---

## Suggested execution order

The findings are ordered roughly by **blast radius first, dependency second**. The intent is that each finding can be landed independently, but later items in some chains benefit from earlier infrastructure:

1. **A1 (baseline)** and **A2 (timestamps)** in parallel — independent, both ship-blocking, neither blocks anything else.
2. **B1 (CWD)** lays the `resolve_input_path` helper that **C4 (external paths)** reuses.
3. **B2 (session scope)** is best landed after extracting the `rule_matches_path` helper, which **D4 (ScopeMatcher caching)** then memoizes.
4. **B3 (subagent session stop)** depends on a session-deferred wire decision — coordinate with **B4 (warnings in envelope)**, **B5 (context in envelope)**, **C5 (sentinel)**, and **C6 (version policy)** in a single deferred-envelope v3 push.
5. **B6 (unshare leakage)** is a self-contained Linux work item with no wire impact.
6. **B7 (exit code 3)** is small but adapter-touching — pair with hook/plugin updates.
7. **C1 (fingerprint stability)** is a deliberate migration; ship behind a CHANGELOG note plus a re-trust nudge.
8. **C2, C3, D2** are correctness rounds the parser owes; group them.
9. **D1 (added_lines decision)**, **D6 (extends precedence)** — design pins; resolve before the 0.3 freeze.
10. **D3, D5** — small standalone wins.

## Wire-format coordination

Three changes touch contracts the field already consumes. Plan them together:

1. **`Status::InternalError` variant** (B7) → existing `Status` consumers see a new variant.
2. **Deferred envelope v3** (B4 + B5 + C5) → `DeferredPayload` gains `warnings`, evaluator-input shape changes, sentinel becomes random per-call.
3. **Verdict schema-version policy** (C6) → decision drives whether B7 forces a verdict schema bump or stays additive.
4. **Trust fingerprint migration** (C1) → every checked-in `.ironlint.yml` re-signs.

Ship these in one coordinated 0.2 release with a single CHANGELOG migration section, not piecemeal.
