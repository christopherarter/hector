# `hector show-resolved-config`

Read-only inspection command. Prints the post-`extends:` merged gate set so you can confirm what your config looks like after inheritance.

```bash
hector show-resolved-config [--config .hector.yml] [--format tsv|yaml|json]
```

Exit codes: `0` on success; `1` on a config error (missing file, parse failure). Never `2` — this command does not run gates.

It does **not** verify trust. You typically reach for it precisely when debugging an as-yet-unblessed config, so trust enforcement would defeat the purpose.

## Origin attribution

Every gate in the output is annotated with the path of the file it was *defined in* — your local `.hector.yml`, an `extends:`-referenced parent, or a deeper ancestor. When a gate id collides between the local file and an inherited one, the local definition wins (matching `extends::resolve` semantics) and the origin reflects that.

## Output: TSV (default)

One gate per line, four columns separated by a single tab, sorted by gate id, no header row:

| # | Column | Notes |
|---|--------|-------|
| 1 | `gate` | Gate id from the merged config. |
| 2 | `origin` | Path of the file that defined the gate. |
| 3 | `files` | Comma-separated glob list. |
| 4 | `run` | The gate's shell command. |

Greppable and cuttable:

```bash
hector show-resolved-config | cut -f1,2      # gate ids + origin
hector show-resolved-config | grep biome     # the biome gate's row
```

## Output: YAML (`--format yaml`)

A sequence of `{ gate, origin, files, run }`, one entry per gate, sorted by gate id:

```yaml
- gate: inherited
  origin: /work/repo/base.yml
  files:
  - "*.txt"
  run: "true"
- gate: local-only
  origin: /work/repo/.hector.yml
  files:
  - "*.md"
  run: "true"
```

## Output: JSON (`--format json`)

Pretty-printed array of the same `{ gate, origin, files, run }` objects, sorted by gate id:

```json
[
  {
    "gate": "inherited",
    "origin": "/work/repo/base.yml",
    "files": ["*.txt"],
    "run": "true"
  },
  {
    "gate": "local-only",
    "origin": "/work/repo/.hector.yml",
    "files": ["*.md"],
    "run": "true"
  }
]
```

## Stability

These three output shapes are a public contract. The TSV column order and the field set of the YAML/JSON objects freeze with this command. Breaking changes go through a versioned `--format` value (e.g. `--format json-v2`).
