# Targeting files

Every gate has a `files:` glob — the pattern that decides which files the gate runs against.

```yaml
# .hector.yml
gates:
  no-console:
    files: "**/*.ts"                       # one glob
    run: "! grep -nH 'console.log' \"$HECTOR_FILE\" || exit 2"

  ts-style:
    files: ["src/**/*.ts", "app/**/*.ts"]  # or a list
    run: ".hector/gates/style.sh"
```

`files` is a single glob or a list of globs. A gate runs against a file when *any* of its globs matches. A file matched by no gate is checked against nothing and passes.

## Bare patterns match at any depth

A glob with no slash matches the filename wherever it lives, not only at the project root — `*.ts` is equivalent to `**/*.ts`:

- `*.py` matches `main.py`, `src/app.py`, and `src/pkg/util/io.py`.
- `Makefile` matches `Makefile` and `tools/Makefile`.

Once a pattern contains a slash, it's matched against the full path relative to the project root:

- `src/*.py` matches `src/app.py` but **not** `src/pkg/util.py`.
- `src/**/*.py` matches `src/app.py` and `src/pkg/util.py` — `**` spans directories.

A bare extension glob is right-anchored so it catches the file at any depth. This mirrors the original bully matcher.

## Checking what matches

To confirm which gates are in scope for a given file, run `hector explain`:

```bash
hector explain src/app.ts
```

See [Inspecting your config](../operating/inspecting-config.md).

## See also

- [Config schema](../reference/config-schema.md) — the full `files` / `run` shape
- [Disabling a gate in-line](disabling.md) — turning one gate off in a single file
- [Sharing config with `extends:`](inheritance.md) — inherit gates across repos
