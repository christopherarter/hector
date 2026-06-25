# CLI reference

Every `hector` subcommand and its flags. For task-oriented guides, see [Running checks](../operating/running-checks.md) and [Inspecting your config](../operating/inspecting-config.md).

The binary is `hector`. Run `hector <command> --help` for the same information at the terminal.

## `hector check`

Run the gates against a file or diff.

```
hector check [--file <path>] [--diff <path>] [--content <string|->]
             [--format human|json] [--config <path>] [--gate <id>]...
             [--event edit|write|pre-commit|manual] [--explain]
             [--allow-external-paths]
```

| Flag | Default | Notes |
|------|---------|-------|
| `--file <path>` | — | File to check. |
| `--diff <path>` | — | Unified diff; each changed file is checked. |
| `--content <string\|->` | — | Proposed post-edit content to evaluate instead of reading `--file` from disk; `-` reads it from stdin. Requires `--file`; conflicts with `--diff`. |
| `--format` | `human` | `human` or `json`. See [Verdict JSON](verdict-json.md). |
| `--config <path>` | `.hector.yml` | Config file to load. |
| `--gate <id>` | — | Run only this gate. Repeatable; multiple flags are OR'd. |
| `--event` | `manual` | What triggered the check, surfaced to gates as `$HECTOR_EVENT`. One of `edit`, `write`, `pre-commit`, `manual`. |
| `--explain` | off | Print a per-gate outcome report to stderr after the verdict. |
| `--allow-external-paths` | off | Allow checking files whose canonical path falls outside the config's directory. |

**Exit codes:** `0` pass · `1` config or load error · `2` block · `3` internal error. See [Running checks](../operating/running-checks.md).

## `hector trust`

Bless a config in the out-of-repo trust store so `hector check` will run it. Computes a SHA-256 over the config, every config it `extends:`, and the files under each `.hector/gates/`, and records it at `~/.config/hector/trust.json` (keyed by the config's absolute path).

```
hector trust [--config <path>]
```

| Flag | Default |
|------|---------|
| `--config <path>` | `.hector.yml` |

See [The trust store](../security/trust.md).

## `hector validate`

Parse and validate the config without running any gate.

```
hector validate [--config <path>]
```

| Flag | Default |
|------|---------|
| `--config <path>` | `.hector.yml` |

## `hector init`

Detect the project stack and scaffold a starter `.hector.yml`, then bless it.

```
hector init [--dir <path>]
```

| Flag | Default |
|------|---------|
| `--dir <path>` | `.` |

## `hector doctor`

Diagnose the install, config, and adapter wiring. Read-only.

```
hector doctor [--dir <path>] [--format human|json]
```

| Flag | Default |
|------|---------|
| `--dir <path>` | `.` |
| `--format` | `human` |

**Exit codes:** `0` if every check passes or warns; `1` on any failure. See [Diagnostics](../operating/diagnostics.md).

## `hector explain`

Show which gates are in scope for a file and the command each would run. Read-only.

```
hector explain <file> [--format human|json] [--config <path>]
```

| Argument / flag | Default |
|------|---------|
| `<file>` | — (required) |
| `--format` | `human` |
| `--config <path>` | `.hector.yml` |

## `hector show-resolved-config`

Print the post-`extends:` merged gate set, each gate annotated by the file that defined it. Read-only.

```
hector show-resolved-config [--config <path>] [--format tsv|yaml|json]
```

| Flag | Default |
|------|---------|
| `--config <path>` | `.hector.yml` |
| `--format` | `tsv` |

See [`show-resolved-config` output](show-resolved-config.md).

## Read-only commands

`validate`, `doctor`, `explain`, and `show-resolved-config` never run a gate or write telemetry. They exit `0` on success and `1` on a config error — never `2`. Trust is enforced only by `check`; these commands run against an unblessed config so you can debug it.
