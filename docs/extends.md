# `extends:` — config inheritance

A child config can inherit from one or more parents:

```yaml
schema_version: 2
extends: ["./shared/base.yml", "./shared/strict.yml"]
trust:
  fingerprint: sha256:...
rules:
  local-only:
    description: ...
```

Hector resolves parents depth-first (a parent can extend its own parents), detects cycles, and merges fields. The `trust:` block is **never inherited** — every file with rules must be signed locally.

## Precedence on conflict

When the same key appears in multiple sources, Hector applies these rules in order:

1. **Local-in-child always wins.** A rule defined at the top level of the child wins over the same rule id inherited from any parent.
2. **First-parent-wins on multi-parent conflict.** When the child extends `[A.yml, B.yml]` and *both* parents define the same `llm:` block or the same rule id, the value from `A.yml` (earlier in the list) wins. Reorder the `extends:` list to flip precedence.

Examples:

```yaml
# a.yml
llm:
  provider: anthropic
  model: claude-from-a

# b.yml
llm:
  provider: openai-compat
  model: gpt-from-b

# child.yml
extends: ["./a.yml", "./b.yml"]
# Result: llm.provider == "anthropic" (a.yml wins).
```

```yaml
# child.yml with a local rule overriding both parents
extends: ["./a.yml", "./b.yml"]
rules:
  some-rule:
    description: "wins over a.yml and b.yml's some-rule"
    ...
```

## What is and isn't inherited

| Field | Inherited? |
|---|---|
| `rules:` | Yes (child rules override parent rules with the same id) |
| `llm:` | Yes (first-parent-wins; local-in-child wins) |
| `skip:` globs | Yes (union'd across the whole chain, deduplicated) |
| `trust:` | **No** — every file with rules must carry its own signed fingerprint |
| `extends:` | N/A (resolved at parse time) |
| `schema_version:` | Must match across the chain; mismatches error at load |

## Why first-parent-wins?

The precedent is shell `PATH` semantics and language-import ordering: an explicit ordered list with the first occurrence taking precedence. The alternative — last-parent-wins — looks like "shadowing" but tends to surprise authors who treat `extends:` as a priority list. Reordering `extends:` makes the intent visible at the call site rather than buried in the merge rules.

A child author who needs the opposite behavior can write `extends: [B.yml, A.yml]` instead of `extends: [A.yml, B.yml]`, or pull the conflicting field local to make the intent explicit.

## See also

- `docs/audits/2026-05-24-check-end-to-end-audit.md` — finding D6 (the multi-parent precedence audit)
- `crates/hector-core/src/config/extends.rs` — resolver implementation
- `crates/hector-core/tests/extends.rs` — precedence regression tests
