# Verdict JSON

`hector check --format json` prints a `Verdict` — the machine-readable result a CI job or adapter consumes. This page is the contract for that shape and the exit codes that go with it.

```json
{
  "schema_version": 3,
  "hector_version": "0.2.0",
  "status": "block",
  "violations": [
    {
      "rule_id": "no-todo-in-src",
      "severity": "error",
      "engine": "ast",
      "file": "src/lib.rs",
      "line": 42,
      "column": 9,
      "message": "todo!() must not ship.",
      "suggestion": "Replace with a real implementation or return an error.",
      "context": "fn handler() {\n    todo!()\n}"
    }
  ],
  "passed_checks": ["no-unwrap-in-src"],
  "elapsed_ms": 42
}
```

## Top-level fields

| Field | Type | Notes |
|-------|------|-------|
| `schema_version` | integer | Currently `3`. See [Versioning](#versioning). |
| `hector_version` | string | Version of the binary that produced the verdict. |
| `status` | enum | `pass`, `warn`, `block`, or `internal_error`. |
| `violations` | array | One [Violation](#violation) per finding. Empty on a clean pass. |
| `passed_checks` | array of strings | Rule ids that ran and passed. |
| `elapsed_ms` | integer | Wall-clock for the whole check. |

## `status`

| Value | Meaning |
|-------|---------|
| `pass` | No violations. |
| `warn` | Only `warning`-severity violations fired. |
| `block` | At least one `error`-severity violation fired. |
| `internal_error` | At least one rule failed to evaluate. Those rows have `engine: internal`. |

The status is the worst outcome across all rules: any internal error makes it `internal_error`; otherwise any error-severity violation makes it `block`; otherwise any violation at all makes it `warn`; otherwise `pass`.

## Exit codes

The exit code mirrors the status — it's what scripts branch on without parsing JSON:

| Exit | Status | Meaning |
|------|--------|---------|
| `0` | `pass` / `warn` | Evaluated cleanly, or warnings only. |
| `1` | — | Config error (untrusted, parse failure, missing file). No verdict produced. |
| `2` | `block` | At least one error-severity violation. |
| `3` | `internal_error` | At least one rule failed to evaluate. |

Adapters fail-open on `3` by default; opt into fail-closed with `HECTOR_FAIL_CLOSED_ON_INTERNAL=1`. See [Running checks](../operating/running-checks.md).

## Violation

| Field | Type | Notes |
|-------|------|-------|
| `rule_id` | string | The rule that fired. For internal errors, suffixed with `__internal`. |
| `severity` | enum | `error` or `warning`. |
| `engine` | enum | `script`, `ast`, `trust`, or `internal`. |
| `file` | string | Path the violation is on. |
| `line` | integer or null | 1-based line. Only the `ast` engine always sets it. |
| `column` | integer or null | 1-based column. Only the `ast` engine sets it. |
| `message` | string | The finding. From the rule `description` or the script output. |
| `suggestion` | string or null | The rule's `fix_hint`, if set. |
| `context` | string or null | Surrounding source. Only the `ast` engine sets it (matched line ±3). |

The `script` engine has no positional information from a command's exit, so it leaves `column` and `context` null and usually `line` too.

## Versioning

`schema_version` is `3` and bumps only on a breaking change — a field removal, type change, enum-variant removal, or re-interpretation of an existing field. Additive changes (a new optional field, a new enum variant) do **not** bump it.

Consumers should assert `schema_version >= 3` rather than `== 3`, so a future additive bump doesn't break them. The shape is "locked-but-unstable" through 0.2 and freezes at 0.3.

## See also

- [Running checks](../operating/running-checks.md) — producing and acting on verdicts
- [Severity and disabling rules](../configuring/severity-and-disabling.md) — how severity sets status
- [Telemetry](../operating/telemetry.md) — the on-disk check log, a separate schema
