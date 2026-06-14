# Running checks

`hector check` runs your rules against a file and returns a verdict. It's what your adapter calls on every edit, and what you run by hand to test a rule.

```bash
hector check --file src/auth.rs
```

Hector loads `.hector.yml`, verifies its trust fingerprint, matches the file against each rule's scope, runs the matched rules, and prints a verdict.

## Choosing what to check

| Flag | Checks |
|------|--------|
| `--file <path>` | A single file on disk. |
| `--diff <path>` | A unified diff, for rules that reason about changes. |
| `--content <string\|->` | Proposed post-edit content instead of disk — pass `-` to read from stdin. Requires `--file` for scope matching. For pre-edit adapters. |

`--content` is for adapters that gate an edit *before* it lands on disk. `script` rules still read the on-disk file; AST rules see the proposed content. See [Running a shell check](../writing-rules/shell-checks.md).

## Exit codes

The exit code is the contract — adapters and CI branch on it:

| Code | Meaning |
|------|---------|
| `0` | **Pass or Warn** — all rules evaluated cleanly, or only `warning`-severity rules fired. |
| `1` | **Config error** — untrusted fingerprint, parse failure, missing file. |
| `2` | **Block** — at least one `error`-severity violation. |
| `3` | **Internal error** — at least one rule failed to evaluate (AST refused the diff, script failed to spawn). |

`0` and `2` are the normal pass/block signals. `1` means *fix your config*. `3` means *a rule couldn't run* — distinct from *a rule found a problem*.

## Fail-open vs. fail-closed on internal errors

Exit `3` is an open question: a rule couldn't run, so Hector can't say pass or block. Adapters **fail-open** on `3` by default — the edit is allowed, because an unrelated problem (say, a script that failed to spawn) shouldn't block an agent's work.

To flip that and treat internal errors as blocking, set:

```bash
export HECTOR_FAIL_CLOSED_ON_INTERNAL=1
```

Use fail-closed where a skipped check is unacceptable — for example, a CI gate where a rule silently not running would let a violation through.

## Output format

`--format human` (default) prints a readable verdict. `--format json` prints the machine-readable [Verdict JSON](../reference/verdict-json.md):

```bash
hector check --file src/auth.rs --format json
```

## Other useful flags

| Flag | Effect |
|------|--------|
| `--config <path>` | Use a config other than `.hector.yml`. |
| `--rule <id>` | Evaluate only this rule. Repeatable; multiple flags are OR'd. |
| `--explain` | After the verdict, print a per-rule outcome report to stderr. |

See the [CLI reference](../reference/cli.md) for the complete list.

## See also

- [Verdict JSON](../reference/verdict-json.md) — the `--format json` shape and exit codes
- [Inspecting your config](inspecting-config.md) — read-only commands that never run a rule
- [Diagnostics](diagnostics.md) — `hector doctor` when checks behave unexpectedly
