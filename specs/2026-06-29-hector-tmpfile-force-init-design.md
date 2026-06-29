# Hector — `$HECTOR_TMPFILE`, `--force`, and stack-agnostic `init`

**Status:** design, approved direction (2026-06-29)
**Builds on:** the 0.4 checks pipeline (`specs/2026-06-28-hector-checks-pipeline-design.md`) and init onboarding (`specs/2026-06-27-hector-init-onboarding-design.md`)
**Breaking:** none. `$HECTOR_TMPFILE` is additive to the ABI (no `SCHEMA_VERSION` bump); `--force` is a new opt-in flag; the `init` change alters generated *config* output only (no on-disk format change).

## 1. Thesis — close the three friction points field-testing surfaced

Field feedback from an agent (deepseek) authoring real checks against an Astro/Biome project graded the model highly once understood, but named three recurring frictions. Each maps to one change here:

1. **File-oriented tools can't see proposed content without scaffolding.** On a `write` event the proposed bytes are on **stdin**, but linters/formatters/type-checkers (Biome, ESLint file-mode, `tsc`, ruff) want a real path on disk with the right extension, resolved against the nearest config. Authors hand-rolled `mktemp` inside the workspace, copied stdin in, ran the tool, and cleaned up — rediscovering the same temp-file dance, plus its sub-traps (dot-prefixed temp files silently ignored by Biome; `/tmp` outside the workspace refused; wrong extension → wrong language). **Fix: `$HECTOR_TMPFILE`.**

2. **Ad-hoc testing a check against an out-of-scope fixture is awkward.** `hector check --file fixtures/Bad.astro` reports `skipped out_of_scope` when the `files` glob is `src/**/*.astro`. The only workaround was faking a matching path with `--content -`. **Fix: `--force`.**

3. **`init` ships toolchain-specific templates that go stale.** The generated Biome check uses `biome check --stdin-file-path=… -`, which is broken in Biome ≥2.5 ("The contents aren't fixed"); the ruff/ESLint wrappers carry similar version risk. Hector's whole thesis is that it *knows nothing about any tool* — but `init` violated that by emitting tool-specific scaffolds. **Fix: stack-agnostic `init`.**

The unifying principle: hector owns portable plumbing; scripts own tool behavior. `$HECTOR_TMPFILE` is more plumbing (a reliable on-disk view of proposed content); the `init` change stops shipping tool knowledge hector shouldn't own.

## 2. `$HECTOR_TMPFILE` — write-event materialized temp file

### 2.1 Behavior

On a **`write`** event, when a check's `run` (or any `step.run`) text references the token `HECTOR_TMPFILE`, hector:

1. writes the proposed content — the exact bytes also fed to stdin — to a temp file,
2. located **in the same directory as `$HECTOR_FILE`**,
3. named `hector-tmp-<unique>.<ext>`, where `<ext>` mirrors the target file's final extension and the name is **not** dot-prefixed,
4. exports its **absolute** path as `$HECTOR_TMPFILE`,
5. runs the check's steps,
6. deletes the file via an RAII guard.

Same-directory placement is the crux: language detection keys on the extension, and config resolution (`tsconfig.json`, `biome.json`, `.eslintrc`, `pyproject.toml`) walks **up** from the file's location. A sibling temp file resolves to the identical config set as the real edit would. The two rejected placements — repo root and `/tmp` — both reintroduce the misdetection/refusal class for nested files.

### 2.2 Lazy creation

If no `run`/`step.run` text in the check contains the substring `HECTOR_TMPFILE`, **nothing is created** — no file, no env var, no I/O. stdin/grep checks (the common case) pay nothing and cause zero source-tree churn. Detection is a plain substring scan over the union of `run` and every `step.run`; the rare false-positive (the literal string in a comment) only causes a harmless unused materialization.

### 2.3 Pre-commit

`$HECTOR_TMPFILE` is **unset** on `pre-commit`. The files are already on disk and reachable via `$HECTOR_FILES`; there is no single primary target and no stdin. The temp-file problem is write-specific by construction. A check that wants to run at both lifecycles reads stdin/`$HECTOR_TMPFILE` on write and `$HECTOR_FILES` on pre-commit, branching on `$HECTOR_EVENT` only if it genuinely must.

### 2.4 Lifetime & cleanup

The guard removes the temp file on every exit path of the check invocation: normal return, internal error, wall-clock timeout (we delete after killing the child), and panic-unwind (`Drop` runs during unwinding). The only leak path is `SIGKILL` of hector itself mid-check — identical to any temp-file scheme, and documented. The file is created once per check invocation and is visible to every step of that check (shared, not per-step).

### 2.5 Code shape

- `GateEnv` (`engine/gate.rs`) gains `tmpfile: Option<&Path>`. `run_gate` sets `HECTOR_TMPFILE` when `Some`. No other gate logic changes; stdin is still fed exactly as today.
- `run_one_check` (`runner.rs`) performs the reference scan; if it fires (write event, content present, token referenced), it writes the temp file, constructs an RAII guard owning the path, threads the path into `GateEnv`, and holds the guard across `run_steps`.
- The unique component avoids an `rng` dependency: derive from `std::process::id()` + a process-static `AtomicU64` counter + `SystemTime` nanos. Collision-resistant within and across concurrent invocations; the spec does not require cryptographic randomness.
- The CLI `--content` path (`commands/check.rs::run_file`) flows through the same `run_one_check`, so `hector check --file X.astro --content - --check biome-check` materializes `$HECTOR_TMPFILE` — checks are testable without a live hook.

### 2.6 Stability surface

Additive only. Existing checks that never mention `HECTOR_TMPFILE` are byte-for-byte unaffected. The verdict/telemetry JSON shapes do not change; **`SCHEMA_VERSION` is not bumped**. The ABI documentation (§5) gains one row.

### 2.7 Known limitation (accepted, documented)

The temp file has a synthetic name (`hector-tmp-<unique>.<ext>`), so tool configuration keyed on **filename globs** — e.g. ESLint `overrides` for `*.test.ts`, or per-file `ignore` lists — may not match the temp file even though it would match the real edit. Extension-based language detection and nearest-config resolution **do** work. The alternative (overwriting the real file in place) is strictly worse — it mutates the user's working tree and races the agent. We accept the limitation and call it out in the ABI docs.

## 3. `--force` — scope-only test bypass

### 3.1 Behavior

`hector check` gains a `--force` flag. When set **with one or more `--check <id>`**, the named checks run against `--file` even if the path does not match their `files` glob — `--force` suppresses exactly the `out_of_scope` skip and nothing else:

- the check-id filter still applies (only named checks run);
- inline `hector-disable: <id>` directives are still honored;
- `on:` lifecycle is still enforced (control it with `--event`, not `--force`).

`--explain` then reports the check as `fire` rather than `skipped out_of_scope`.

### 3.2 Guard rails

`--force` **requires** `--check`. Without it the flag would force *every* check against a mismatched file (e.g. a Python ruff check against an `.astro` file → vacuous pass or error noise), which is never the intent. Enforced two ways: clap `requires = "checks"`, plus an explicit error message if `--force` is passed without `--check` (so the failure reads clearly rather than as a generic clap usage dump).

### 3.3 Scope of effect

`--force` affects the per-file evaluation path (`skip_reason`, `runner.rs`). It does **not** alter `pre-commit` run-once set-matching (`check_set`) — there, scope *is* the staged-set intersection and bypassing it has no coherent meaning. This keeps the flag's semantics to a single sentence.

### 3.4 Code shape

- `CheckOptions` gains `force: bool`.
- `skip_reason` takes `force` into account: when `force` is set and `check_id` is in `options.checks`, the `out_of_scope` branch returns `None` instead of `Some("out_of_scope")`. Ordering relative to `filtered`/`disabled`/`event` is unchanged.
- `commands/check.rs::run` plumbs the flag into `CheckOptions` and performs the `--force`-without-`--check` validation.

## 4. Stack-agnostic `init`

### 4.1 What is removed

The toolchain-specific config assemblers and the detection that *only* feeds them:

- `emit_rust_gates` (the `no-unwrap` grep is Rust-specific; the clippy breadcrumb),
- `emit_node_gates` (Biome / ESLint wrappers, the `no-console-log` grep),
- `emit_python_gates` (the ruff wrapper),
- the `LinterSet` / `Workspace` / `JsRunner` plumbing and `scope_list_with_default`, to the extent they exist solely to parameterize the above,
- the per-stack assertions in `cli_init.rs`, and now-dead stack branches in `detect.rs`.

### 4.2 What `init` emits instead

A single baseline, identical regardless of manifest:

- **Two universal checks**, both default lifecycle (`on: [write]`), reading **stdin**:
  - `no-fixme` — blocks when the proposed content contains `FIXME`.
  - `no-merge-markers` — blocks on git conflict markers, conservatively matched (`^<<<<<<< ` / `^=======$` / `^>>>>>>> `) to limit false positives on prose dividers.
- **A commented, copy-paste-ready block** demonstrating the two real authoring patterns:
  - a stdin/grep check (the shape above), and
  - a file-oriented linter invoked via `$HECTOR_TMPFILE` (e.g. `<tool> check "$HECTOR_TMPFILE"`), with a one-line note that this is how to wrap Biome/ESLint/ruff/tsc without hector knowing the tool.

The grep-from-stdin form aligns generated config with the config-authoring skill's existing convention (the `be1b7a4` sweep) and removes the latent inconsistency where `init` emitted `grep … "$HECTOR_FILE"` while the skill taught stdin.

### 4.3 What is untouched

**Harness onboarding** — detecting claude-code / pi / opencode / reasonix and installing hector's hook — is unchanged. That detection is orthogonal to manifest/stack detection; only the manifest→checks scaffolding path is removed. `--harness`, `--global`, `--yes`, `--no-hook`, `--hook-only`, `--uninstall`, `--dry-run` all behave as before.

## 5. ABI documentation updates

The check ABI gains one variable; the write/pre-commit stdin consequence the tester hit gets stated explicitly. Update every place the ABI is enumerated:

| Variable | Set when | Value |
| --- | --- | --- |
| `$HECTOR_TMPFILE` | **`write`** only, and only if the check's `run`/`steps` reference it | Absolute path to a temp file (sibling of `$HECTOR_FILE`, same extension) holding the proposed content; auto-removed after the check |

Touch points: `AGENTS.md` (ABI section), the hector-config authoring skill, the `hector schema` guide text, and `docs/` if it mirrors the ABI. Add a sentence to the lifecycle docs: *on `write`, proposed content is on stdin (and, if referenced, in `$HECTOR_TMPFILE`); on `pre-commit`, stdin is empty and content lives on disk at the `$HECTOR_FILES` paths — a check that reads stdin will see nothing at pre-commit.*

## 6. Testing

Per repo rule, each feature lands behind a failing test first; coverage stays ≥80% region per touched file and cognitive complexity ≤15.

- **`$HECTOR_TMPFILE`** (gate + runner + e2e):
  - referenced check on `write` → `$HECTOR_TMPFILE` exists, has proposed content, correct extension, lives beside `$HECTOR_FILE`;
  - the temp file is **gone** after the run (pass, block, and timeout paths);
  - unreferenced check → no temp file created (assert the directory is unchanged);
  - `pre-commit` → `$HECTOR_TMPFILE` unset;
  - CLI: `hector check --file … --content - --check …` materializes it.
- **`--force`**: out-of-scope file + `--check id --force` → check fires (verdict reflects it, `--explain` shows `fire`); `--force` without `--check` → exits 1 with the explicit error; `--force` does not bypass `disabled`/`event`.
- **`init`**: generated config contains `no-fixme` and `no-merge-markers` and the `$HECTOR_TMPFILE` example; it contains **no** `biome`, `eslint`, or `ruff` string for any detected stack; harness onboarding output is unchanged.

## 7. Out of scope (YAGNI)

- Staged-blob materialization for pre-commit (`git show :path` into a temp file) — larger feature, changes pre-commit's run-once shape; deferred.
- `--force` reaching pre-commit set-matching — no coherent meaning (§3.3).
- Auto-`.gitignore` of the temp file — it exists only for the check's duration; not worth the config write.
- Preserving the original basename (temp subdirectory) to satisfy filename-glob overrides — added churn and dot-dir ignore risk outweigh the edge case (§2.7).
