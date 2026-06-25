# Telemetry — `.hector/log.jsonl`

Hector appends one JSON record per line to `.hector/log.jsonl` for every check it performs. The file is owner-only (`0o600`) and append-only — Hector never rewrites or truncates it. Operators rotate it themselves; downstream tools (dashboards, log greppers) read it line by line.

**Schema version:** `3`. This is a code constant (`telemetry::SCHEMA_VERSION`) that bumps when the record shape changes. It is **not** written into each line — there is no per-line version field.

## Discriminator

Every line carries a `type` field. The only value is `check`. Field names are `snake_case`.

## `check`

Written once per `hector check` call against a single file. It carries the verdict status, wall-clock elapsed, and a per-gate outcome list.

```jsonl
{"type":"check","ts":"2026-06-15T12:00:01Z","file":"src/app.ts","status":"block","elapsed_ms":42,"gates":[{"gate":"no-console","status":"block","elapsed_ms":12},{"gate":"biome","status":"pass","elapsed_ms":28}]}
{"type":"check","ts":"2026-06-15T12:00:02Z","file":"README.md","status":"pass","elapsed_ms":1,"gates":[]}
```

| Field | Type | Description |
|---|---|---|
| `type` | `"check"` | Record discriminator. |
| `ts` | RFC3339 string | Wall-clock at the time the line was written. |
| `file` | string | Path to the file checked. |
| `status` | `"pass"` \| `"block"` \| `"internal_error"` | Verdict status (matches `verdict.status`). |
| `elapsed_ms` | integer | Wall-clock for the whole check. |
| `gates` | array of per-gate records | One entry per gate that ran against the file. **Empty** when no gate's `files` matched (the file was checked, but no gate ran). |

### Per-gate record

| Field | Type | Description |
|---|---|---|
| `gate` | string | Gate id from `.hector.yml`. |
| `status` | `"pass"` \| `"block"` \| `"internal_error"` | Outcome of this gate on this file. |
| `elapsed_ms` | integer | Wall-clock for this gate's run. |
| `reason` | string, optional | Why the gate crashed. Omitted on a plain pass or block; on an `internal_error` it's a stable string — `timeout`, `not_found`, `not_executable`, `signal:9`, `exit_code:137`. |

There is no warn status at either level, no `engine` field, and no `rule_id` — a gate owns its verdict through its exit code, and Hector logs the outcome it observed.

## Atomicity and concurrency

`telemetry::append` opens the file with `O_APPEND` and owner-only mode (`0o600`), takes an advisory `flock(LOCK_EX)`, writes one buffered line in a single `write_all`, then releases the lock. Concurrent `hector` invocations cannot interleave bytes: the kernel's `O_APPEND` atomicity covers writes below `PIPE_BUF`, and the `flock` covers larger lines.

## Rotation

Hector does not rotate `.hector/log.jsonl` itself — operators handle rotation. The append-only contract means external rotation (e.g. `logrotate copytruncate`) is safe: a missing-or-empty file is silently re-created on the next append.
