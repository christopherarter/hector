# `ironlint show-resolved-config`

Read-only inspection command. Prints the post-`extends:` merged check set so you can confirm what your config looks like after inheritance.

```bash
ironlint show-resolved-config [--config .ironlint.yml] [--format tsv|yaml|json]
```

Exit codes: `0` on success; `1` on a config error (missing file, parse failure). Never `2` — this command does not run checks.

It does **not** verify trust. You typically reach for it precisely when debugging an as-yet-unblessed config, so trust enforcement would defeat the purpose.

## Origin attribution

Every check in the output is annotated with the path of the file it was *defined in* — your local `.ironlint.yml`, an `extends:`-referenced parent, or a deeper ancestor. When a check id collides between the local file and an inherited one, the local definition wins (matching `extends::resolve` semantics) and the origin reflects that.

## Output: TSV (default)

One check per line, four columns separated by a single tab, sorted by check id, no header row:

| # | Column | Notes |
|---|--------|-------|
| 1 | `check` | Check id from the merged config. |
| 2 | `origin` | Path of the file that defined the check. |
| 3 | `files` | Comma-separated glob list. |
| 4 | `run` | The check's shell command. |

Greppable and cuttable:

```bash
ironlint show-resolved-config | cut -f1,2      # check ids + origin
ironlint show-resolved-config | grep biome     # the biome check's row
```

## Output: YAML (`--format yaml`)

A sequence of `{ check, origin, files, run }`, one entry per check, sorted by check id:

```yaml
- check: inherited
  origin: /work/repo/base.yml
  files:
  - "*.txt"
  run: "true"
- check: local-only
  origin: /work/repo/.ironlint.yml
  files:
  - "*.md"
  run: "true"
```

## Output: JSON (`--format json`)

Pretty-printed array of the same `{ check, origin, files, run }` objects, sorted by check id:

```json
[
  {
    "check": "inherited",
    "origin": "/work/repo/base.yml",
    "files": ["*.txt"],
    "run": "true"
  },
  {
    "check": "local-only",
    "origin": "/work/repo/.ironlint.yml",
    "files": ["*.md"],
    "run": "true"
  }
]
```

## Stability

These three output shapes are a public contract. The TSV column order and the field set of the YAML/JSON objects freeze with this command. Breaking changes go through a versioned `--format` value (e.g. `--format json-v2`).
