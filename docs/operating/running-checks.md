# Running checks

`hector check` runs your gates against a file and returns a verdict. It's what your adapter calls on every edit, and what you run by hand to test a gate.

```bash
hector check --file src/auth.rs
```

Hector loads `.hector.yml`, confirms the config is trusted, then runs every gate whose `files` globs match the path — one `run` invocation per matching file. It reads only each gate's exit code, folds them into a single verdict, and prints it.

## Choosing what to check

| Flag | Checks |
|------|--------|
| `--file <path>` | A single file on disk. |
| `--diff <path>` | A unified diff; each changed file is checked. |
| `--content <string\|->` | Proposed post-edit content instead of reading `--file` from disk — pass `-` to read it from stdin. Requires `--file`; conflicts with `--diff`. |

`--content` evaluates a *proposed* edit before it lands on disk — the case an adapter hits when it gates an agent's write before committing it. The proposed content reaches every matching gate the same way on-disk content does: on **stdin**. There is no engine/on-disk split — a gate sees the bytes you pass, whether they came from disk or `--content`.

## Exit codes

The exit code is the contract — adapters and CI branch on it:

| Code | Meaning |
|------|---------|
| `0` | **Pass** — no gate blocked and none crashed. |
| `1` | **Config or load error** — untrusted config or gates, parse failure, missing file, unknown `--gate`. No verdict is produced. |
| `2` | **Block** — at least one gate exited `2`. |
| `3` | **Internal error** — at least one gate crashed (command not found, not executable, timed out, or killed by a signal). |

There is no warn tier. `0` and `2` are the normal pass/block signals. `1` means *fix your config or trust it*. `3` means *a gate couldn't run* — distinct from *a gate found a problem*. A gate blocks only by exiting `2`; every other clean exit (`0`, `1`, `3`–`125`) is a pass, so a tool that exits `1` on findings passes unless its `run` remaps that to `2`.

## Fail-open vs. fail-closed on internal errors

Exit `3` is an open question: a gate couldn't run, so Hector can't say pass or block. Adapters **fail-open** on `3` by default — the edit is allowed, because an unrelated problem (a script that failed to spawn, say) shouldn't block an agent's work.

To flip that and treat internal errors as blocking, set:

```bash
export HECTOR_FAIL_CLOSED_ON_INTERNAL=1
```

Use fail-closed where a skipped check is unacceptable — a CI gate where a gate silently not running would let a violation through.

## Other useful flags

| Flag | Effect |
|------|--------|
| `--config <path>` | Load a config other than `.hector.yml`. |
| `--gate <id>` | Run only this gate. Repeatable; multiple flags are OR'd. |

For JSON output (`--format json`) and the complete flag list, see the [CLI reference](../reference/cli.md) and [Verdict JSON](../reference/verdict-json.md).

## See also

- [Verdict JSON](../reference/verdict-json.md) — the machine-readable verdict and exit codes
- [Inspecting your config](inspecting-config.md) — read-only commands that never run a gate
- [Diagnostics](diagnostics.md) — `hector doctor` when checks behave unexpectedly
