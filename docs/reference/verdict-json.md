# Verdict JSON

`hector check --format json` prints a `Verdict` — the machine-readable result a CI job or adapter consumes. This page is the contract for that shape and the exit codes that go with it.

```json
{
  "schema_version": 4,
  "hector_version": "0.3.0",
  "status": "block",
  "blocks": [
    {
      "gate": "no-console",
      "file": "src/app.ts",
      "message": "src/app.ts:12: console.log('debug')"
    }
  ],
  "errors": [],
  "passed": ["biome"],
  "elapsed_ms": 42
}
```

## Top-level fields

| Field | Type | Notes |
|-------|------|-------|
| `schema_version` | integer | Currently `4`. See [Versioning](#versioning). |
| `hector_version` | string | Version of the binary that produced the verdict. |
| `status` | enum | `pass`, `block`, or `internal_error`. |
| `blocks` | array | One [Block](#block) per gate that exited `2`. Empty on a clean pass. |
| `errors` | array | One [GateError](#gateerror) per gate that crashed. Empty when every gate ran to a verdict. |
| `passed` | array of strings | Gate ids that ran and passed. |
| `elapsed_ms` | integer | Wall-clock for the whole check. |

There is no `violations` array, no `severity`, no `line`/`column`, and no warn status. A gate owns its verdict through its exit code; Hector records the outcome, not a parsed finding.

## `status`

| Value | Meaning |
|-------|---------|
| `pass` | No gate blocked and none crashed. |
| `block` | At least one gate exited `2`. |
| `internal_error` | No gate blocked, but at least one crashed (not found, not executable, timeout, or killed by signal). |

`block` wins over `internal_error`: a confirmed policy violation stops the edit even if an unrelated gate crashed in the same run.

## Block

A gate that exited `2` on a file.

| Field | Type | Notes |
|-------|------|-------|
| `gate` | string | The gate id that blocked. |
| `file` | string | Path the gate blocked on. |
| `message` | string | The gate's combined stdout and stderr, trimmed and passed through verbatim. When both streams are empty, `"<gate-id> blocked"`. |

## GateError

A gate that failed to run to a verdict.

| Field | Type | Notes |
|-------|------|-------|
| `gate` | string | The gate id that crashed. |
| `file` | string | Path it was checking. |
| `reason` | string | A stable reason string (see below). |

`reason` is one of:

| Value | Cause |
|-------|-------|
| `not_found` | Exit `127` — the command was not found. |
| `not_executable` | Exit `126` — the command was found but not executable. |
| `timeout` | The gate exceeded `execution.timeout_secs` and was killed. |
| `signal:<n>` | The gate was killed by signal `n` (e.g. `signal:9`). |
| `exit_code:<n>` | A normal exit with code `n ≥ 128` (not a signal death). |
| `spawn:<message>` | The process could not be spawned at all. |

## Exit codes

The exit code mirrors the status — it is what scripts branch on without parsing JSON:

| Exit | Status | Meaning |
|------|--------|---------|
| `0` | `pass` | Every matched gate passed. |
| `1` | — | Config or load error (untrusted config, parse failure, missing file). No verdict is produced. |
| `2` | `block` | At least one gate exited `2`. |
| `3` | `internal_error` | At least one gate crashed. |

Adapters fail-open on `3` by default; opt into fail-closed with `HECTOR_FAIL_CLOSED_ON_INTERNAL=1`. See [Running checks](../operating/running-checks.md).

## Versioning

`schema_version` is `4` and bumps only on a breaking change — a field removal, type change, enum-variant removal, or re-interpretation of an existing field. Additive changes (a new optional field, a new enum variant) do not bump it.

Consumers should assert `schema_version >= 4` rather than `== 4`, so a future additive bump doesn't break them.

## See also

- [Running checks](../operating/running-checks.md) — producing and acting on verdicts
- [Telemetry](../operating/telemetry.md) — the on-disk check log, which has its own schema
