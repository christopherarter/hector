# Targeting files

Every rule has a `scope:` — the globs that decide which files it applies to. Hector also has skip patterns that take files out of consideration entirely. Together they control what each rule sees.

## Scoping a rule

`scope:` accepts a single glob or a list:

```yaml
rules:
  no-console-log:
    description: "No console.log in committed source."
    engine: script
    scope: ["src/**/*.ts", "app/**/*.ts"]   # a list
    severity: error
    script: "grep -nE 'console\\.log\\(' {file} && exit 1 || exit 0"

  no-todo:
    description: "No TODO markers."
    engine: script
    scope: "src/**/*"                         # a single string
    severity: warning
    script: "grep -n TODO {file} && exit 1 || exit 0"
```

A rule runs against a file when *any* of its scope globs matches. A file outside every rule's scope is checked against nothing and passes trivially.

## Bare patterns match at any depth

A glob with no slash matches the filename at any depth, not only the repo root:

- `*.py` matches `main.py`, `src/app.py`, and `src/pkg/util/io.py`.
- `Makefile` matches `Makefile` and `tools/Makefile`.

Once a pattern contains a slash, it's matched against the full relative path:

- `src/*.py` matches `src/app.py` but **not** `src/pkg/util.py`.
- `src/**/*.py` matches `src/app.py` and `src/pkg/util.py` — `**` spans directories.

This mirrors the behavior of the original bully matcher: a bare extension glob is right-anchored so it catches the file wherever it lives.

## Skipping files

Some files should never be linted — lockfiles, minified bundles, generated code, vendored dependencies. Hector skips a built-in set before any rule runs:

```
Cargo.lock, package-lock.json, yarn.lock, pnpm-lock.yaml, bun.lock,
poetry.lock, Pipfile.lock, *.min.js, *.min.css, dist/**, build/**,
__pycache__/**, node_modules/**, target/**, .next/**, .nuxt/**,
*.generated.*, *.pb.go, *.g.dart, *.freezed.dart
```

A skipped file short-circuits the whole check — no rule is evaluated, no LLM is called, and the verdict is a clean pass.

### Adding your own skip patterns

Add a project-wide `skip:` list to `.hector.yml`:

```yaml
schema_version: 2

skip:
  - "*.snap"
  - "fixtures/**"
  - "vendor/**"

rules:
  # ...
```

Your patterns are added to the built-ins; they don't replace them. A directory pattern like `vendor/**` also matches nested copies (`packages/web/vendor/...`), the same way `node_modules/**` does.

### Per-developer skips

For globs personal to your machine that don't belong in the committed config, create `~/.hector-ignore` — one glob per line, `#` for comments:

```
# personal scratch files
*.scratch.ts
notes/**
```

These merge with the built-ins and the project `skip:` list.

## Checking what matches

To confirm which rules apply to a given file — and which skip pattern, if any, suppressed it — use `hector explain`:

```bash
hector explain src/app.ts
```

See [Inspecting your config](../operating/inspecting-config.md).

## See also

- [Severity and disabling rules](severity-and-disabling.md) — turning a single match off in-line
- [Inspecting your config](../operating/inspecting-config.md) — `explain` and `guide`
- [Sharing config with `extends:`](inheritance.md) — `skip:` lists union across an inheritance chain
