---
name: hector-review
description: Reviews hector gate health from the telemetry log. Use when the user says "review my hector gates", "check gate health", "which hector gates are noisy", "find dead hector gates", "hector review", or asks for an audit of .hector.yml.
metadata:
  author: dynamik-dev
  version: 0.2.0
  category: workflow-automation
  tags: [linting, telemetry, gate-pruning]
---

# Hector Review

Audit the gate set against telemetry. Surface candidates for removal, scope
adjustment, or a source fix.

## Source of truth

`.hector/log.jsonl` — one record per check invocation, with a **per-gate**
breakdown. Each line:

```json
{
  "type": "check",
  "ts": "2026-06-15T00:00:00Z",
  "file": "src/foo.rs",
  "status": "block",
  "elapsed_ms": 42,
  "gates": [
    {"gate": "no-debug", "status": "block", "elapsed_ms": 30},
    {"gate": "no-todo",  "status": "pass",  "elapsed_ms": 12}
  ]
}
```

- The top-level `status` is the most severe of the gates that ran (`block` >
  `internal_error` > `pass`). There is no `warn` tier.
- `gates[]` attributes the outcome to each gate by id, so you can recommend on
  **specific gates**, not just files. `gates` is empty when no gate matched the file.
- A gate with `status: "internal_error"` carries a `reason` (timeout, not_found,
  …) — it *couldn't run*, which is a broken gate, not a finding.

## Process

1. Read `.hector/log.jsonl`.
2. Aggregate over the last N entries (default last 1000, or all if fewer),
   grouping by `gates[].gate` (and cross-referencing `file`).
3. Surface concerning patterns per gate:
   - **High block rate** (>50% of the files a gate ran on): the gate may be too
     strict, or the code it covers genuinely needs fixing at the source.
   - **Zero blocks across many runs**: the gate may be dead — its `files` scope
     never matches anything dirty, or it never fires. Confirm it still earns its keep.
   - **Recurring `internal_error`**: the gate is broken (read its `reason`) — fix
     or remove it; a gate that can't run protects nothing.
   - **Slow gates** (high `elapsed_ms`): flag for optimization or a narrower scope.

## Recommendations

For each concerning gate, propose ONE of:
- **Investigate source**: a high block rate may be a real codebase problem to fix
  in code, not in the gate.
- **Tighten the `files` scope**: narrow the glob so a noisy gate fires only where
  it should.
- **Remove the gate**: a gate that never blocks (and isn't meant as a tripwire)
  is noise in the config.
- **Fix a broken gate**: for recurring `internal_error`, repair the `run` command
  or the tool it shells out to.

Never apply recommendations silently. Present each one and ask the user. To
re-confirm what a gate does on a file, run `hector check --file <path> --gate
<id> --format json` and read the verdict.

## Output format

```
Reviewed N entries from .hector/log.jsonl (date range A → B).

Per-gate health:

| Gate            | Runs | pass | block | error | Note / recommendation                                  |
|-----------------|------|------|-------|-------|--------------------------------------------------------|
| no-debug        | 31   | 24   | 7     | 0     | High block rate (23%) — investigate src/api or tighten |
| no-todo         | 84   | 84   | 0     | 0     | No blocks — confirm it still earns its keep            |
| eslint-check    | 12   | 9    | 0     | 3     | Broken (reason: not_found) — eslint missing on PATH    |

To re-confirm a gate on a file:
  hector check --file <path> --gate <id> --format json
```

## Notes

- Records are per check invocation with a per-gate breakdown; group by
  `gates[].gate` to attribute outcomes to a specific gate.
- Statuses are `pass`, `block`, and `internal_error` — there is no `warn`.
