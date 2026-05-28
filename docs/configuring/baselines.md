# Baselines

When you adopt a rule on a codebase that doesn't pass it yet, a baseline silences the violations that already exist so you only get flagged on *new* ones. It's how you turn on a strict rule without drowning in day-one noise.

## Record a baseline

With your rules in place, snapshot the current violations:

```bash
hector baseline record
```

This scans your files, runs the rules, and writes every violation it finds to `.hector/baseline.json`. From then on, `hector check` suppresses any violation that's in the baseline — those are the issues you've accepted for now. A violation that *isn't* in the baseline still fires.

To restrict the scan to part of the tree:

```bash
hector baseline record --scan "src/**"
```

## How a violation is matched

A baselined violation is identified by its rule id, file, and line. Move the code and the violation re-surfaces — which is what you want: the baseline pardons the issue where it is, not everywhere forever.

File-level violations (the line-less findings from `script` and `semantic` rules) are matched on both their fingerprint and a normalized hash of the violation message, so a baseline entry doesn't accidentally pardon a *different* file-level finding from the same rule. Normalization strips timestamps and color codes so cosmetic differences don't cause a re-flag.

## Refresh after edits

As you fix issues and edit files, baseline entries drift — a pardoned line moves or disappears. Re-hash the baseline against current file content:

```bash
hector baseline refresh
```

This re-checks each stored entry against the file as it is now and drops entries whose line is gone. Run it after a cleanup pass to keep the baseline honest, so it isn't pardoning issues that no longer exist.

## Working through a baseline

The point of a baseline is to shrink it. A healthy workflow:

1. Turn on the rule and `hector baseline record` to accept the backlog.
2. Fix violations as you touch the surrounding code — each fix means one fewer accepted issue.
3. Run `hector baseline refresh` periodically to drop the resolved entries.
4. When the baseline is empty, delete `.hector/baseline.json` — the rule now holds on the whole codebase.

## Baseline vs. disable directives

A baseline is for a *bulk* of pre-existing violations you intend to work down. A [`hector-disable:` directive](severity-and-disabling.md) is for a *single* line that's correct by design and should stay exempt. Use the baseline to adopt a rule; use a directive for a permanent, reviewed exception.

## See also

- [Severity and disabling rules](severity-and-disabling.md) — in-line, permanent exceptions
- [Telemetry](../operating/telemetry.md) — track how often rules fire over time
