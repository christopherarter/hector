# Config schema

The full shape of `.hector.yml`. For guides on writing rules and configuring scope and severity, see [Writing rules](../writing-rules/README.md) and [Configuring](../configuring/targeting-files.md).

## Top-level

```yaml
schema_version: 2          # required

extends: ["./base.yml"]    # optional — inherit from parent configs

trust:                     # written by `hector trust`
  fingerprint: sha256:...

skip: ["vendor/**"]        # optional — extra skip globs

execution:                 # optional — parallelism tuning
  max_workers: 8

rules:                     # required — the policy rules, keyed by id
  rule-id:
    # ...
```

| Key | Type | Required | Notes |
|-----|------|----------|-------|
| `schema_version` | integer | yes | Must be `2`. `1` is legacy bully — run `hector migrate`. |
| `extends` | list of strings | no | Parent config paths. See [Sharing config with `extends:`](../configuring/inheritance.md). |
| `trust` | block | written by tool | The signed fingerprint. See [The trust gate](../security/trust.md). |
| `skip` | list of strings | no | Globs added to the built-in skip set. See [Targeting files](../configuring/targeting-files.md). |
| `execution` | block | no | See [Execution block](#execution-block). |
| `rules` | map of id → rule | yes | The policy rules. See [Rule](#rule). |

## Rule

Keyed by a rule id you choose. Four fields are always required; the rest depend on the engine.

```yaml
rules:
  no-console-log:
    description: "No console.log in committed source."   # required
    engine: script                                       # required
    scope: ["src/**/*.ts"]                               # required
    severity: error                                      # required
    script: "grep -nE 'console\\.log\\(' {file} && exit 1 || exit 0"
    output: passthrough
    capabilities:
      network: false
      writes: cwd-only
    fix_hint: "Remove the console.log or use the logger."
```

| Field | Type | Required | Engines | Notes |
|-------|------|----------|---------|-------|
| `description` | string | yes | all | Shown when the rule fires. |
| `engine` | enum | yes | all | `script` or `ast`. |
| `scope` | string or list | yes | all | Glob(s). A bare string is treated as a one-element list. |
| `severity` | enum | yes | all | `error` or `warning`. |
| `script` | string | for `script` | `script` | Shell command; `{file}` expands to the path. |
| `output` | enum | no | `script` | `passthrough` (default) or `parsed`. |
| `pattern` | string | for `ast` | `ast` | ast-grep pattern. |
| `language` | string | for `ast` | `ast` | ast-grep language name; required (no inference). |
| `capabilities` | block | no | `script` | Network/write sandbox. See [Capabilities block](#capabilities-block). |
| `fix_hint` | string | no | all | Suggestion attached to the violation. |

A field set for the wrong engine is ignored — `language` on a `script` rule, for example, does nothing.

## Capabilities block

Per-`script`-rule sandbox. Defaults to no network and no writes outside the working directory.

```yaml
capabilities:
  network: false        # default false
  writes: cwd-only      # default none
```

| Field | Type | Values | Default |
|-------|------|--------|---------|
| `network` | bool | `true` / `false` | `false` |
| `writes` | enum | `none`, `cwd-only`, `tmp`, `unrestricted` | `none` |

Enforcement is platform-dependent and partly advisory. See [Capability sandboxing](../security/capabilities.md).

## Execution block

Tunes the worker pool that dispatches rules in parallel.

```yaml
execution:
  max_workers: 8
```

| Field | Type | Notes |
|-------|------|-------|
| `max_workers` | integer | Max worker threads. `0` clamps to 1. |

Absent, the pool defaults to `min(8, num_cpus)`. The `HECTOR_MAX_WORKERS` environment variable overrides whatever is set here.

## Enums

| Enum | Values |
|------|--------|
| `engine` | `script`, `ast` |
| `severity` | `error`, `warning` |
| `output` | `passthrough`, `parsed` |
| `writes` | `none`, `cwd-only`, `tmp`, `unrestricted` |

## Removed engines

A config that still declares `engine: semantic` or `engine: session` is rejected at load with a pointed error naming the rule (e.g. `rule 'X': engine 'semantic' was removed in hector 0.2 — delete this rule or rewrite it as a script or ast rule`). Run `hector migrate` to strip such rules and the old `llm:` block automatically; it prints a `note:` to stderr for each thing it removes.

## See also

- [Writing rules](../writing-rules/README.md) — what each engine does with these fields
- [Verdict JSON](verdict-json.md) — the output shape `hector check` produces
- [CLI reference](cli.md) — commands that read this config
