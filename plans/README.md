# Plans

Implementation plans Hector is built from. Each plan is a self-contained, multi-step, checkbox-tracked design + execution doc following the `superpowers:writing-plans` format. Plans are executed via `superpowers:executing-plans` (inline) or `superpowers:subagent-driven-development` (subagent-per-task).

A plan owns its own progress via its checkboxes — that's the source of truth. This README is a navigation surface; keep it in sync by hand when state changes meaningfully (new plan added, plan completed, priority shifted). The `Future` section below is the closest thing to a backlog: short bullets for work that hasn't graduated to a plan file yet.

**Layout:**

- `plans/*.md` — in-flight or queued
- `plans/archive/*.md` — completed (frozen design records, useful as "how was X built" context)

## Active

The 0.2.0 bully-parity cohort (A1 prompt-injection, A2 skip patterns, A3 diff pre-filter) shipped, then B1 (parallel rule execution), E1 (baseline checksum), and C4 (`--rule` / `--explain` / `--print-prompt`) followed it. Four queued plans below cover the next batch from [`specs/2026-05-12-bully-parity-closures.md`](../specs/2026-05-12-bully-parity-closures.md).

- [`2026-05-13-hector-c1-doctor`](2026-05-13-hector-c1-doctor.md) — `hector doctor` diagnostic subcommand. **0.2.0 release blocker.**
- [`2026-05-13-hector-d1-typed-telemetry`](2026-05-13-hector-d1-typed-telemetry.md) — `serde(tag = "type")` enum for `LogEntry`; back-compat reader for the deprecation window. Foundation for D2/D3.
- [`2026-05-13-hector-c2-explain-guide`](2026-05-13-hector-c2-explain-guide.md) — `hector explain <file>` and `hector guide <file>` read-only inspection commands.
- [`2026-05-13-hector-c3-show-resolved-config`](2026-05-13-hector-c3-show-resolved-config.md) — `hector show-resolved-config` (TSV / YAML / JSON) with per-rule origin tracking.

## Future

Ideas that haven't graduated to plans. When something here has enough definition to write a plan against, lift it into a dated plan file.

- _(nothing queued yet — add bullets here as the next cohort takes shape)_

## Archive

Completed plans live in [`archive/`](archive/). They're frozen design records.

- [`2026-05-11-hector-0.1a-foundation`](archive/2026-05-11-hector-0.1a-foundation.md) — workspace skeleton, script engine, trust gate.
- [`2026-05-11-hector-0.1b-engines`](archive/2026-05-11-hector-0.1b-engines.md) — `ast`, `semantic`, `session` engines + `init` / `migrate` / `baseline` / `session record` commands.
- [`2026-05-11-hector-0.1c-claude-code-adapter`](archive/2026-05-11-hector-0.1c-claude-code-adapter.md) — Claude Code adapter (`plugin.json`, PostToolUse + Stop hooks, ported skills).
- [`2026-05-12-hector-opencode-adapter`](archive/2026-05-12-hector-opencode-adapter.md) — OpenCode adapter at parity with the Claude Code adapter.
- [`2026-05-12-bug-audit-remediation`](archive/2026-05-12-bug-audit-remediation.md) — remediation campaign for the [P0/P1/P2 findings](../docs/audits/2026-05-12-bug-audit.md) from the 2026-05-12 audit.
- [`2026-05-12-hector-a1-prompt-injection`](archive/2026-05-12-hector-a1-prompt-injection.md) — `<TRUSTED_POLICY>` / `<UNTRUSTED_EVIDENCE>` sentinel boundary in semantic prompt.
- [`2026-05-12-hector-a2-skip-patterns`](archive/2026-05-12-hector-a2-skip-patterns.md) — built-in skip patterns + project `skip:` + `~/.hector-ignore`.
- [`2026-05-12-hector-a3-diff-prefilter`](archive/2026-05-12-hector-a3-diff-prefilter.md) — local `can_match_diff` short-circuit for `engine: semantic`; new `reason` field on telemetry; runner-side wiring.
- [`2026-05-12-hector-e1-baseline-checksum`](archive/2026-05-12-hector-e1-baseline-checksum.md) — `line_sha256` fingerprinting in `Baseline`; v1-format read tolerance; new `hector baseline refresh` subcommand.
- [`2026-05-12-hector-b1-parallel-rules`](archive/2026-05-12-hector-b1-parallel-rules.md) — rayon-driven parallel rule dispatch in `HectorEngine::check`; `execution.max_workers` config + `HECTOR_MAX_WORKERS` env override; deterministic output order via BTreeMap iteration.
- [`2026-05-12-hector-c4-check-flags`](archive/2026-05-12-hector-c4-check-flags.md) — `hector check --rule <id>` (repeatable) restricts evaluation upstream of the parallel pool; `--explain` prints a per-rule outcome report to stderr; `--print-prompt` renders the semantic prompt without dispatching to the LLM.

## Conventions

- One feature/refactor per plan file. Filename: `YYYY-MM-DD-<short-slug>.md`.
- Plan header carries Goal, Architecture, Tech Stack, and (when relevant) Severity + Sequencing. See `superpowers:writing-plans` for the canonical format.
- Tasks are bite-sized (2–5 min) and checkbox-tracked. The TDD cycle (write failing test → verify red → minimal impl → verify green → commit) is the per-task default per repo rules.
- Substantive work — multi-file, multi-phase, architectural — earns a plan. A 3-line "fix X" doesn't; just commit it.
- When a plan ships, `git mv` it to `archive/` and strike its row from the table above.
