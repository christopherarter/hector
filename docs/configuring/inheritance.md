# Sharing config with `extends:`

To reuse a set of rules across repos — or layer a strict profile on top of a base one — a config can inherit from one or more parents with `extends:`.

```yaml
schema_version: 2
extends: ["./shared/base.yml", "./shared/strict.yml"]
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
| `skip:` globs | Yes — union'd across the whole chain and deduplicated. |
| trust | n/a — trust isn't a config field. You bless the root config in the out-of-repo store and its hash covers the whole `extends:` closure. See [Trust and `extends:`](#trust-and-extends). |
| `schema_version:` | Must match across the chain; a mismatch errors at load. |

## Precedence on conflict

When the same rule id appears in more than one place, two rules decide the winner:

1. **Local wins.** A rule defined in the child always overrides the same id inherited from any parent.
2. **First parent wins.** When the child extends `[a.yml, b.yml]` and both define the same id, the one from `a.yml` — earlier in the list — wins.

```yaml
# a.yml
rules:
  no-todo:
    description: "from a"
    engine: script
    scope: ["src/**/*"]
    severity: error
    script: "grep -n TODO {file} && exit 1 || exit 0"

# b.yml
rules:
  no-todo:
    description: "from b"
    engine: script
    scope: ["src/**/*"]
    severity: warning
    script: "true"

# child.yml
extends: ["./a.yml", "./b.yml"]
# Result: the no-todo rule from a.yml wins.
```

To flip the precedence, reorder the list: `extends: ["./b.yml", "./a.yml"]`. The order is the priority, the same way a shell `PATH` resolves the first match. Making the order explicit at the call site beats burying the intent in merge rules — and pulling the conflicting rule down into the child settles it outright.

## Trust and `extends:`

Trust isn't a config field and isn't blessed per file. You bless the **root** config you run `hector check` against, and its blessed hash covers the entire `extends:` closure — every file it transitively extends, plus their gate scripts. One bless covers the chain:

```bash
hector trust            # blesses .hector.yml and everything it extends
```

So editing a parent — `shared/base.yml` or one of its gate scripts — invalidates the root's hash and forces a re-review before the next `check`. A parent change can't silently alter what runs under an already-blessed child. (You only bless a parent directly if you also run `hector check --config shared/base.yml` against it as a root in its own right.) See [The trust gate](../security/trust.md).

## Confirming the merged result

To see exactly what your config resolves to after inheritance — every rule, annotated with the file that defined it — run:

```bash
hector show-resolved-config
```

See [Inspecting your config](../operating/inspecting-config.md).

## See also

- [The trust gate](../security/trust.md) — how one blessing covers the `extends:` closure
- [Inspecting your config](../operating/inspecting-config.md) — `show-resolved-config`
