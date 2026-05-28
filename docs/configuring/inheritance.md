# Sharing config with `extends:`

To reuse a set of rules across repos — or layer a strict profile on top of a base one — a config can inherit from one or more parents with `extends:`.

```yaml
schema_version: 2
extends: ["./shared/base.yml", "./shared/strict.yml"]
trust:
  fingerprint: sha256:...
rules:
  local-only:
    description: "A rule that lives only in this repo."
    engine: script
    scope: ["src/**/*"]
    severity: warning
    script: "true"
```

Hector resolves each parent depth-first (a parent may extend its own parents), detects cycles, and merges the result. Your local rules sit on top of everything inherited.

## What gets inherited

| Field | Inherited? |
|-------|-----------|
| `rules:` | Yes — a local rule overrides a parent rule with the same id. |
| `llm:` | Yes — first parent in the list wins; a local block wins over all parents. |
| `skip:` globs | Yes — union'd across the whole chain and deduplicated. |
| `trust:` | **No** — every file with rules carries its own signed fingerprint. |
| `schema_version:` | Must match across the chain; a mismatch errors at load. |

## Precedence on conflict

When the same rule id or `llm:` block appears in more than one place, two rules decide the winner:

1. **Local wins.** A rule defined in the child always overrides the same id inherited from any parent.
2. **First parent wins.** When the child extends `[a.yml, b.yml]` and both define the same id, the one from `a.yml` — earlier in the list — wins.

```yaml
# a.yml
llm:
  provider: anthropic
  model: claude-from-a

# b.yml
llm:
  provider: openrouter
  model: model-from-b

# child.yml
extends: ["./a.yml", "./b.yml"]
# Result: the llm block from a.yml wins.
```

To flip the precedence, reorder the list: `extends: ["./b.yml", "./a.yml"]`. The order is the priority, the same way a shell `PATH` resolves the first match. Making the order explicit at the call site beats burying the intent in merge rules — and pulling the conflicting field down into the child settles it outright.

## Trust is never inherited

`trust:` blocks don't propagate. Every config file that contains rules — parent or child — must be signed on its own:

```bash
hector trust --config shared/base.yml
hector trust --config .hector.yml
```

This keeps a parent change from silently altering what runs under a child's already-trusted fingerprint. See [The trust gate](../security/trust.md).

## Confirming the merged result

To see exactly what your config resolves to after inheritance — every rule, annotated with the file that defined it — run:

```bash
hector show-resolved-config
```

See [Inspecting your config](../operating/inspecting-config.md).

## See also

- [The trust gate](../security/trust.md) — why each file signs itself
- [LLM providers](llm-providers.md) — the `llm:` block that inheritance merges
- [Inspecting your config](../operating/inspecting-config.md) — `show-resolved-config`
