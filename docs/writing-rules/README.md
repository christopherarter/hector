# Writing rules

A rule is one policy: a thing your agent must not do, plus how to detect it. You write rules under the `rules:` map in `.hector.yml`, keyed by a rule id you choose.

Here is a complete rule:

```yaml
rules:
  no-console-log:
    description: "No console.log in committed source."
    engine: script
    scope: ["src/**/*.ts"]
    severity: error
    script: "grep -nE 'console\\.log\\(' {file} && exit 1 || exit 0"
```

Every rule carries the same four required fields:

| Field | What it does |
|-------|--------------|
| `description` | One sentence, shown to the agent when the rule fires. Write it as the fix, not the complaint — "Return an error instead of panicking" beats "panic found". |
| `engine` | Which detector runs the rule: `script` or `ast`. |
| `scope` | Glob(s) selecting which files the rule applies to. A bare string or a list. See [Targeting files](../configuring/targeting-files.md). |
| `severity` | `error` blocks the edit (exit `2`); `warning` reports but allows it. See [Severity and disabling](../configuring/severity-and-disabling.md). |

The remaining fields depend on the engine — `script:` for a shell rule, `pattern:` and `language:` for an AST rule, and so on. Each engine page covers its own.

## Choosing an engine

Reach for the cheapest engine that can express the policy. Cost rises down this table; so does expressive power.

| Engine | Use it when | Cost |
|--------|-------------|------|
| [`script`](shell-checks.md) | A shell command can decide it — grep, a linter, a test, a custom script. | Process spawn. |
| [`ast`](matching-code.md) | The policy is about code *structure*: a banned call, macro, or syntax, regardless of formatting. | In-process, fast. |

Reach for `ast` when the policy is about code structure and a regex would misfire; otherwise a `script` rule covers it.

## How a check runs

When `hector check --file <path>` runs, Hector:

1. **Skips** the file if it matches a built-in or configured skip pattern (lockfiles, `node_modules/`, etc.) — no rules run.
2. **Matches scope** — selects the rules whose `scope:` globs include the file.
3. **Dispatches** each matched rule to its engine.
4. **Filters** out any violation that's baselined or suppressed by a `hector-disable:` directive.
5. **Returns a verdict** — `pass`, `warn`, or `block` — and the matching exit code.

A rule fires once per match. The `script` engine emits at most one violation per file; the `ast` engine emits one per matched node.

## Per-rule fields by engine

| Field | Engines | Purpose |
|-------|---------|---------|
| `script` | `script` | The shell command to run. |
| `output` | `script` | `passthrough` (default) or `parsed`. See [Running a shell check](shell-checks.md). |
| `capabilities` | `script` | Network and write sandboxing. See [Capability sandboxing](../security/capabilities.md). |
| `pattern` | `ast` | The ast-grep structural pattern. |
| `language` | `ast` | Language to parse as (`rust`, `ts`, `python`, …). Required. |
| `fix_hint` | all | A suggestion attached to the violation, shown alongside `description`. |

See the [Config schema](../reference/config-schema.md) for the exhaustive field reference.
