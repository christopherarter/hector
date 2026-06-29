# `hector watch` — Live TUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only `hector watch` TUI that tails `.hector/log.jsonl` and shows a live **Stream** (newest-first run feed) and an **Explorer** (per-check health, ranked by blocks), built entirely on today's telemetry — no schema change.

**Architecture:** Pure aggregation + formatting live in a new `hector-core::watch` module (fully unit-tested). The CLI command `commands/watch.rs` holds pure view logic (`ViewState`, `handle_key`, `stream_lines`, `explorer_lines`) tested directly, a `ui()` draw function verified with ratatui's `TestBackend`, and a thin impure event loop. Each tick re-reads the log via the existing `telemetry::read_all`, recomputes the summary, and redraws.

**Tech Stack:** Rust (workspace: `hector-core` lib + `hector-cli` bin `hector`), `ratatui` 0.29 (TUI + re-exported `crossterm`), `chrono` (already a dep), `assert_cmd` for CLI e2e.

**Design spec:** `specs/2026-06-28-hector-watch-tui-design.md`

## Global Constraints

- **No schema change.** Telemetry stays `SCHEMA_VERSION = 5`; verdict stays `5`. Watch reads `LogEntry::Check` / `PerCheckRecord` as-is. Do not add `message`/`exit_code` fields (spec §9).
- **Read-only.** Watch never runs checks, never writes telemetry, and never calls `trust::ensure_trusted`. It loads config via `HectorEngine::load` (best-effort) and reads the log.
- **Coverage:** every touched `crates/*/src/` file must hit ≥80% **region** coverage (`bash scripts/ci-coverage.sh`, cargo-llvm-cov; CI-enforced per file, **no exclusion mechanism**). Coverage tooling does not run locally (no llvm-tools-preview) — write the tests; CI verifies. The TUI loop is the one inherently-uncovered surface; §coverage strategy below keeps each file's aggregate over the gate.
- **Cognitive complexity:** ≤15 per function (clippy `clippy.toml`, `#![warn(clippy::cognitive_complexity)]` at crate roots). Refactor over `#[allow]`; decompose `summarize` into helpers to stay under.
- **Lint/format:** `cargo clippy --all-targets -- -D warnings` and `cargo fmt` clean before every commit.
- **TDD:** every behavioral change starts with a failing test.
- **`Cargo.lock` is gitignored** — never `git add` it.
- **Binary name is `hector`**, not `hector-cli`.
- **Outer exit codes:** `watch` adds one new code path — no-TTY exits `1` with a message. The existing `0/1/2/3` contract for `check` is untouched.
- **After each phase**, request code review from a separate agent before moving on.
- **Clean up** any `cargo build --release` / `cargo mutants` artifacts you create (`cargo clean -p <crate>`); the cleanup-build-artifacts skill covers this.

### Coverage strategy (read before Phase 3/4)

The gate is per-file with no ignore list. The TUI's terminal setup + event loop cannot be unit-tested. To keep every file ≥80%:

- All **decision logic** (aggregation, ranking, formatting, glyphs, line building, key handling) lives in pure functions with exhaustive tests.
- The **draw glue** (`ui`/`render_*`) is verified by rendering into `ratatui::backend::TestBackend` and asserting on the buffer — these tests count toward coverage.
- The **e2e test** (`assert_cmd`) runs the real binary; the spawned process's coverage IS captured (same mechanism `update.rs`'s no-receipt branch relies on), so the no-TTY branch of `run()` is covered.
- The **irreducible remainder** — `run_tui` (raw mode / alternate screen) and `event_loop` (poll/read) — is the only uncovered code. Keep it minimal. Co-locating it in `commands/watch.rs` with the heavily-tested pure helpers + `TestBackend` tests keeps that file's aggregate over 80%. If CI shows the file below threshold, shrink `event_loop`/`run_tui` further (push any remaining branch into a tested helper) — do **not** reach for `COVERAGE_THRESHOLD`, which is a local investigation override only.

---

## File Structure

**Created (hector-core):**
- `crates/hector-core/src/watch.rs` — pure view-model + aggregation + formatting. Types: `ArmedCheck`, `CheckRollup`, `LogSummary`. Fns: `summarize`, `fmt_elapsed`, `short_time`, `lifecycle_badge`, `status_glyph`. Carries its own ≥80% via unit tests.

**Modified (hector-core):**
- `crates/hector-core/src/lib.rs` — add `pub mod watch;`.

**Created (hector-cli):**
- `crates/hector-cli/src/commands/watch.rs` — `run(dir)`, pure `ViewState`/`View`/`Loop`/`handle_key`/`stream_lines`/`explorer_lines`, draw `ui`/`render_stream`/`render_explorer`, impure `run_tui`/`event_loop`. Plus unit tests + `TestBackend` tests.
- `crates/hector-cli/tests/cli_e2e_watch.rs` — e2e: `hector watch` in a non-TTY exits 1 with the expected message.

**Modified (hector-cli):**
- `crates/hector-cli/Cargo.toml` — add `ratatui = "0.29"`.
- `crates/hector-cli/src/cli.rs` — add `Watch { dir }` subcommand.
- `crates/hector-cli/src/main.rs` — dispatch `Command::Watch`.
- `crates/hector-cli/src/commands/mod.rs` — `pub mod watch;`.

**Modified (docs):**
- `docs/reference/cli.md` — document `hector watch`.
- `CHANGELOG.md` — changelog entry.

**Untouched (regression guards):** `telemetry.rs`, `verdict.rs`, `runner.rs`, `commands/check.rs`, trust — watch only consumes their public surface.

---

# Phase 1 — Core aggregation (`hector-core::watch`)

Pure, dependency-light, no terminal. This phase compiles and tests on its own and is the foundation both views render from. Definitions follow spec §6/§7 verbatim.

### Task 1.1: View-model types + `summarize`

**Files:**
- Create: `crates/hector-core/src/watch.rs`
- Modify: `crates/hector-core/src/lib.rs` (add `pub mod watch;`)

**Interfaces:**
- Consumes: `crate::telemetry::LogEntry`, `crate::telemetry::PerCheckRecord`, `crate::verdict::Status`, `crate::config::Lifecycle`.
- Produces:
  - `pub struct ArmedCheck { pub name: String, pub on: Vec<Lifecycle> }`
  - `pub struct CheckRollup { pub name: String, pub on: Vec<Lifecycle>, pub runs: usize, pub blocks: usize, pub internal: usize, pub p50_ms: Option<u64> }` with `pub fn rate(&self) -> f64`
  - `pub struct LogSummary { pub runs: usize, pub blocks: usize, pub internal: usize, pub pass: usize, pub rollups: Vec<CheckRollup> }` with `pub fn pass_pct(&self) -> Option<u32>`
  - `pub fn summarize(entries: &[LogEntry], armed: &[ArmedCheck]) -> LogSummary` — rollups ranked by blocks desc, then rate desc, then name asc.

- [ ] **Step 1: Add the module to the crate root.**

In `crates/hector-core/src/lib.rs`, add `pub mod watch;` in alphabetical position (after `pub mod verdict;` is fine, or alongside the others — keep the list sorted: insert after `pub mod verdict;`).

- [ ] **Step 2: Write failing tests for `summarize`.**

Create `crates/hector-core/src/watch.rs` with only the test module first (it won't compile until Step 3 adds the types — that's the failing state):

```rust
//! Pure aggregation + formatting for `hector watch`. No I/O, no terminal.
//!
//! `summarize` folds the telemetry log (+ the configured check list) into a
//! `LogSummary` the TUI renders. This is the single definition of the watch
//! numbers (spec §6) — `hector review` should consume it too so they agree.

use crate::config::Lifecycle;
use crate::telemetry::{LogEntry, PerCheckRecord};
use crate::verdict::Status;
use std::collections::HashMap;

// (types + summarize added in Step 3)

#[cfg(test)]
mod tests {
    use super::*;

    fn check(file: &str, event: &str, status: Status, records: Vec<PerCheckRecord>) -> LogEntry {
        LogEntry::Check {
            ts: "2026-06-28T14:00:00+00:00".into(),
            file: Some(file.into()),
            set_size: None,
            event: event.into(),
            status,
            elapsed_ms: 10,
            checks: records,
        }
    }
    fn rec(name: &str, status: Status, ms: u64) -> PerCheckRecord {
        PerCheckRecord { check: name.into(), step: None, status, elapsed_ms: ms, reason: None }
    }

    #[test]
    fn empty_log_is_all_zero_and_pass_pct_none() {
        let s = summarize(&[], &[]);
        assert_eq!((s.runs, s.blocks, s.internal, s.pass), (0, 0, 0, 0));
        assert_eq!(s.pass_pct(), None);
        assert!(s.rollups.is_empty());
    }

    #[test]
    fn counts_entries_by_status_and_pass_pct_rounds() {
        // 3 pass, 1 block => 4 runs, 75% pass
        let entries = vec![
            check("a.ts", "write", Status::Pass, vec![rec("lint", Status::Pass, 5)]),
            check("b.ts", "write", Status::Pass, vec![rec("lint", Status::Pass, 5)]),
            check("c.ts", "write", Status::Pass, vec![rec("lint", Status::Pass, 5)]),
            check("d.ts", "write", Status::Block, vec![rec("lint", Status::Block, 5)]),
        ];
        let s = summarize(&entries, &[]);
        assert_eq!((s.runs, s.blocks, s.internal, s.pass), (4, 1, 0, 3));
        assert_eq!(s.pass_pct(), Some(75));
    }

    #[test]
    fn per_check_rollup_rate_and_p50() {
        let entries = vec![
            check("a", "write", Status::Block, vec![rec("nft", Status::Block, 10)]),
            check("b", "write", Status::Pass, vec![rec("nft", Status::Pass, 20)]),
            check("c", "write", Status::Pass, vec![rec("nft", Status::Pass, 30)]),
        ];
        let s = summarize(&entries, &[]);
        let r = s.rollups.iter().find(|r| r.name == "nft").unwrap();
        assert_eq!(r.runs, 3);
        assert_eq!(r.blocks, 1);
        assert!((r.rate() - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(r.p50_ms, Some(20)); // sorted [10,20,30], lower-median index 1
    }

    #[test]
    fn p50_lower_median_on_even_counts() {
        let entries = vec![
            check("a", "write", Status::Pass, vec![rec("x", Status::Pass, 10)]),
            check("b", "write", Status::Pass, vec![rec("x", Status::Pass, 40)]),
        ];
        let s = summarize(&entries, &[]);
        let r = s.rollups.iter().find(|r| r.name == "x").unwrap();
        assert_eq!(r.p50_ms, Some(10)); // [10,40] lower-median = 10
    }

    #[test]
    fn internal_errors_counted_per_check_and_overall() {
        let entries = vec![check(
            "a",
            "write",
            Status::InternalError,
            vec![rec("types", Status::InternalError, 240)],
        )];
        let s = summarize(&entries, &[]);
        assert_eq!(s.internal, 1);
        let r = s.rollups.iter().find(|r| r.name == "types").unwrap();
        assert_eq!(r.internal, 1);
    }

    #[test]
    fn armed_checks_with_zero_runs_appear_with_lifecycle() {
        let armed = vec![ArmedCheck { name: "unused".into(), on: vec![Lifecycle::Write] }];
        let s = summarize(&[], &armed);
        let r = s.rollups.iter().find(|r| r.name == "unused").unwrap();
        assert_eq!((r.runs, r.blocks), (0, 0));
        assert_eq!(r.p50_ms, None);
        assert_eq!(r.on, vec![Lifecycle::Write]);
    }

    #[test]
    fn ranking_is_blocks_then_rate_then_name() {
        // many: 1 block / 10 runs (rate .1); few: 1 block / 2 runs (rate .5); zero: 0 blocks
        let mut entries = vec![check("x", "write", Status::Block, vec![rec("many", Status::Block, 1)])];
        for _ in 0..9 {
            entries.push(check("x", "write", Status::Pass, vec![rec("many", Status::Pass, 1)]));
        }
        entries.push(check("y", "write", Status::Block, vec![rec("few", Status::Block, 1)]));
        entries.push(check("y", "write", Status::Pass, vec![rec("few", Status::Pass, 1)]));
        entries.push(check("z", "write", Status::Pass, vec![rec("zero", Status::Pass, 1)]));
        let s = summarize(&entries, &[]);
        let names: Vec<&str> = s.rollups.iter().map(|r| r.name.as_str()).collect();
        // both blockers (tie on 1 block) ordered by rate desc: few (.5) before many (.1); zero last
        assert_eq!(names, vec!["few", "many", "zero"]);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail to compile.**

Run: `cargo test -p hector-core watch::`
Expected: FAIL — `cannot find type/function ArmedCheck/summarize/...` (types not yet defined).

- [ ] **Step 4: Implement the types and `summarize` (decomposed to stay under complexity 15).**

Insert above the `#[cfg(test)]` block in `crates/hector-core/src/watch.rs`:

```rust
/// A configured check projected to what the summary needs (name + lifecycle).
/// Built by the CLI from `HectorEngine::checks()`; keeps core free of the
/// full `Check`/`BTreeMap` shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmedCheck {
    pub name: String,
    pub on: Vec<Lifecycle>,
}

/// Per-check rollup across the whole log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRollup {
    pub name: String,
    pub on: Vec<Lifecycle>,
    pub runs: usize,
    pub blocks: usize,
    pub internal: usize,
    pub p50_ms: Option<u64>,
}

impl CheckRollup {
    /// Block rate in [0,1]; `0.0` when the check never ran.
    pub fn rate(&self) -> f64 {
        if self.runs == 0 {
            0.0
        } else {
            self.blocks as f64 / self.runs as f64
        }
    }
}

/// Whole-log aggregate the TUI renders from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSummary {
    pub runs: usize,
    pub blocks: usize,
    pub internal: usize,
    pub pass: usize,
    pub rollups: Vec<CheckRollup>,
}

impl LogSummary {
    /// Entry-level pass percent, rounded. `None` on an empty log (avoids a
    /// misleading "100%").
    pub fn pass_pct(&self) -> Option<u32> {
        if self.runs == 0 {
            None
        } else {
            Some(((self.pass as f64 / self.runs as f64) * 100.0).round() as u32)
        }
    }
}

#[derive(Default)]
struct Acc {
    runs: usize,
    blocks: usize,
    internal: usize,
    elapsed: Vec<u64>,
}

/// Lower-median of an already-sorted slice. `None` when empty.
fn median(sorted: &[u64]) -> Option<u64> {
    if sorted.is_empty() {
        None
    } else {
        Some(sorted[(sorted.len() - 1) / 2])
    }
}

/// Entry-level totals + per-check accumulators.
fn accumulate(entries: &[LogEntry]) -> (LogSummary, HashMap<String, Acc>) {
    let mut totals = LogSummary { runs: 0, blocks: 0, internal: 0, pass: 0, rollups: Vec::new() };
    let mut per: HashMap<String, Acc> = HashMap::new();
    for entry in entries {
        let LogEntry::Check { status, checks, .. } = entry;
        totals.runs += 1;
        match status {
            Status::Pass => totals.pass += 1,
            Status::Block => totals.blocks += 1,
            Status::InternalError => totals.internal += 1,
        }
        for c in checks {
            let a = per.entry(c.check.clone()).or_default();
            a.runs += 1;
            a.elapsed.push(c.elapsed_ms);
            match c.status {
                Status::Pass => {}
                Status::Block => a.blocks += 1,
                Status::InternalError => a.internal += 1,
            }
        }
    }
    (totals, per)
}

/// Build the ranked rollup list from armed checks unioned with seen checks.
fn build_rollups(armed: &[ArmedCheck], per: &HashMap<String, Acc>) -> Vec<CheckRollup> {
    let mut names: Vec<String> = armed.iter().map(|a| a.name.clone()).collect();
    for k in per.keys() {
        if !names.contains(k) {
            names.push(k.clone());
        }
    }
    let on_for = |name: &str| {
        armed
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.on.clone())
            .unwrap_or_default()
    };
    let mut rollups: Vec<CheckRollup> = names
        .into_iter()
        .map(|name| {
            let (runs, blocks, internal, p50) = match per.get(&name) {
                Some(a) => {
                    let mut el = a.elapsed.clone();
                    el.sort_unstable();
                    (a.runs, a.blocks, a.internal, median(&el))
                }
                None => (0, 0, 0, None),
            };
            CheckRollup { on: on_for(&name), name, runs, blocks, internal, p50_ms: p50 }
        })
        .collect();
    rollups.sort_by(|a, b| {
        b.blocks
            .cmp(&a.blocks)
            .then_with(|| b.rate().partial_cmp(&a.rate()).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.name.cmp(&b.name))
    });
    rollups
}

/// Fold the log + armed checks into a `LogSummary` (spec §6).
pub fn summarize(entries: &[LogEntry], armed: &[ArmedCheck]) -> LogSummary {
    let (mut summary, per) = accumulate(entries);
    summary.rollups = build_rollups(armed, &per);
    summary
}
```

- [ ] **Step 5: Run the tests to verify they pass.**

Run: `cargo test -p hector-core watch::`
Expected: PASS (all 7 tests). Then `cargo clippy -p hector-core --all-targets -- -D warnings` and `cargo fmt` clean.

- [ ] **Step 6: Commit.**

```bash
git add crates/hector-core/src/watch.rs crates/hector-core/src/lib.rs
git commit -m "feat(core): add watch::summarize log aggregation"
```

### Task 1.2: Formatting helpers

**Files:**
- Modify: `crates/hector-core/src/watch.rs`

**Interfaces:**
- Produces:
  - `pub fn fmt_elapsed(ms: u64) -> String`
  - `pub fn short_time(ts: &str) -> String`
  - `pub fn lifecycle_badge(on: &[Lifecycle]) -> String`
  - `pub fn status_glyph(status: Status) -> char`

- [ ] **Step 1: Write failing tests.**

Add to the `tests` module in `crates/hector-core/src/watch.rs`:

```rust
#[test]
fn fmt_elapsed_boundaries() {
    assert_eq!(fmt_elapsed(12), "12ms");
    assert_eq!(fmt_elapsed(999), "999ms");
    assert_eq!(fmt_elapsed(1000), "1.0s");
    assert_eq!(fmt_elapsed(1200), "1.2s");
    assert_eq!(fmt_elapsed(30_000), "30.0s");
    assert_eq!(fmt_elapsed(90_500), "1m30s");
}

#[test]
fn short_time_renders_hms_from_rfc3339() {
    assert_eq!(short_time("2026-06-28T14:23:09.5+00:00"), "14:23:09");
}

#[test]
fn short_time_falls_back_on_garbage() {
    assert_eq!(short_time("nope"), "nope");
}

#[test]
fn lifecycle_badge_variants() {
    assert_eq!(lifecycle_badge(&[Lifecycle::Write]), "[w]");
    assert_eq!(lifecycle_badge(&[Lifecycle::PreCommit]), "[c]");
    assert_eq!(lifecycle_badge(&[Lifecycle::Write, Lifecycle::PreCommit]), "[w+c]");
    assert_eq!(lifecycle_badge(&[]), "[]");
}

#[test]
fn status_glyph_per_status() {
    assert_eq!(status_glyph(Status::Pass), '✓');
    assert_eq!(status_glyph(Status::Block), '✗');
    assert_eq!(status_glyph(Status::InternalError), '⚠');
}
```

- [ ] **Step 2: Run to verify failure.**

Run: `cargo test -p hector-core watch::tests::fmt_elapsed_boundaries`
Expected: FAIL — `cannot find function fmt_elapsed`.

- [ ] **Step 3: Implement the helpers.**

Add above the `#[cfg(test)]` block in `crates/hector-core/src/watch.rs`:

```rust
/// Human elapsed: `12ms` / `1.2s` / `1m30s`.
pub fn fmt_elapsed(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let secs = ms / 1000;
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

/// `HH:MM:SS` from the log's RFC3339 `ts`, in the timestamp's own offset
/// (UTC, as written). Falls back to the first 8 chars if unparseable.
pub fn short_time(ts: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|_| ts.chars().take(8).collect())
}

/// `[w]` / `[c]` / `[w+c]` lifecycle badge (spec §7).
pub fn lifecycle_badge(on: &[Lifecycle]) -> String {
    let w = on.contains(&Lifecycle::Write);
    let c = on.contains(&Lifecycle::PreCommit);
    match (w, c) {
        (true, true) => "[w+c]".into(),
        (true, false) => "[w]".into(),
        (false, true) => "[c]".into(),
        (false, false) => "[]".into(),
    }
}

/// `✓` pass / `✗` block / `⚠` internal error.
pub fn status_glyph(status: Status) -> char {
    match status {
        Status::Pass => '✓',
        Status::Block => '✗',
        Status::InternalError => '⚠',
    }
}
```

Note: `chrono` is already a workspace dependency used by `hector-core`'s runner; no `Cargo.toml` change needed. If `cargo build -p hector-core` reports `chrono` is not a direct dep of the core crate, add `chrono.workspace = true` to `crates/hector-core/Cargo.toml` `[dependencies]` and commit that with this task.

- [ ] **Step 4: Run to verify pass.**

Run: `cargo test -p hector-core watch::`
Expected: PASS (all watch tests). `cargo clippy -p hector-core --all-targets -- -D warnings` + `cargo fmt` clean.

- [ ] **Step 5: Commit.**

```bash
git add crates/hector-core/src/watch.rs crates/hector-core/Cargo.toml
git commit -m "feat(core): add watch formatting helpers"
```

**Phase 1 gate:** request code review from a separate agent before Phase 2.

---

# Phase 2 — CLI command: wiring, no-TTY exit, view state

Adds the `watch` subcommand end-to-end with a clean non-interactive exit, plus the pure interaction logic. The TUI itself is drawn in Phase 3.

### Task 2.1: Subcommand wiring + no-TTY exit (e2e)

**Files:**
- Modify: `crates/hector-cli/Cargo.toml`
- Create: `crates/hector-cli/src/commands/watch.rs`
- Modify: `crates/hector-cli/src/commands/mod.rs`
- Modify: `crates/hector-cli/src/cli.rs`
- Modify: `crates/hector-cli/src/main.rs`
- Create: `crates/hector-cli/tests/cli_e2e_watch.rs`

**Interfaces:**
- Produces: `commands::watch::run(dir: &Path) -> anyhow::Result<i32>`. Returns `1` (with a stderr message) when stdout is not a TTY; otherwise enters the TUI (stubbed `Ok(0)` in this task, real loop in Phase 4).
- Consumes: clap `Command::Watch { dir: PathBuf }`.

- [ ] **Step 1: Add the ratatui dependency.**

In `crates/hector-cli/Cargo.toml`, under `[dependencies]` (after the `axoupdater` block), add:

```toml
# Live TUI (`hector watch`): renders the telemetry stream + per-check explorer.
# ratatui re-exports crossterm, so we depend on it alone for backend + events.
ratatui = "0.29"
```

- [ ] **Step 2: Write the failing e2e test.**

Create `crates/hector-cli/tests/cli_e2e_watch.rs`:

```rust
//! E2E for `hector watch`. In the test harness stdout is piped (not a TTY),
//! so `watch` hits the no-TTY branch: exit 1 with a guidance message. This
//! also exercises `run()`'s entry path for coverage.
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn watch_without_tty_exits_one_with_message() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("hector")
        .unwrap()
        .arg("watch")
        .arg("--dir")
        .arg(dir.path())
        .assert()
        .failure()
        .code(1)
        .stderr(contains("requires an interactive terminal"));
}
```

- [ ] **Step 3: Run to verify failure.**

Run: `cargo test -p hector-cli --test cli_e2e_watch`
Expected: FAIL — no `watch` subcommand yet (clap errors / binary build fails).

- [ ] **Step 4: Create the command module (no-TTY branch + TUI stub).**

Create `crates/hector-cli/src/commands/watch.rs`:

```rust
//! `hector watch` — a read-only live TUI over `.hector/log.jsonl`.
//!
//! All decision logic (aggregation in core, plus `handle_key`/`stream_lines`/
//! `explorer_lines`/`ui` here) is pure and tested; the only uncovered code is
//! the terminal setup (`run_tui`) and event loop (`event_loop`), kept minimal.
use anyhow::Result;
use std::io::IsTerminal;
use std::path::Path;

/// Entry point. Requires an interactive terminal; otherwise exits 1 with a hint.
pub fn run(dir: &Path) -> Result<i32> {
    if !std::io::stdout().is_terminal() {
        eprintln!("hector watch: requires an interactive terminal (no TTY detected).");
        eprintln!(
            "A non-interactive `--once` snapshot is planned; for now inspect {}/.hector/log.jsonl directly.",
            dir.display()
        );
        return Ok(1);
    }
    // Phase 4 replaces this stub with the live loop.
    Ok(0)
}
```

- [ ] **Step 5: Register the module.**

In `crates/hector-cli/src/commands/mod.rs`, add `pub mod watch;` (keep sorted — after `pub mod validate;` or alphabetically before it; match existing order which is alphabetical, so insert `pub mod watch;` after `pub mod validate;`).

- [ ] **Step 6: Add the clap subcommand.**

In `crates/hector-cli/src/cli.rs`, add to `enum Command` (after `Update,`):

```rust
    /// Live TUI over the telemetry log: a stream of check runs and a per-check
    /// explorer. Read-only; requires an interactive terminal.
    Watch {
        /// Directory containing `.hector.yml` / `.hector/log.jsonl`. Defaults to cwd.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
```

- [ ] **Step 7: Dispatch in main.**

In `crates/hector-cli/src/main.rs`, add to the `match cli.command` (after the `Command::Update` arm):

```rust
        Command::Watch { dir } => commands::watch::run(&dir)?,
```

- [ ] **Step 8: Run the e2e test to verify pass.**

Run: `cargo test -p hector-cli --test cli_e2e_watch`
Expected: PASS. Then `cargo clippy -p hector-cli --all-targets -- -D warnings` + `cargo fmt` clean.

- [ ] **Step 9: Commit.**

```bash
git add crates/hector-cli/Cargo.toml crates/hector-cli/src/commands/watch.rs \
  crates/hector-cli/src/commands/mod.rs crates/hector-cli/src/cli.rs \
  crates/hector-cli/src/main.rs crates/hector-cli/tests/cli_e2e_watch.rs
git commit -m "feat(cli): add hector watch subcommand with no-TTY exit"
```

### Task 2.2: View state + key handling

**Files:**
- Modify: `crates/hector-cli/src/commands/watch.rs`

**Interfaces:**
- Consumes: `hector_core::watch::LogSummary`.
- Produces:
  - `pub enum View { Stream, Explorer }`
  - `pub struct ViewState { pub view: View, pub selected: usize, pub filter: Option<String> }` (`Default` → Stream, 0, None)
  - `pub enum Loop { Continue, Quit }`
  - `pub fn handle_key(code: KeyCode, state: &mut ViewState, summary: &LogSummary) -> Loop`

Key map (spec §5.3): `q`/`Esc` → Quit; `Tab`/`Right`/`Left` → toggle view; in Explorer `Up`/`Down` move `selected` (clamped to `rollups.len()`); `Enter` in Explorer sets `filter` to the selected rollup's name and switches to Stream.

- [ ] **Step 1: Write failing tests.**

Add a test module to `crates/hector-cli/src/commands/watch.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use hector_core::config::Lifecycle;
    use hector_core::watch::{CheckRollup, LogSummary};

    fn summary_with(names: &[&str]) -> LogSummary {
        LogSummary {
            runs: 0,
            blocks: 0,
            internal: 0,
            pass: 0,
            rollups: names
                .iter()
                .map(|n| CheckRollup {
                    name: (*n).into(),
                    on: vec![Lifecycle::Write],
                    runs: 0,
                    blocks: 0,
                    internal: 0,
                    p50_ms: None,
                })
                .collect(),
        }
    }

    #[test]
    fn q_and_esc_quit() {
        let mut s = ViewState::default();
        assert_eq!(handle_key(KeyCode::Char('q'), &mut s, &summary_with(&[])), Loop::Quit);
        assert_eq!(handle_key(KeyCode::Esc, &mut s, &summary_with(&[])), Loop::Quit);
    }

    #[test]
    fn tab_toggles_view() {
        let mut s = ViewState::default();
        assert!(matches!(s.view, View::Stream));
        handle_key(KeyCode::Tab, &mut s, &summary_with(&[]));
        assert!(matches!(s.view, View::Explorer));
        handle_key(KeyCode::Tab, &mut s, &summary_with(&[]));
        assert!(matches!(s.view, View::Stream));
    }

    #[test]
    fn down_up_clamp_in_explorer() {
        let mut s = ViewState { view: View::Explorer, selected: 0, filter: None };
        let sum = summary_with(&["a", "b"]);
        handle_key(KeyCode::Down, &mut s, &sum);
        assert_eq!(s.selected, 1);
        handle_key(KeyCode::Down, &mut s, &sum); // clamp at len-1
        assert_eq!(s.selected, 1);
        handle_key(KeyCode::Up, &mut s, &sum);
        assert_eq!(s.selected, 0);
        handle_key(KeyCode::Up, &mut s, &sum); // clamp at 0
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn enter_in_explorer_filters_stream() {
        let mut s = ViewState { view: View::Explorer, selected: 1, filter: None };
        let sum = summary_with(&["a", "b"]);
        handle_key(KeyCode::Enter, &mut s, &sum);
        assert_eq!(s.filter.as_deref(), Some("b"));
        assert!(matches!(s.view, View::Stream));
    }

    #[test]
    fn navigation_returns_continue() {
        let mut s = ViewState::default();
        assert_eq!(handle_key(KeyCode::Tab, &mut s, &summary_with(&[])), Loop::Continue);
    }
}
```

- [ ] **Step 2: Run to verify failure.**

Run: `cargo test -p hector-cli watch::tests::tab_toggles_view`
Expected: FAIL — `View`/`ViewState`/`handle_key` undefined.

- [ ] **Step 3: Implement the state + key handler.**

Add to `crates/hector-cli/src/commands/watch.rs` (above the `#[cfg(test)]` block). Add the import at the top of the file alongside the others:

```rust
use hector_core::watch::LogSummary;
use ratatui::crossterm::event::KeyCode;
```

Then:

```rust
/// Active pane.
pub enum View {
    Stream,
    Explorer,
}

/// What the loop should do after a key.
#[derive(Debug, PartialEq, Eq)]
pub enum Loop {
    Continue,
    Quit,
}

/// UI state threaded through the loop. Pure data.
pub struct ViewState {
    pub view: View,
    pub selected: usize,
    pub filter: Option<String>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self { view: View::Stream, selected: 0, filter: None }
    }
}

fn toggle(view: &View) -> View {
    match view {
        View::Stream => View::Explorer,
        View::Explorer => View::Stream,
    }
}

/// Map a key to a state mutation (spec §5.3). Pure; no I/O.
pub fn handle_key(code: KeyCode, state: &mut ViewState, summary: &LogSummary) -> Loop {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return Loop::Quit,
        KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
            state.view = toggle(&state.view);
            state.selected = 0;
        }
        KeyCode::Down if matches!(state.view, View::Explorer) => {
            let max = summary.rollups.len().saturating_sub(1);
            state.selected = (state.selected + 1).min(max);
        }
        KeyCode::Up if matches!(state.view, View::Explorer) => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Enter if matches!(state.view, View::Explorer) => {
            if let Some(r) = summary.rollups.get(state.selected) {
                state.filter = Some(r.name.clone());
                state.view = View::Stream;
            }
        }
        _ => {}
    }
    Loop::Continue
}
```

- [ ] **Step 4: Run to verify pass.**

Run: `cargo test -p hector-cli watch::`
Expected: PASS (all 5 state tests). `cargo clippy -p hector-cli --all-targets -- -D warnings` + `cargo fmt` clean.

- [ ] **Step 5: Commit.**

```bash
git add crates/hector-cli/src/commands/watch.rs
git commit -m "feat(cli): add watch view state and key handling"
```

**Phase 2 gate:** request code review from a separate agent before Phase 3.

---

# Phase 3 — Rendering (pure line builders + TestBackend)

Build the two views as pure `Vec<Line>` builders (tested by extracting text), then a `ui()` that lays out tabs/header/footer and renders them — verified with `TestBackend`.

### Task 3.1: Stream line builder

**Files:**
- Modify: `crates/hector-cli/src/commands/watch.rs`

**Interfaces:**
- Consumes: `hector_core::telemetry::LogEntry`, `hector_core::watch::{short_time, fmt_elapsed, status_glyph}`, `hector_core::verdict::Status`.
- Produces: `pub fn stream_lines(entries: &[LogEntry], filter: Option<&str>) -> Vec<ratatui::text::Line<'static>>` — newest-first; each entry a header line + optional detail sub-line(s) for non-pass.

- [ ] **Step 1: Write failing tests.**

Add to the `tests` module in `crates/hector-cli/src/commands/watch.rs`:

```rust
use hector_core::telemetry::{LogEntry, PerCheckRecord};
use hector_core::verdict::Status;
use ratatui::text::Line;

fn line_text(l: &Line) -> String {
    l.spans.iter().map(|s| s.content.as_ref()).collect()
}
fn all_text(lines: &[Line]) -> String {
    lines.iter().map(line_text).collect::<Vec<_>>().join("\n")
}
fn entry(file: Option<&str>, set: Option<usize>, event: &str, status: Status, recs: Vec<PerCheckRecord>) -> LogEntry {
    LogEntry::Check {
        ts: "2026-06-28T14:23:09+00:00".into(),
        file: file.map(Into::into),
        set_size: set,
        event: event.into(),
        status,
        elapsed_ms: 12,
        checks: recs,
    }
}
fn prec(name: &str, status: Status, reason: Option<&str>) -> PerCheckRecord {
    PerCheckRecord { check: name.into(), step: None, status, elapsed_ms: 12, reason: reason.map(Into::into) }
}

#[test]
fn stream_renders_pass_row_with_time_file_elapsed_event() {
    let e = vec![entry(Some("src/auth.ts"), None, "write", Status::Pass, vec![prec("lint", Status::Pass, None)])];
    let text = all_text(&stream_lines(&e, None));
    assert!(text.contains("14:23:09"));
    assert!(text.contains("src/auth.ts"));
    assert!(text.contains("12ms"));
    assert!(text.contains("write"));
}

#[test]
fn stream_block_row_has_check_and_write_rejected_no_message() {
    let e = vec![entry(Some("src/auth.test.ts"), None, "write", Status::Block, vec![prec("no-focused-tests", Status::Block, None)])];
    let text = all_text(&stream_lines(&e, None));
    assert!(text.contains("no-focused-tests"));
    assert!(text.contains("write rejected"));
    assert!(!text.contains("exited")); // exit code not stored, not shown
}

#[test]
fn stream_precommit_row_shows_set_size_and_commit() {
    let e = vec![entry(None, Some(47), "pre-commit", Status::Pass, vec![prec("lint", Status::Pass, None)])];
    let text = all_text(&stream_lines(&e, None));
    assert!(text.contains("pre-commit · 47 files"));
    assert!(text.contains("commit"));
}

#[test]
fn stream_internal_error_shows_reason() {
    let e = vec![entry(Some("big.ts"), None, "write", Status::InternalError, vec![prec("types-pass", Status::InternalError, Some("timeout"))])];
    let text = all_text(&stream_lines(&e, None));
    assert!(text.contains("check error: timeout"));
}

#[test]
fn stream_is_newest_first() {
    let mut a = entry(Some("old.ts"), None, "write", Status::Pass, vec![prec("lint", Status::Pass, None)]);
    if let LogEntry::Check { ts, .. } = &mut a {
        *ts = "2026-06-28T14:00:00+00:00".into();
    }
    let b = entry(Some("new.ts"), None, "write", Status::Pass, vec![prec("lint", Status::Pass, None)]);
    let lines = stream_lines(&[a, b], None);
    // first rendered line mentions the newest entry
    assert!(line_text(&lines[0]).contains("new.ts"));
}

#[test]
fn stream_filter_keeps_only_matching_check() {
    let e = vec![
        entry(Some("a.ts"), None, "write", Status::Pass, vec![prec("lint", Status::Pass, None)]),
        entry(Some("b.ts"), None, "write", Status::Pass, vec![prec("types", Status::Pass, None)]),
    ];
    let text = all_text(&stream_lines(&e, Some("types")));
    assert!(text.contains("b.ts"));
    assert!(!text.contains("a.ts"));
}
```

- [ ] **Step 2: Run to verify failure.**

Run: `cargo test -p hector-cli watch::tests::stream_renders_pass_row_with_time_file_elapsed_event`
Expected: FAIL — `stream_lines` undefined.

- [ ] **Step 3: Implement `stream_lines` + color constants.**

Add to `crates/hector-cli/src/commands/watch.rs`. Add imports at the top:

```rust
use hector_core::telemetry::LogEntry;
use hector_core::verdict::Status;
use hector_core::watch::{fmt_elapsed, short_time, status_glyph};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
```

Color vocabulary + builder:

```rust
const ORANGE: Color = Color::Rgb(255, 92, 56);
const GREEN: Color = Color::Rgb(52, 211, 153);
const AMBER: Color = Color::Rgb(245, 191, 79);
const MUTED: Color = Color::Rgb(132, 132, 140);

fn status_color(status: Status) -> Color {
    match status {
        Status::Pass => GREEN,
        Status::Block => ORANGE,
        Status::InternalError => AMBER,
    }
}

/// Target label: a file path, or `pre-commit · N files` for a set run.
fn target_label(file: Option<&String>, set_size: Option<usize>) -> String {
    match file {
        Some(f) => f.clone(),
        None => format!("pre-commit · {} files", set_size.unwrap_or(0)),
    }
}

/// Detail sub-line text for a non-pass per-check record. `event` decides the
/// block verb. Returns `None` for passing records.
fn detail_text(check: &str, status: Status, reason: Option<&str>, event: &str) -> Option<String> {
    match status {
        Status::Pass => None,
        Status::Block => {
            let verb = if event == "pre-commit" { "commit blocked" } else { "write rejected" };
            Some(format!("  └ {check} · {verb}"))
        }
        Status::InternalError => {
            Some(format!("  └ {check} · check error: {}", reason.unwrap_or("unknown")))
        }
    }
}

/// Newest-first stream lines. `filter` keeps only entries whose `checks`
/// contains that check name.
pub fn stream_lines(entries: &[LogEntry], filter: Option<&str>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for entry in entries.iter().rev() {
        let LogEntry::Check { ts, file, set_size, event, status, elapsed_ms, checks } = entry;
        if let Some(f) = filter {
            if !checks.iter().any(|c| c.check == f) {
                continue;
            }
        }
        let glyph = status_glyph(*status);
        let target = target_label(file.as_ref(), *set_size);
        let badge = if event == "pre-commit" { "commit" } else { "write" };
        lines.push(Line::from(vec![
            Span::styled(format!("{:>8}  ", short_time(ts)), Style::default().fg(MUTED)),
            Span::styled(format!("{glyph}  "), Style::default().fg(status_color(*status))),
            Span::raw(format!("{target:<40}")),
            Span::styled(format!("  {:>6}  ", fmt_elapsed(*elapsed_ms)), Style::default().fg(MUTED)),
            Span::styled(badge.to_string(), Style::default().fg(MUTED)),
        ]));
        for c in checks {
            if let Some(text) = detail_text(&c.check, c.status, c.reason.as_deref(), event) {
                lines.push(Line::from(Span::styled(text, Style::default().fg(status_color(c.status)))));
            }
        }
    }
    lines
}
```

- [ ] **Step 4: Run to verify pass.**

Run: `cargo test -p hector-cli watch::`
Expected: PASS. `cargo clippy -p hector-cli --all-targets -- -D warnings` + `cargo fmt` clean. (`Modifier` import may be unused until Task 3.3 — if clippy flags it, move that import into Task 3.3's step instead.)

- [ ] **Step 5: Commit.**

```bash
git add crates/hector-cli/src/commands/watch.rs
git commit -m "feat(cli): add watch stream line builder"
```

### Task 3.2: Explorer line builder

**Files:**
- Modify: `crates/hector-cli/src/commands/watch.rs`

**Interfaces:**
- Consumes: `hector_core::watch::{LogSummary, lifecycle_badge, fmt_elapsed}`.
- Produces: `pub fn explorer_lines(summary: &LogSummary, selected: usize) -> Vec<ratatui::text::Line<'static>>` — a summary line, a `CHECKS · RANKED BY BLOCKS` header, blocking rollups, a `✓ NO BLOCKS IN LOG` divider, then zero-block rollups.

- [ ] **Step 1: Write failing tests.**

Add to the `tests` module:

```rust
use hector_core::watch::{CheckRollup as Roll, LogSummary as Sum};

fn roll(name: &str, runs: usize, blocks: usize, internal: usize, p50: Option<u64>) -> Roll {
    Roll {
        name: name.into(),
        on: vec![hector_core::config::Lifecycle::Write],
        runs,
        blocks,
        internal,
        p50_ms: p50,
    }
}

#[test]
fn explorer_summary_line_has_totals_and_pass_pct() {
    let s = Sum { runs: 159, blocks: 4, internal: 1, pass: 154, rollups: vec![] };
    let text = all_text(&explorer_lines(&s, 0));
    assert!(text.contains("159 runs"));
    assert!(text.contains("4 blocks"));
    assert!(text.contains("1 internal"));
    assert!(text.contains("97% pass")); // 154/159 -> 97
}

#[test]
fn explorer_lists_blocking_then_divider_then_clean() {
    let s = Sum {
        runs: 10,
        blocks: 3,
        internal: 0,
        pass: 7,
        rollups: vec![roll("nft", 15, 3, 0, Some(11)), roll("no-secrets", 50, 0, 0, Some(3))],
    };
    let lines = explorer_lines(&s, 0);
    let text = all_text(&lines);
    assert!(text.contains("nft"));
    assert!(text.contains("20%")); // 3/15
    assert!(text.contains("11ms"));
    assert!(text.contains("NO BLOCKS IN LOG"));
    assert!(text.contains("no-secrets"));
    // ordering: nft (blocking) appears before the divider; no-secrets after
    let nft_idx = lines.iter().position(|l| line_text(l).contains("nft")).unwrap();
    let div_idx = lines.iter().position(|l| line_text(l).contains("NO BLOCKS IN LOG")).unwrap();
    let sec_idx = lines.iter().position(|l| line_text(l).contains("no-secrets")).unwrap();
    assert!(nft_idx < div_idx && div_idx < sec_idx);
}

#[test]
fn explorer_shows_internal_warning_and_dash_p50() {
    let s = Sum { runs: 1, blocks: 0, internal: 1, pass: 0, rollups: vec![roll("types", 1, 0, 1, None)] };
    let text = all_text(&explorer_lines(&s, 0));
    assert!(text.contains("⚠ 1"));
    assert!(text.contains("—")); // p50 None renders as em dash
}

#[test]
fn explorer_omits_divider_when_all_checks_block() {
    let s = Sum { runs: 1, blocks: 1, internal: 0, pass: 0, rollups: vec![roll("only", 1, 1, 0, Some(5))] };
    let text = all_text(&explorer_lines(&s, 0));
    assert!(!text.contains("NO BLOCKS IN LOG"));
}
```

- [ ] **Step 2: Run to verify failure.**

Run: `cargo test -p hector-cli watch::tests::explorer_summary_line_has_totals_and_pass_pct`
Expected: FAIL — `explorer_lines` undefined.

- [ ] **Step 3: Implement `explorer_lines`.**

Add to `crates/hector-cli/src/commands/watch.rs`. Update the `hector_core::watch` import to include the new helpers:

```rust
use hector_core::watch::{fmt_elapsed, lifecycle_badge, short_time, status_glyph, CheckRollup, LogSummary};
```

Builder:

```rust
fn pass_pct_text(summary: &LogSummary) -> String {
    summary.pass_pct().map(|p| format!("{p}% pass")).unwrap_or_else(|| "— pass".into())
}

fn rollup_line(r: &CheckRollup, selected: bool) -> Line<'static> {
    let dot_color = if r.blocks > 0 { ORANGE } else { GREEN };
    let rate = (r.rate() * 100.0).round() as u32;
    let p50 = r.p50_ms.map(fmt_elapsed).unwrap_or_else(|| "—".into());
    let warn = if r.internal > 0 { format!("  ⚠ {}", r.internal) } else { String::new() };
    let marker = if selected { "› " } else { "  " };
    let name_style = if selected {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::raw(marker),
        Span::styled("● ", Style::default().fg(dot_color)),
        Span::styled(format!("{:<20}", r.name), name_style),
        Span::styled(format!("{} ", lifecycle_badge(&r.on)), Style::default().fg(MUTED)),
        Span::styled(format!("{warn}"), Style::default().fg(AMBER)),
        Span::raw(format!("  {:>3}", r.blocks)),
        Span::styled(format!("  {:>4}%", rate), Style::default().fg(MUTED)),
        Span::styled(format!("  {:>6}", p50), Style::default().fg(MUTED)),
    ])
}

/// Explorer view: summary bar + ranked per-check table with a divider before
/// the zero-block checks (spec §5.2).
pub fn explorer_lines(summary: &LogSummary, selected: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            "log {} runs · {} blocks · {} internal · {}",
            summary.runs,
            summary.blocks,
            summary.internal,
            pass_pct_text(summary),
        ),
        Style::default().fg(MUTED),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "CHECKS · RANKED BY BLOCKS                              blocks  rate     p50",
        Style::default().fg(MUTED),
    )));

    let mut divider_emitted = false;
    for (i, r) in summary.rollups.iter().enumerate() {
        if r.blocks == 0 && !divider_emitted {
            lines.push(Line::from(Span::styled("✓ NO BLOCKS IN LOG", Style::default().fg(GREEN))));
            divider_emitted = true;
        }
        lines.push(rollup_line(r, i == selected));
    }
    lines
}
```

- [ ] **Step 4: Run to verify pass.**

Run: `cargo test -p hector-cli watch::`
Expected: PASS. `cargo clippy -p hector-cli --all-targets -- -D warnings` + `cargo fmt` clean. (`short_time`/`status_glyph` are used by `stream_lines`; the shared import line keeps them in scope.)

- [ ] **Step 5: Commit.**

```bash
git add crates/hector-cli/src/commands/watch.rs
git commit -m "feat(cli): add watch explorer line builder"
```

### Task 3.3: `ui()` layout + TestBackend smoke tests

**Files:**
- Modify: `crates/hector-cli/src/commands/watch.rs`

**Interfaces:**
- Produces: `pub fn ui(frame: &mut ratatui::Frame, entries: &[LogEntry], summary: &LogSummary, armed: usize, state: &ViewState, clock: &str)` — draws tab bar + header, the active view's lines as a `Paragraph`, and the footer.

- [ ] **Step 1: Write failing TestBackend tests.**

Add to the `tests` module:

```rust
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn buf_text(t: &Terminal<TestBackend>) -> String {
    t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
}

#[test]
fn ui_stream_view_renders_feed_and_footer() {
    let entries = vec![entry(Some("src/auth.ts"), None, "write", Status::Pass, vec![prec("lint", Status::Pass, None)])];
    let summary = Sum { runs: 1, blocks: 0, internal: 0, pass: 1, rollups: vec![roll("lint", 1, 0, 0, Some(5))] };
    let state = ViewState::default();
    let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
    term.draw(|f| ui(f, &entries, &summary, 7, &state, "14:24:00")).unwrap();
    let text = buf_text(&term);
    assert!(text.contains("stream"));
    assert!(text.contains("explorer"));
    assert!(text.contains("src/auth.ts"));
    assert!(text.contains("7 checks armed"));
    assert!(text.contains("quit"));
}

#[test]
fn ui_explorer_view_renders_table() {
    let summary = Sum { runs: 10, blocks: 1, internal: 0, pass: 9, rollups: vec![roll("nft", 5, 1, 0, Some(11))] };
    let state = ViewState { view: View::Explorer, selected: 0, filter: None };
    let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
    term.draw(|f| ui(f, &[], &summary, 7, &state, "14:24:09")).unwrap();
    let text = buf_text(&term);
    assert!(text.contains("RANKED BY BLOCKS"));
    assert!(text.contains("nft"));
}
```

- [ ] **Step 2: Run to verify failure.**

Run: `cargo test -p hector-cli watch::tests::ui_stream_view_renders_feed_and_footer`
Expected: FAIL — `ui` undefined.

- [ ] **Step 3: Implement `ui`.**

Add to `crates/hector-cli/src/commands/watch.rs`. Add imports:

```rust
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
```

Implementation:

```rust
fn header_line(state: &ViewState, summary: &LogSummary, clock: &str) -> Line<'static> {
    let (stream_style, explorer_style) = match state.view {
        View::Stream => (Style::default().fg(ORANGE).add_modifier(Modifier::BOLD), Style::default().fg(MUTED)),
        View::Explorer => (Style::default().fg(MUTED), Style::default().fg(ORANGE).add_modifier(Modifier::BOLD)),
    };
    let status_word = if summary.blocks > 0 { "BLOCK" } else { "PASS" };
    Line::from(vec![
        Span::styled("≈ stream", stream_style),
        Span::raw("   "),
        Span::styled("▤ explorer", explorer_style),
        Span::raw("        "),
        Span::styled(format!("{status_word} · {clock}"), Style::default().fg(MUTED)),
    ])
}

fn footer_line(state: &ViewState, summary: &LogSummary, armed: usize) -> Line<'static> {
    match state.view {
        View::Stream => Line::from(Span::styled(
            format!(
                "{armed} checks armed · {} runs · {} blocks        → explorer   q quit",
                summary.runs, summary.blocks
            ),
            Style::default().fg(MUTED),
        )),
        View::Explorer => Line::from(Span::styled(
            "↑↓ select   ↵ open check log        → stream   q quit".to_string(),
            Style::default().fg(MUTED),
        )),
    }
}

/// Draw the full TUI for the current state.
pub fn ui(
    frame: &mut Frame,
    entries: &[LogEntry],
    summary: &LogSummary,
    armed: usize,
    state: &ViewState,
    clock: &str,
) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Min(1),    // body
        Constraint::Length(1), // footer
    ])
    .split(frame.area());

    frame.render_widget(Paragraph::new(header_line(state, summary, clock)), chunks[0]);

    let body = match state.view {
        View::Stream => stream_lines(entries, state.filter.as_deref()),
        View::Explorer => explorer_lines(summary, state.selected),
    };
    frame.render_widget(
        Paragraph::new(body).block(Block::default().borders(Borders::TOP)),
        chunks[1],
    );

    frame.render_widget(Paragraph::new(footer_line(state, summary, armed)), chunks[2]);
}
```

- [ ] **Step 4: Run to verify pass.**

Run: `cargo test -p hector-cli watch::`
Expected: PASS (all watch tests, including the two TestBackend ones). `cargo clippy -p hector-cli --all-targets -- -D warnings` + `cargo fmt` clean.

- [ ] **Step 5: Commit.**

```bash
git add crates/hector-cli/src/commands/watch.rs
git commit -m "feat(cli): add watch ui layout with TestBackend tests"
```

**Phase 3 gate:** request code review from a separate agent before Phase 4.

---

# Phase 4 — Live loop + docs

Wire the tested pieces into a running TUI and document it. The loop is the only uncovered code; verify the file still clears the coverage gate (the pure + TestBackend tests dominate).

### Task 4.1: Event loop + armed-check loading

**Files:**
- Modify: `crates/hector-cli/src/commands/watch.rs`

**Interfaces:**
- Consumes: `hector_core::runner::HectorEngine`, `hector_core::watch::{ArmedCheck, summarize}`, `hector_core::telemetry::read_all`.
- Produces: replaces the `Ok(0)` stub in `run()` with the live loop. New private fns `load_armed(dir) -> Vec<ArmedCheck>`, `run_tui(dir, &[ArmedCheck]) -> Result<()>`, `event_loop<B: Backend>(...) -> Result<()>`.

**Architecture note (divergence from spec §8):** v1 re-reads the whole log via `telemetry::read_all` each tick rather than a seek-based tailer. Same observable behavior, far smaller (and already-tested) surface, and it naturally handles truncation/rotation. Seek-tailing is a noted future optimization.

- [ ] **Step 1: Implement `load_armed` with a unit test.**

Add the test to the `tests` module:

```rust
#[test]
fn load_armed_reads_check_names_and_lifecycles() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".hector.yml"),
        "checks:\n  lint:\n    files: \"*.ts\"\n    run: \"true\"\n    on: [write, pre-commit]\n",
    )
    .unwrap();
    let armed = load_armed(dir.path());
    assert_eq!(armed.len(), 1);
    assert_eq!(armed[0].name, "lint");
    assert_eq!(armed[0].on, vec![hector_core::config::Lifecycle::Write, hector_core::config::Lifecycle::PreCommit]);
}

#[test]
fn load_armed_is_empty_when_config_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load_armed(dir.path()).is_empty());
}
```

Add `tempfile` to `crates/hector-cli`'s `[dev-dependencies]` if not already present (it is — used by other e2e tests).

- [ ] **Step 2: Run to verify failure.**

Run: `cargo test -p hector-cli watch::tests::load_armed_reads_check_names_and_lifecycles`
Expected: FAIL — `load_armed` undefined.

- [ ] **Step 3: Implement `load_armed` (best-effort config load).**

Add to `crates/hector-cli/src/commands/watch.rs`. Add imports:

```rust
use hector_core::runner::HectorEngine;
use hector_core::watch::{summarize, ArmedCheck};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::crossterm::execute;
use ratatui::Terminal;
use std::time::Duration;
```

Function:

```rust
/// Resolve the armed-check projection from `<dir>/.hector.yml`. Best-effort:
/// returns empty on any load error (watch still tails the log).
fn load_armed(dir: &Path) -> Vec<ArmedCheck> {
    let config = dir.join(".hector.yml");
    match HectorEngine::load(&config) {
        Ok(engine) => engine
            .checks()
            .iter()
            .map(|(name, check)| ArmedCheck { name: name.clone(), on: check.on.clone() })
            .collect(),
        Err(_) => Vec::new(),
    }
}
```

- [ ] **Step 4: Run to verify pass.**

Run: `cargo test -p hector-cli watch::tests::load_armed_reads_check_names_and_lifecycles watch::tests::load_armed_is_empty_when_config_missing`
Expected: PASS.

- [ ] **Step 5: Implement the loop + replace the stub (no new test — verified manually).**

In `crates/hector-cli/src/commands/watch.rs`, replace the `// Phase 4 replaces this stub` line and `Ok(0)` in `run()` with:

```rust
    let armed = load_armed(dir);
    run_tui(dir, &armed)?;
    Ok(0)
```

Then add the impure loop (keep it minimal — this is the only uncovered code):

```rust
fn run_tui(dir: &Path, armed: &[ArmedCheck]) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let result = event_loop(&mut terminal, dir, armed);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}

fn event_loop<B: Backend>(terminal: &mut Terminal<B>, dir: &Path, armed: &[ArmedCheck]) -> Result<()> {
    let log = dir.join(".hector/log.jsonl");
    let mut state = ViewState::default();
    loop {
        let entries = hector_core::telemetry::read_all(&log).unwrap_or_default();
        let summary = summarize(&entries, armed);
        let clock = short_time(&chrono::Utc::now().to_rfc3339());
        terminal.draw(|f| ui(f, &entries, &summary, armed.len(), &state, &clock))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press
                    && handle_key(key.code, &mut state, &summary) == Loop::Quit
                {
                    return Ok(());
                }
            }
        }
    }
}
```

- [ ] **Step 6: Build, then verify the binary launches and quits cleanly.**

Run:
```bash
cargo build -p hector-cli
# In a real terminal, from a repo that has a .hector.yml:
./target/debug/hector watch --dir .
```
Expected: the TUI renders (header tabs, body, footer); `Tab`/`→` switches Stream↔Explorer; `↑`/`↓` move the explorer selection; `q` exits and the terminal is restored (no leftover raw mode). If a `.hector/log.jsonl` exists, recent runs appear newest-first within ~250ms.

- [ ] **Step 7: Confirm the coverage gate holds for `watch.rs` (CI).**

The pure helpers + `handle_key` + `stream_lines` + `explorer_lines` + `ui` (TestBackend) + `load_armed` tests, plus the e2e no-TTY branch, cover everything except `run_tui`/`event_loop`. If CI's `ci-coverage.sh` reports `commands/watch.rs` below 80%, shrink the loop: e.g. inline `run_tui` into `run`, or extract any remaining branch (none should exist) into a tested helper. Do not use `COVERAGE_THRESHOLD`.

- [ ] **Step 8: Run the full suite + lints, then clean up.**

Run:
```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo clean -p hector-cli   # drop the debug build artifact from manual verification
```
Expected: all green.

- [ ] **Step 9: Commit.**

```bash
git add crates/hector-cli/src/commands/watch.rs
git commit -m "feat(cli): wire hector watch live event loop"
```

### Task 4.2: Documentation + changelog

**Files:**
- Modify: `docs/reference/cli.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Document the command.**

In `docs/reference/cli.md`, add a `hector watch` section after the existing command entries, in the established house format. Content to include:

```markdown
## `hector watch`

A read-only live TUI over `.hector/log.jsonl`. Run it in a pane beside your
coding agent to watch checks fire in real time.

```
hector watch [--dir DIR]
```

- `--dir DIR` — directory containing `.hector.yml` / `.hector/log.jsonl` (default: cwd).

Two views, toggled with `Tab` / `→` / `←`:

- **Stream** — newest-first feed of check runs: time, ✓/✗/⚠, file (or
  `pre-commit · N files`), elapsed, and the `write`/`commit` event. Blocked rows
  show the failing check and `write rejected`; internal-error rows show the
  reason. The failure *message* is not shown — it isn't stored in the log
  (the agent receives it live via the hook).
- **Explorer** — whole-log aggregate: runs / blocks / internal / pass%, and a
  per-check table ranked by blocks with block-rate and p50 latency. `↑`/`↓`
  select a check; `↵` jumps to the Stream filtered to it.

`q` / `Esc` quits. Requires an interactive terminal; in a non-TTY it exits `1`
with a hint. Read-only: it runs no checks, writes no telemetry, and does not
enforce trust.
```

(Adjust headings/anchors to match the file's existing structure.)

- [ ] **Step 2: Add a changelog entry.**

In `CHANGELOG.md`, under the unreleased/next section, add:

```markdown
- **`hector watch`** — a read-only live TUI over `.hector/log.jsonl` with a
  Stream (newest-first run feed) and an Explorer (per-check health ranked by
  blocks). Built on existing telemetry; no schema change.
```

- [ ] **Step 3: Commit.**

```bash
git add docs/reference/cli.md CHANGELOG.md
git commit -m "docs: document hector watch"
```

**Phase 4 gate:** request final code review from a separate agent.

---

## Self-Review

**Spec coverage:**
- §1 thesis / §2 scope → the whole plan; non-goals (no schema change, no message, read-only, no trust) are in Global Constraints and Task 4.1 `load_armed` (best-effort, no trust).
- §3 data sources → Task 4.1 `load_armed` (config, best-effort) + `event_loop` (`read_all`).
- §4 command surface (`--dir`, TTY requirement) → Task 2.1.
- §5.1 Stream (fields, block/internal sub-lines, colors, newest-first, filter) → Task 3.1.
- §5.2 Explorer (summary bar, ranked table, badges, ⚠, divider, Enter-filter) → Task 3.2 + 2.2.
- §5.3 navigation → Task 2.2 `handle_key`.
- §6 aggregation semantics → Task 1.1 `summarize` (+ tests for each definition).
- §7 formatting helpers → Task 1.2.
- §8 architecture (core/CLI split) → Phase 1 vs Phases 2–4; the §8 seek-tailer is consciously simplified to poll-reread (noted in Task 4.1).
- §9 metadata-only decision → enforced by the "no schema change" constraint and the stream's no-message rows (Task 3.1 test asserts no "exited").
- §10 edge cases → empty log (Task 1.1 test), no/invalid config (Task 4.1 `load_armed` empty), malformed lines (`read_all` drops them), no TTY (Task 2.1).
- §11 testing/coverage → Coverage strategy section + TestBackend tests + e2e.
- §12 deps → Task 2.1 (`ratatui` only).
- §13 futures → out of scope, noted.

**Placeholder scan:** no TBD/TODO; every code step shows full code; test code is concrete.

**Type consistency:** `ArmedCheck`/`CheckRollup`/`LogSummary`/`summarize` defined in Task 1.1 and consumed unchanged in 2.2/3.1/3.2/3.3/4.1; `handle_key`/`ViewState`/`View`/`Loop` defined in 2.2 and used in 3.3/4.1; `stream_lines`/`explorer_lines`/`ui` signatures match between definition and call sites; color consts (`ORANGE`/`GREEN`/`AMBER`/`MUTED`) defined once in 3.1 and reused in 3.2/3.3.
