# Diagnostics

When checks misbehave — hooks not firing, an "untrusted config" error, a gate that won't run — start with `hector doctor`. It's a read-only, minimal static-check command that walks a fixed list of load-time invariants and reports which one is broken and how to fix it:

```bash
hector doctor
```

Each row is a check with a status and, when something's wrong, a remediation hint. The command exits `0` when every check is `pass` or `warn`, and `1` when any check `fail`s — so it drops cleanly into CI as a setup gate.

For a machine-readable report, add `--format json`. The rest of this page is the contract for that output.

> `hector verify` and a fuller `doctor` are planned; today `doctor` runs the five static checks below.

## The five checks

`doctor` emits exactly these checks, in this order:

| `name` | What it verifies |
|---|---|
| `binary` | The running `hector` resolves to a path; reports the version. Always `pass`. |
| `adapter` | `~/.claude/settings.json` exists and a PostToolUse hook references `hector` or the adapter's `hook.sh`. `warn` if the settings file is missing or no hook matches — not every user runs Claude Code. |
| `config` | `<dir>/.hector.yml` exists. `fail` if missing. |
| `parses` | The config (and every transitive `extends:` ancestor) parses. `fail` on malformed YAML or a rejected legacy config. |
| `gate_scripts` | For each gate whose `run` is a single-token path beginning with `.hector/`, that the path exists and is executable. Inline commands (anything with a space) are skipped. `fail` lists the offending gate(s). |

## Report shape

```json
{
  "hector_version": "<x.y.z>",
  "checks": [
    {
      "name": "config",
      "status": "pass",
      "detail": "/work/repo/.hector.yml exists",
      "remediation": null
    }
  ]
}
```

| Field | Type | Meaning |
|---|---|---|
| `hector_version` | string | Version of the running `hector` binary. |
| `checks` | array of check objects | One per check, in the order above. |

Each check object:

| Field | Type | Meaning |
|---|---|---|
| `name` | string | Stable check id (one of the five above). |
| `status` | `"pass"` \| `"warn"` \| `"fail"` | Outcome. Any `fail` → exit `1`; otherwise → exit `0`. |
| `detail` | string | One short sentence on what was checked and found. May contain absolute paths or version numbers. |
| `remediation` | string \| null | Actionable hint when `status` is not `pass`; `null` on pass. |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Every check is `pass` or `warn`. |
| `1` | At least one check is `fail`. |

These are *distinct* from `hector check`'s `0`/`1`/`2`/`3` contract. `doctor` never produces a `Verdict` and never participates in adapter exit-code routing.

## Stability

- The set of `name` values is **additive-only** — new checks land at the end of the list.
- The `status` values (`pass` / `warn` / `fail`) are frozen.
- `detail` and `remediation` strings are human-readable and may change between releases — do not parse them.
- The exit-code rule (`0` for pass-or-warn, `1` for any fail) is frozen.
