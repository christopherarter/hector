# CLI reference

Every `hector` subcommand and its flags. For task-oriented guides, see [Running checks](../operating/running-checks.md) and [Inspecting your config](../operating/inspecting-config.md).

The binary is `hector`. Run `hector <command> --help` for the same information at the terminal.

## `hector check`

Run the pipeline against a file, diff, or session.

```
hector check [--file <path>] [--diff <path>] [--content <string|->] [--session]
             [--format human|json] [--config <path>] [--rule <id>]... [--explain]
             [--print-prompt] [--emit-semantic-payload] [--allow-external-paths]
```

| Flag | Default | Notes |
|------|---------|-------|
| `--file <path>` | — | File to check. |
| `--diff <path>` | — | Unified diff to check. |
| `--content <string\|->` | — | Proposed post-edit content instead of disk; `-` reads stdin. Requires `--file`; conflicts with `--diff` and `--session`. |
| `--session` | — | Check accumulated edits in `.hector/session.json`. |
| `--format` | `human` | `human` or `json`. |
| `--config <path>` | `.hector.yml` | Config file to load. |
| `--rule <id>` | — | Evaluate only this rule. Repeatable; multiple flags OR'd. |
| `--explain` | off | Print a per-rule outcome report to stderr after the verdict. |
| `--print-prompt` | off | Render semantic prompts to stdout and exit `0` without calling the LLM. |
| `--emit-semantic-payload` | off | Collect semantic/session rules into a deferred envelope. Adapter-internal; conflicts with `--session` and `--print-prompt`. See [`--emit-semantic-payload`](emit-semantic-payload.md). |
| `--allow-external-paths` | off | Allow checking files outside the config's directory. |

**Exit codes:** `0` pass/warn · `1` config error · `2` block · `3` internal error. See [Verdict JSON](verdict-json.md) and [Running checks](../operating/running-checks.md).

## `hector trust`

Compute the trust fingerprint and write it into the config.

```
hector trust [--config <path>]
```

| Flag | Default |
|------|---------|
| `--config <path>` | `.hector.yml` |

See [The trust gate](../security/trust.md).

## `hector validate`

Parse and validate the config without running any rule.

```
hector validate [--config <path>]
```

| Flag | Default |
|------|---------|
| `--config <path>` | `.hector.yml` |

## `hector init`

Detect the project stack and scaffold a starter `.hector.yml`.

```
hector init [--dir <path>]
```

| Flag | Default |
|------|---------|
| `--dir <path>` | `.` |

## `hector migrate`

Rewrite a legacy `.bully.yml` to `.hector.yml` (schema v1 → v2) and move `.bully/` → `.hector/`.

```
hector migrate [--dir <path>] [--clean]
```

| Flag | Default | Notes |
|------|---------|-------|
| `--dir <path>` | `.` | Directory to migrate. |
| `--clean` | off | Delete `.bully.yml` after migration. |

## `hector baseline`

Record or refresh the violation baseline. With no action, defaults to `record`.

```
hector baseline [record|refresh] [--config <path>] [--scan <glob>]
```

| Argument / flag | Default | Notes |
|------|---------|-------|
| `record` | (default) | Capture current violations to `.hector/baseline.json`. |
| `refresh` | — | Re-hash every stored entry against current file content. |
| `--config <path>` | `.hector.yml` | Config file to load. |
| `--scan <glob>` | — | (record) Restrict the scan to matching files. |

See [Baselines](../configuring/baselines.md).

## `hector session`

Session-state management, used by adapter hooks.

```
hector session record --file <path> --diff <string> [--dir <path>] [--session-id <id>]
hector session start [--dir <path>]
```

**`record`** appends an edit to `.hector/session.json`:

| Flag | Default | Notes |
|------|---------|-------|
| `--file <path>` | — | File the edit touched. |
| `--diff <string>` | — | The edit's diff. |
| `--dir <path>` | `.` | Directory containing `.hector/`. |
| `--session-id <id>` | — | Session identifier. |

**`start`** stamps a `session_init` record into the telemetry log:

| Flag | Default |
|------|---------|
| `--dir <path>` | `.` |

## `hector doctor`

Diagnose the install, config, trust, engine availability, and adapter wiring. Read-only.

```
hector doctor [--dir <path>] [--format human|json]
```

| Flag | Default |
|------|---------|
| `--dir <path>` | `.` |
| `--format` | `human` |

**Exit codes:** `0` if every check passes or warns; `1` on any failure. See [Diagnostics](../operating/diagnostics.md).

## `hector explain`

Show which rules are in scope for a file and which skip pattern (if any) suppresses it. Read-only.

```
hector explain <file> [--format human|json] [--config <path>]
```

| Argument / flag | Default |
|------|---------|
| `<file>` | — (required) |
| `--format` | `human` |
| `--config <path>` | `.hector.yml` |

## `hector guide`

List the rules whose scope matches a file, with description and severity. Read-only.

```
hector guide <file> [--format human|json] [--config <path>]
```

| Argument / flag | Default |
|------|---------|
| `<file>` | — (required) |
| `--format` | `human` |
| `--config <path>` | `.hector.yml` |

## `hector show-resolved-config`

Print the post-`extends:` merged rule set, annotated by defining file. Read-only.

```
hector show-resolved-config [--config <path>] [--format tsv|yaml|json]
```

| Flag | Default |
|------|---------|
| `--config <path>` | `.hector.yml` |
| `--format` | `tsv` |

See [Inspecting your config](../operating/inspecting-config.md).

## `hector record-verdict`

Append one `semantic_verdict` record to `.hector/log.jsonl`. Adapter-internal.

```
hector record-verdict --rule <id> --verdict <pass|violation> [--file <path>] [--dir <path>]
```

See [`record-verdict`](record-verdict.md) for the full contract.

## Read-only commands

`validate`, `doctor`, `explain`, `guide`, and `show-resolved-config` never run a rule, call an LLM, or write telemetry. They exit `0` on success and `1` on a config error — never `2`.
