---
name: hector-review
description: Reviews hector check health from the telemetry log. Use when the user says "review my hector checks", "check health", "which hector checks are noisy", "find dead hector checks", "hector review", or asks for an audit of .hector.yml.
metadata:
  author: dynamik-dev
  version: 0.2.0
  category: workflow-automation
  tags: [linting, telemetry, check-pruning]
---

# Hector Review

Audit the check set against telemetry. Surface candidates for removal, scope
adjustment, or a source fix.

## Source of truth

`.hector/log.jsonl` — one record per check invocation, with a **per-check**
breakdown. Each line:

```json
{
  "type": "check",
  "ts": "2026-06-15T00:00:00Z",
  "file": "src/foo.rs",
  "status": "block",
  "elapsed_ms": 42,
  "checks": [
    {"check": "no-debug", "status": "block", "elapsed_ms": 30},
    {"check": "no-todo",  "status": "pass",  "elapsed_ms": 12}
  ]
}
```

- The top-level `status` is the most severe of the checks that ran (`block` >
  `internal_error` > `pass`). There is no `warn` tier.
- `checks[]` attributes the outcome to each check by id, so you can recommend on
  **specific checks**, not just files. `checks` is empty when no check matched the file.
- A check with `status: "internal_error"` carries a `reason` (timeout, not_found,
  …) — it *couldn't run*, which is a broken check, not a finding.

## Process

1. Read `.hector/log.jsonl`.
2. Aggregate over the last N entries (default last 1000, or all if fewer),
   grouping by `checks[].check` (and cross-referencing `file`).
3. Surface concerning patterns per check:
   - **High block rate** (>50% of the files a check ran on): the check may be too
     strict, or the code it covers genuinely needs fixing at the source.
   - **Zero blocks across many runs**: the check may be dead — its `files` scope
     never matches anything dirty, or it never fires. Confirm it still earns its keep.
   - **Recurring `internal_error`**: the check is broken (read its `reason`) — fix
     or remove it; a check that can't run protects nothing.
   - **Slow checks** (high `elapsed_ms`): flag for optimization or a narrower scope.

## Recommendations

For each concerning check, propose ONE of:
- **Investigate source**: a high block rate may be a real codebase problem to fix
  in code, not in the check.
- **Tighten the `files` scope**: narrow the glob so a noisy check fires only where
  it should.
- **Remove the check**: a check that never blocks (and isn't meant as a tripwire)
  is noise in the config.
- **Fix a broken check**: for recurring `internal_error`, repair the `run` command
  or the tool it shells out to.

Never apply recommendations silently. Present each one and ask the user. To
re-confirm what a check does on a file, run `hector check --file <path> --check
<id> --format json` and read the verdict.

## Output format

```
Reviewed N entries from .hector/log.jsonl (date range A → B).

Per-check health:

| Check           | Runs | pass | block | error | Note / recommendation                                  |
|-----------------|------|------|-------|-------|--------------------------------------------------------|
| no-debug        | 31   | 24   | 7     | 0     | High block rate (23%) — investigate src/api or tighten |
| no-todo         | 84   | 84   | 0     | 0     | No blocks — confirm it still earns its keep            |
| eslint-check    | 12   | 9    | 0     | 3     | Broken (reason: not_found) — eslint missing on PATH    |

To re-confirm a check on a file:
  hector check --file <path> --check <id> --format json
```

## Notes

- Records are per check invocation with a per-check breakdown; group by
  `checks[].check` to attribute outcomes to a specific check.
- Statuses are `pass`, `block`, and `internal_error` — there is no `warn`.
