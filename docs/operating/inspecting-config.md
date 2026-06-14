# Inspecting your config

Three read-only commands answer "what would Hector do here?" without running a single rule. None of them executes a `script:` or writes telemetry. They exit `0` on success and `1` on a config error — never `2`.

## "Which rules apply to this file?"

`hector explain` shows every rule in scope for a file, which scope glob matched, and — if the file is skipped — which skip pattern suppressed it:

```bash
hector explain src/app.ts
```

Reach for it when a rule you expected to fire didn't (or one you didn't expect did). It tells you whether the file is in scope at all. Add `--format json` for machine-readable output.

## "What does this file get checked against?"

`hector guide` lists the rules whose scope matches a file, with each rule's description and severity:

```bash
hector guide src/app.ts
```

Where `explain` is for debugging *why* a rule matched, `guide` is the quick "what policies apply here" summary — useful to hand an agent the rules it's about to be held to.

## "What does my config resolve to after `extends:`?"

`hector show-resolved-config` prints the merged rule set after inheritance is applied, with each rule annotated by the file that defined it:

```bash
hector show-resolved-config
```

It does **not** verify the trust fingerprint — you reach for it precisely when debugging an unsigned or in-progress config, so trust enforcement would defeat the purpose.

### Output formats

`--format tsv` (default) is greppable and cuttable:

```bash
hector show-resolved-config | cut -f1,2,6     # id, engine, origin
hector show-resolved-config | grep ast        # all ast rules
```

The TSV columns, in order: `id`, `engine`, `severity`, `scope`, `fix_hint`, `origin`.

`--format yaml` and `--format json` render the same merged view (minus `trust:` and `extends:`, which no longer apply post-merge), each rule carrying the path of the file that defined it. These three shapes are a stable contract; see [`show-resolved-config`](../reference/show-resolved-config.md) for the full output reference.

## See also

- [Targeting files](../configuring/targeting-files.md) — the scope and skip rules `explain` reports on
- [Sharing config with `extends:`](../configuring/inheritance.md) — what `show-resolved-config` merges
- [Diagnostics](diagnostics.md) — `hector doctor` for install- and trust-level problems
