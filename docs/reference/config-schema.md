# Config schema

The full shape of `.hector.yml`. For guides on writing rules and configuring scope, severity, and LLMs, see [Writing rules](../writing-rules/README.md) and [Configuring](../configuring/targeting-files.md).

## Top-level

```yaml
schema_version: 2          # required

llm:                       # optional — required by semantic/session rules
  provider: anthropic
  model: claude-sonnet-4-6
  api_key_env: ANTHROPIC_API_KEY

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
| `llm` | block | no | Required at evaluation by `semantic`/`session` rules. See [LLM block](#llm-block). |
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
| `description` | string | yes | all | Shown when the rule fires; doubles as the prompt for `semantic`/`session`. |
| `engine` | enum | yes | all | `script`, `ast`, `semantic`, or `session`. |
| `scope` | string or list | yes | all | Glob(s). A bare string is treated as a one-element list. |
| `severity` | enum | yes | all | `error` or `warning`. |
| `script` | string | for `script` | `script` | Shell command; `{file}` expands to the path. |
| `output` | enum | no | `script` | `passthrough` (default) or `parsed`. |
| `pattern` | string | for `ast` | `ast` | ast-grep pattern. |
| `language` | string | for `ast` | `ast` | ast-grep language name; required (no inference). |
| `context` | enum | no | `semantic` | `diff` (default), `file`, or `repo`. |
| `capabilities` | block | no | `script` | Network/write sandbox. See [Capabilities block](#capabilities-block). |
| `fix_hint` | string | no | all | Suggestion attached to the violation. |

A field set for the wrong engine is ignored — `language` on a `script` rule, for example, does nothing.

## LLM block

```yaml
llm:
  provider: anthropic            # required
  model: claude-sonnet-4-6       # required for direct-API providers
  api_key_env: ANTHROPIC_API_KEY # provider-dependent
  base_url: https://...          # optional endpoint override
  evaluator_model: haiku         # claude-code-subagent only
```

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `provider` | string | yes | `anthropic`, `openrouter`, `ollama`, `claude-code-subagent`. |
| `model` | string | direct-API only | Ignored for `claude-code-subagent`. |
| `api_key_env` | string | provider-dependent | Name of the env var holding the key. Not used by `ollama` or `claude-code-subagent`. |
| `base_url` | string | no | Overrides the provider default endpoint. |
| `evaluator_model` | string | no | Only for `claude-code-subagent`; the subagent's model. |

See [LLM providers](../configuring/llm-providers.md) for per-provider defaults.

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
| `engine` | `script`, `ast`, `semantic`, `session` |
| `severity` | `error`, `warning` |
| `output` | `passthrough`, `parsed` |
| `context` | `diff`, `file`, `repo` |
| `writes` | `none`, `cwd-only`, `tmp`, `unrestricted` |

## See also

- [Writing rules](../writing-rules/README.md) — what each engine does with these fields
- [Verdict JSON](verdict-json.md) — the output shape `hector check` produces
- [CLI reference](cli.md) — commands that read this config
