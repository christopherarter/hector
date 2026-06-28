# Sharing config with `extends:`

To reuse a set of gates across repos — or layer a strict profile on top of a base one — a config can inherit from one or more parents with `extends:`.

```yaml
# .hector.yml
extends: ["./shared/base.yml", "./shared/strict.yml"]
gates:
  local-only:
    files: "src/**/*.ts"
    run: ".hector/gates/local-only.sh"
```

`extends:` is a list of parent config paths, relative to the file that declares them. Hector resolves each parent depth-first (a parent may extend its own parents) and detects cycles. The merged gate set is your local gates plus every gate inherited from the closure.

## Precedence on conflict

Inherited gates fill gaps only — they never overwrite. When the same gate id is defined in more than one place, two rules decide the winner:

1. **Local wins.** A gate defined in the child always overrides the same id inherited from any parent.
2. **First parent wins.** When the child extends `[a.yml, b.yml]` and both define the same id, the one from `a.yml` — earlier in the list — wins.

```yaml
# a.yml
gates:
  no-todo:
    files: "src/**/*"
    run: "! grep -n TODO || exit 2"  # proposed content arrives on stdin

# b.yml
gates:
  no-todo:
    files: "src/**/*"
    run: "true"

# child.yml
extends: ["./a.yml", "./b.yml"]
# Result: the no-todo gate from a.yml wins.
```

To flip the precedence, reorder the list: `extends: ["./b.yml", "./a.yml"]`. The order is the priority, the same way a shell `PATH` resolves the first match. Pulling the conflicting gate down into the child settles it outright.

## Trust and `extends:`

Trust isn't a config field and isn't blessed per file. You bless the **root** config you run `hector check` against, and its blessed hash covers the entire `extends:` closure — every file it transitively extends, plus every script under `.hector/gates/`. One bless covers the chain:

```bash
hector trust            # blesses .hector.yml and everything it extends
```

So editing a parent — `shared/base.yml` or one of its gate scripts — invalidates the root's hash and forces a re-review before the next `check`. A parent change can't silently alter what runs under an already-blessed child. (You only bless a parent directly if you also run `hector check --config shared/base.yml` against it as a root in its own right.) See [The trust store](../security/trust.md).

## Confirming the merged result

To see exactly what your config resolves to after inheritance — every gate, annotated with the file that defined it — run:

```bash
hector show-resolved-config
```

See [`show-resolved-config` output](../reference/show-resolved-config.md) and [Inspecting your config](../operating/inspecting-config.md).

## See also

- [The trust store](../security/trust.md) — how one blessing covers the `extends:` closure
- [Config schema](../reference/config-schema.md) — the full gate shape
- [Inspecting your config](../operating/inspecting-config.md) — `show-resolved-config`
