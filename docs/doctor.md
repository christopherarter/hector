# `hector doctor` output schema

`hector doctor` is a read-only diagnostic command. It prints a checklist
of every load-time invariant Hector cares about and exits `0` when every
check is `pass` or `warn`, `1` when any check `fail`s.

This document is the public contract for `--format json`. The set of
field *names* and the meaning of each `status` value are stable; new
fields land at the end of `Report` or `CheckResult` with `Option<…>`
defaults so the schema stays additive.

## Top-level shape

```json
{
  "hector_version": "0.2.0",
  "checks": [ /* CheckResult[] */ ]
}
```

| Field | Type | Meaning |
|---|---|---|
| `hector_version` | string | The version of the running `hector` binary. |
| `checks` | array of `CheckResult` | Ordered, one row per check. Order is stable across runs. |

## `CheckResult`

```json
{
  "name": "trust",
  "status": "pass",
  "detail": "fingerprint matches",
  "remediation": null
}
```

| Field | Type | Meaning |
|---|---|---|
| `name` | string (snake_case) | Stable check id. See [check ids](#check-ids). |
| `status` | `"pass"` \| `"warn"` \| `"fail"` | Outcome. Exit-code rule: any `fail` → exit 1; otherwise → exit 0. |
| `detail` | string | One short sentence describing what was checked and what was found. May contain absolute paths, version numbers, or sizes. |
| `remediation` | string \| null | Actionable hint when `status` is not `pass`. `null` on pass. |

## Check ids

These are emitted in this order:

| `name` | What it verifies |
|---|---|
| `binary` | The running `hector` resolves to a path; reports the version. Always `pass`. |
| `config` | `<dir>/.hector.yml` exists. `fail` if missing. |
| `parses` | The config (and every transitive `extends:` ancestor) parses. `fail` if the YAML is malformed or schema_version is unsupported. |
| `trust` | The trust fingerprint matches the recomputed canonical hash. `fail` if it doesn't; `warn` if there's no config to verify. |
| `schema` | `schema_version` is a supported value (currently `2`). `fail` on `1` (legacy bully — remediation: `hector migrate`). |
| `scope_globs` | Every rule's `scope:` constructs a valid glob matcher. `fail` lists the offending rule(s). |
| `engines` | If any rule is `engine: semantic` or `engine: session`, an `llm:` block is present and `api_key_env` resolves to a non-empty value. `provider: ollama` exempts the api-key requirement. `warn` (not `fail`) on missing key — the binary still works for non-LLM rules. |
| `adapter` | `~/.claude/settings.json` exists and a PostToolUse hook references `hector` or the adapter's `hook.sh`. Missing settings file is `warn` (not every user runs Claude Code). |
| `runtime_state` | `<dir>/.hector/` is writable (probed by writing+deleting a marker file). Reports sizes of `baseline.json`, `session.json`, `log.jsonl` if present. |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Every check is `pass` or `warn`. |
| `1` | At least one check is `fail`. |

These are *distinct* from `hector check`'s `0` / `1` / `2` contract.
`doctor` never produces a `Verdict` and never participates in the
adapter exit-code routing.

## Stability

- The set of `name` values is **additive-only**. New checks land at the end of the list.
- `Status` values (`pass` / `warn` / `fail`) are frozen.
- `detail` strings are human-readable and may change between releases — do not parse them.
- `remediation` strings are human-readable and may change between releases — do not parse them.
- The exit-code rule (`0` for pass-or-warn, `1` for any fail) is frozen.
