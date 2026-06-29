# `hector watch` — live TUI over the telemetry log

**Status:** design, approved direction (2026-06-28)
**Builds on:** the checks pipeline (`specs/2026-06-28-hector-checks-pipeline-design.md`) and the telemetry log (`crates/hector-core/src/telemetry.rs`, record-set `SCHEMA_VERSION = 5`).
**Relates to:** `hector review` (the gate-health skill) — both summarize `.hector/log.jsonl`; the aggregation defined here is the shared source of truth so their numbers can't drift.
**Breaking:** none. New read-only subcommand; no schema change; no change to `check`/hook behavior.

## 1. Thesis

Hector enforces synchronously and invisibly: a hook fires `hector check` on each agent edit, the exit code blocks, and the moment passes. The only durable trace is `.hector/log.jsonl` — one `LogEntry::Check` appended per invocation. Today nothing surfaces that stream; you find out hector is working only when an edit is rejected.

`hector watch` is a long-running TUI you keep open in a pane beside your agent. It does two things, both purely as a **viewer** over data hector already writes:

- **Stream** — a live, newest-first tail of check runs as they happen.
- **Explorer** — an aggregate over the whole log: pass/block/error totals and a per-check health table (blocks, block-rate, p50 latency), ranked by blocks.

It adds no enforcement, runs no checks, and changes no schema. It is the read side of the system hector already has.

## 2. Scope & non-goals

**In scope (v1):** the two views above; live tail; view switching; quit; explorer row selection + "jump to stream filtered by this check"; graceful cold-start and degraded states.

**Explicit non-goals:**

- **No schema change.** Watch builds on `LogEntry::Check` / `PerCheckRecord` as they exist at `SCHEMA_VERSION = 5`. We decided *not* to persist block messages or exit codes in the log (the secrets-on-disk and unbounded-growth costs outweigh the benefit; the log stays metadata-only). See §9.
- **No message display.** Because the message isn't in the log, the stream's block row shows *which check rejected which file*, not the failure text. This is honest, not a placeholder. The agent receives the full message live via the hook; the log/watch is the human-facing metadata view.
- **No running of checks.** Watch never re-runs anything. It does not watch the filesystem; its only input is the log (+ config for enrichment).
- **No trust enforcement.** Watch is read-only, like `explain`/`doctor`/`show-resolved-config`. It does not call `trust::ensure_trusted`.
- **Deferred:** free-text filtering (`f`), session/`--since` windowing (v1 is all-time over the whole log), and any v6 telemetry enrichment (§9).

## 3. Data sources

Two inputs, both read-only:

1. **`.hector/log.jsonl`** — the event stream. Each line is one `LogEntry::Check`:
   `ts`, `file: Option<String>` (present on `write`, absent on `pre-commit`), `set_size: Option<usize>` (present on `pre-commit`), `event` (`"write"` / `"pre-commit"`), `status` (`Pass`/`Block`/`InternalError`), `elapsed_ms`, and `checks: Vec<PerCheckRecord>` where each is `{ check, step?, status, elapsed_ms, reason? }`. `reason` carries the `InternalReason` string for crashed checks (and *only* those).
2. **Resolved config** via `HectorEngine::load` (best-effort) — supplies the set of configured checks with their `name` and `on:` lifecycle, used for: the "N checks armed" count, the `[w]`/`[c]`/`[w+c]` lifecycle badges, and listing checks that exist in config but have not yet appeared in the log.

Config load is **best-effort**: if it fails (missing/invalid/mid-edit), watch still tails the log in a degraded mode — no armed count, no lifecycle badges, no zero-run rows — and shows a one-line banner. The log is the load-bearing input; config only enriches.

## 4. Command surface

```
hector watch [--dir DIR]      # project root; defaults to cwd discovery, like other commands
```

Deliberately flagless in v1. The log path is `<root>/.hector/log.jsonl`; the config is resolved the same way the other commands resolve it. (`--since`, `--check`, `--once`/snapshot are noted futures, §10.)

Requires a TTY. With no TTY (CI, piped), exit with a clear message pointing at a future `--once` snapshot mode rather than attempting to drive a terminal.

## 5. Views

### 5.1 Stream (default)

Newest-first list of log entries as they arrive. One row per `LogEntry::Check`:

```
14:23:09  ✗  src/auth.test.ts                         12ms   write
          └ no-focused-tests · write rejected
14:23:04  ✓  src/auth.ts                               38ms   write
14:22:40  ✓  pre-commit · 47 files                     1.2s   commit
14:21:40  ⚠  src/big.ts                               30.0s   write
          └ types-pass · check error: timeout
```

Field mapping (all from the log, no derivation gaps except as noted):

| Column | Source |
|---|---|
| time `14:23:09` | `ts` (rendered HH:MM:SS) |
| glyph `✓`/`✗`/`⚠` | `status` → Pass / Block / InternalError |
| target | `file` for `write`; `pre-commit · {set_size} files` when `file` is absent |
| elapsed | `elapsed_ms` (formatted §7) |
| event badge | `event` (`write` / `commit`) |

**Detail sub-line** (only for non-pass rows), built from the failing `PerCheckRecord`(s) in `checks[]`:

- **Block:** `<check> · write rejected` (or `· commit blocked` for `pre-commit`). No exit code (not stored), no message (not stored, by §9 decision).
- **Internal error:** `<check> · check error: <reason>` — the `reason` field *is* stored, so this one row type can show why (e.g. `timeout`, `exit 127`, `signal`).

If multiple checks failed in one entry, list each on its own sub-line under the row.

Color vocabulary (matches the brand): block = `#ff5c38`, pass `✓` = `#34d399`, internal `⚠` = amber, secondary text (time/elapsed/badge) = `#84848c`, on a near-black `#08080a` ground. The active block row is highlighted with a tinted background + left rule.

**Header:** active tab indicator + the most recent overall status word (`BLOCK`/`PASS`/`ERROR`) + a live clock.
**Footer:** `N checks armed · M runs · K blocks` + `→ explorer  q quit`. `N` from config; `M`/`K` from the log aggregate (§6).

### 5.2 Explorer

Aggregate over the **whole log**, plus config enrichment. Two regions:

**Summary bar:** `log  {runs} runs · {blocks} blocks · {internal} internal error · {pass}% pass`.

**Checks table — ranked by blocks descending**, then by block-rate, then name:

```
CHECKS · RANKED BY BLOCKS                                   rate   p50
● no-focused-tests [w+c]   ████████░░  3                     20%   11ms
● lint            [w]      ██░░░░░░░░  1                      3%   44ms
✓ NO BLOCKS IN LOG
● no-secrets      [w+c]                0                      0%    3ms
● types-pass      [c]  ⚠ 1            0                      0%   240ms
```

Per-check columns:

| Column | Source |
|---|---|
| dot + name | check `name` (red dot if it has blocks, green otherwise) |
| `[w]`/`[c]`/`[w+c]` | config `on:` lifecycle for that check |
| bar + count | that check's block count (PerCheckRecord-level) |
| `⚠ n` | that check's InternalError count |
| `rate` | check blocks ÷ check runs, as % |
| `p50` | median `elapsed_ms` over that check's `PerCheckRecord`s |

Checks with zero blocks fall under a `✓ NO BLOCKS IN LOG` divider. Checks present in config but never seen in the log appear there with `0` / `0%` / `—` p50 (config-only enrichment), so "armed but never fired" is visible.

**Interactions:** `↑/↓` select a row; `Enter` jumps to Stream filtered to the selected check; `f` (free-text filter) is deferred (§10).

### 5.3 Navigation (global)

`Tab` / `→` / `←` switch views; `q` / `Esc` / `Ctrl-C` quit. Terminal resize re-lays out (ratatui handles redraw).

## 6. Aggregation semantics (the one place numbers are defined)

A single pure function in `hector-core` folds `&[LogEntry]` (+ the config check list) into a `LogSummary` view-model. Definitions, stated once to avoid ambiguity between the footer, summary bar, and table:

- **runs** = number of log entries (invocations).
- **blocks** = entries whose `status == Block`.
- **internal errors** = entries whose `status == InternalError`.
- **pass %** = `round(100 × pass_entries / runs)`; `100%` when `runs == 0` is rendered as `—` to avoid a misleading "perfect" on an empty log.
- **per-check run count** = number of `PerCheckRecord`s for that check name across all entries.
- **per-check block count** / **internal count** = those records filtered by status.
- **per-check rate** = `block_count / run_count` (0 when `run_count == 0`).
- **per-check p50** = median of that check's record `elapsed_ms` (lower-median on even counts); `None` → `—` when the check has no records.

(With one blocking check per blocking entry — the common case, and what the mockup shows — entry-level `blocks` equals the sum of per-check block counts. They can diverge if two checks block in one entry; the summary bar is entry-level, the table is per-check, and that's intentional.)

This function is the shared definition `hector review` should also consume, so the live view and the report agree.

## 7. Formatting helpers (pure, tested)

- **elapsed:** `< 1000ms → "{n}ms"`; `< 60s → "{:.1}s"`; else `"{m}m{s}s"`. (`12ms`, `1.2s`, `30.0s`.)
- **glyph + color:** `Pass → ✓ green`, `Block → ✗ #ff5c38`, `InternalError → ⚠ amber`.
- **target label:** `Some(file) → file`; `None → "pre-commit · {set_size} files"`.
- **lifecycle badge:** `[write] → [w]`, `[pre-commit] → [c]`, both → `[w+c]`.

All of these are pure `fn`s with unit tests — they carry the region-coverage weight (§11).

## 8. Architecture

Split logic (testable, in core) from rendering (thin, in CLI):

- **`hector-core`** — new module `watch` (or `log_stats`):
  - `LogSummary` + `CheckRollup` view-model types.
  - `summarize(entries: &[LogEntry], checks: &[Check]) -> LogSummary` — §6 (only `Check::name` + `Check::on` are read; a narrower projection is fine if cleaner).
  - the formatting helpers — §7.
  - All pure, no I/O, no terminal. Fully unit-tested.
- **`hector-cli`** — new `commands/watch.rs` + a clap subcommand:
  - terminal setup/teardown (raw mode, alternate screen) via **crossterm**;
  - a **tailer** that opens `.hector/log.jsonl`, seeks to end, and on each tick reads newly-appended lines, parsing each as `LogEntry` (malformed/partial lines skipped; size-shrink ⇒ reopen from start — handles truncation/rotation);
  - an event loop on a ~250ms tick that (a) ingests new lines, (b) refreshes the clock, (c) handles key events, (d) redraws;
  - **ratatui** widgets that render the current view from the core view-model + a small `ViewState` (active view, selected row, stream filter).

The CLI layer holds only "drive the terminal + map model→widgets"; every value shown is computed in core.

## 9. Why the log stays metadata-only (decision record)

We considered persisting block messages (full or truncated) to power the stream's block row. Rejected for v1:

- **Secrets on disk.** Check stdout/stderr is the one place file *content* would enter the log — a `no-secrets` gate could write the matched secret into `.hector/log.jsonl`, a plaintext file that may be shared or less-guarded than source. The current metadata-only log is a deliberate safety posture.
- **Unbounded growth.** Records are ~200 bytes today; a `tsc`/`eslint` dump is kilobytes. The log has no rotation and watch re-reads it to aggregate, so verbose blocks would bloat the exact hot path.
- **Redundant for fixing.** The agent already gets the full message live via the hook (the live channel is unaffected by this decision). For an agent acting *off the log*, the entry is a sufficient re-run recipe (check + file) for `write` blocks; the message would only save a round-trip.

The residual gaps this leaves (acceptable for v1): `pre-commit` blocks carry no file attribution (hector can't produce one — the check runs once over the whole set and hector sees only the exit code), and non-idempotent checks aren't faithfully reproducible from the log. If these bite, the minimal future addition is an **optional, truncated first-line** `message` + `exit_code` on `PerCheckRecord` behind an opt-in `execution.log_messages` flag — a `SCHEMA_VERSION` 5→6 bump. Out of scope here; recorded so the door is intentional, not accidental.

## 10. Edge cases

- **No log file yet** → cold-start: render armed checks from config with "waiting for edits…"; empty aggregates show `—`.
- **No/invalid config** → degraded tail (§3): log still streams; banner notes config unavailable; no armed count / badges / zero-run rows.
- **Malformed log line** (torn append) → skip that line, keep tailing. (Appends are atomic + `flock`-serialized per telemetry, so this is defensive.)
- **Log truncated/rotated** (size shrinks) → reopen from start, rebuild aggregate.
- **Empty log** → `0 runs`, pass% rendered `—`.
- **No TTY** → exit with a message (no terminal to drive); points at the future `--once` snapshot.

## 11. Testing & the coverage gate

The repo enforces ≥80% **region** coverage per file. TUI draw code and the raw event loop are hard to cover, so the design front-loads logic into pure functions:

- **Core (`watch` module)** — unit tests for `summarize` (empty log; all-pass; mixed blocks; internal errors via `reason`; per-check rate; p50 odd/even counts; merging config checks with zero runs; rank order: blocks desc → rate → name) and every formatting helper (elapsed boundaries 999/1000ms and 60s; each status glyph/color; file vs. `set_size` label; each lifecycle badge). This module carries its own ≥80% comfortably.
- **CLI (`watch.rs`)** — keep it thin. Extract "view-model + `ViewState` → widget tree" and the tailer's line-ingest/rotation logic into functions testable without a live terminal (feed synthetic bytes, assert parsed entries / produced rows). The irreducible terminal-init + event-loop shell is the only part that may need a documented `#[allow]`; minimize its size so the file still clears the gate. Flag in the plan if it can't.
- **E2E** — an `assert_cmd` smoke test that `hector watch` in a non-TTY exits with the expected message (covers the entry point without driving a terminal).

## 12. Dependencies

New CLI-crate deps: **`ratatui`** + **`crossterm`** (ratatui's default backend). No filesystem-watch crate — tailing is poll+seek on the local append-only log, which keeps the dependency surface small and avoids `notify`'s platform variance.

## 13. Future (not v1)

- `--once` — render one snapshot to stdout and exit (CI/non-TTY; also a clean E2E surface).
- `--since <dur>` / session windowing — aggregate a recent window instead of all-time.
- `f` free-text filter in explorer; richer stream filtering.
- Optional truncated-message telemetry (`execution.log_messages`, schema v6) per §9, to make `pre-commit` block attribution and flaky-check faithfulness possible.
- Shared aggregation adoption by `hector review`.
