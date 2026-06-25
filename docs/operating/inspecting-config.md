# Inspecting your config

Two read-only commands answer "what would Hector do here?" without running a single gate. Neither executes a `run` command or writes telemetry. They exit `0` on success and `1` on a config error — never `2`.

## "Which gates apply to this file?"

`hector explain` shows every gate in scope for a file and the command each would run:

```bash
hector explain src/app.ts
```

Reach for it when a gate you expected to fire didn't (or one you didn't expect did) — it tells you whether the file is in scope at all and exactly what `run` would execute. Add `--format json` for machine-readable output (default is `--format human`).

## "What does my config resolve to after `extends:`?"

`hector show-resolved-config` prints the merged gate set after inheritance is applied, with each gate annotated by the file that defined it:

```bash
hector show-resolved-config
```

It does **not** enforce trust — you reach for it precisely when debugging an as-yet-unblessed or in-progress config, so trust enforcement would defeat the purpose.

### Output formats

`--format tsv` (default) is greppable and cuttable. One gate per line, four tab-separated columns — `gate`, `origin`, `files`, `run`:

```bash
hector show-resolved-config | cut -f1,2     # gate ids + origin
hector show-resolved-config | grep biome    # the biome gate's row
```

`--format yaml` and `--format json` render the same merged view as a list of `{ gate, origin, files, run }` objects. These three shapes are a stable contract; see [`show-resolved-config` output](../reference/show-resolved-config.md) for the full reference.

## See also

- [Targeting files](../configuring/targeting-files.md) — the `files:` globs each gate matches
- [Sharing config with `extends:`](../configuring/inheritance.md) — what `show-resolved-config` merges
- [Diagnostics](diagnostics.md) — `hector doctor` for install- and config-level problems
