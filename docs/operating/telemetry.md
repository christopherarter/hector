# Telemetry — `.hector/log.jsonl`

Hector appends one JSON record per line to `.hector/log.jsonl` for every check it performs. The file is owner-only (`0o600`) and append-only — Hector never rewrites or truncates it. Operators rotate it themselves; downstream tools (dashboards, log greppers) read it line-by-line.

**Schema version:** `2`. Bumps when the record shape changes (added or removed fields).

**Compatibility:** the current reader (`hector_core::telemetry::read_all`) also accepts older flat-format lines (`{ "timestamp": ..., "kind": ..., ... }`) and lines from earlier typed schemas, emitting a one-time stderr deprecation warning. Every legacy line collapses into the single `check` record described below. The legacy reader is removed at the 0.3 verdict freeze.

## Discriminator

Every record carries a `type` field. The only value is `check`. Field names are `snake_case`.

---

## `check`

Written once per `hector check` call against a single file. Carries the verdict status, wall-clock elapsed, and a per-rule outcome list.

A check whose `rules` array is **empty** indicates one of two scenarios:
1. The file matched an A2 skip pattern (`Cargo.lock`, `node_modules/`, etc.); no rule ran.
2. Legacy upgrade path: an older line was lifted into this shape because the flat format never carried per-rule detail.

```json
{
  "type": "check",
  "ts": "2026-05-13T12:00:01Z",
  "file": "src/lib.rs",
  "status": "warn",
  "elapsed_ms": 42,
  "rules": [
    {
      "rule_id": "no-unwrap",
      "engine": "ast",
      "status": "pass",
      "elapsed_ms": 30
    },
    {
      "rule_id": "no-todo",
      "engine": "script",
      "status": "warn",
      "elapsed_ms": 4
    }
  ]
}
```

| Field | Type | Description |
|---|---|---|
| `type` | `"check"` | Record discriminator. |
| `ts` | RFC3339 string | Wall-clock at the time the record was written. |
| `file` | string | Path to the file checked. |
| `status` | `"pass"` \| `"warn"` \| `"block"` | Verdict status (matches `verdict.status`). |
| `elapsed_ms` | integer | Wall-clock for the whole check, including dispatch and baseline filter. |
| `rules[]` | array of `PerRuleRecord` | One entry per rule that reached engine dispatch. Empty when an A2 skip pattern matched. |

**`PerRuleRecord`:**

| Field | Type | Description |
|---|---|---|
| `rule_id` | string | Rule id from `.hector.yml`. |
| `engine` | `"script"` \| `"ast"` \| `"trust"` \| `"internal"` | Engine that evaluated the rule. |
| `status` | `"pass"` \| `"warn"` \| `"block"` | Pass for clean evaluations and disable-suppressed; warn/block follows the rule's `severity` if it fired. |
| `elapsed_ms` | integer | Wall-clock for this rule's dispatch. |
| `reason` | string, optional | `"engine_error"` for runtime failures, `"disabled"` for `hector-disable:`-suppressed rows. Omitted when there's nothing to say. |

---

## Schema versioning policy

`SCHEMA_VERSION` bumps **only** on:

- field removals or type changes,
- enum variant removals,
- semantic re-interpretations of existing fields.

Additive changes (new optional field with `skip_serializing_if`, new enum variant marked `#[non_exhaustive]`) do **not** bump.

Consumers wanting backward compatibility should read `MIN_REQUIRED_SCHEMA_VERSION` and accept anything `>=`. Keeping additive changes off the version number means a strict consumer pinned to a version isn't broken by a wire-compatible addition.

---

## Atomicity and concurrency

`telemetry::append` opens with `O_APPEND`, takes an advisory `flock(LOCK_EX)`, writes one buffered line in a single `write_all`, then releases the lock. Concurrent `hector` invocations (e.g. parallel rules in a future B1 work-stealing pool) cannot interleave bytes. The kernel's `O_APPEND` atomicity guarantee covers writes below `PIPE_BUF`; the `flock` covers larger lines.

## Rotation

Hector does not rotate `.hector/log.jsonl` itself. Operators handle rotation. The append-only contract means external rotation (e.g. `logrotate copytruncate`) is safe — a missing-or-empty file is silently re-created on the next append.
