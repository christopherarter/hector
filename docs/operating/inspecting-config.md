# Inspecting your config

Two read-only commands answer "what would IronLint do here?" without running a single check. Neither executes a `run` command or writes telemetry. They exit `0` on success and `1` on a config error — never `2`.

## "Which checks apply to this file?"

`ironlint explain` shows every check in scope for a file and the command each would run:

```bash
ironlint explain src/app.ts
```

Reach for it when a check you expected to fire didn't (or one you didn't expect did) — it tells you whether the file is in scope at all and exactly what `run` would execute. Add `--format json` for machine-readable output (default is `--format human`).

## "What does my config resolve to after `extends:`?"

`ironlint show-resolved-config` prints the merged check set after inheritance is applied, with each check annotated by the file that defined it:

```bash
ironlint show-resolved-config
```

It does **not** enforce trust — you reach for it precisely when debugging an as-yet-unblessed or in-progress config, so trust enforcement would defeat the purpose.

### Output formats

`--format tsv` (default) is greppable and cuttable. One check per line, four tab-separated columns — `check`, `origin`, `files`, `run`:

```bash
ironlint show-resolved-config | cut -f1,2     # check ids + origin
ironlint show-resolved-config | grep biome    # the biome check's row
```

`--format yaml` and `--format json` render the same merged view as a list of `{ check, origin, files, run }` objects. These three shapes are a stable contract; see [`show-resolved-config` output](../reference/show-resolved-config.md) for the full reference.

## See also

- [Targeting files](../configuring/targeting-files.md) — the `files:` globs each check matches
- [Sharing config with `extends:`](../configuring/inheritance.md) — what `show-resolved-config` merges
- [Diagnostics](diagnostics.md) — `ironlint doctor` for install- and config-level problems
