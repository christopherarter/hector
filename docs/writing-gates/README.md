# Anatomy of a gate

A gate is one policy: something your agent must not do, plus a command that detects it. You write gates under the `gates:` map in `.hector.yml`, keyed by a gate id you choose.

Here is a complete gate:

```yaml
gates:
  no-console:
    files: "src/**/*.ts"
    run: "! grep -nH 'console.log' \"$HECTOR_FILE\" || exit 2"
```

That's the whole surface. A gate has exactly two fields:

| Field | What it does |
|-------|--------------|
| `files` | The glob (or list of globs) selecting which files this gate watches. A bare pattern without `/` matches at any depth — `*.ts` is the same as `**/*.ts`. See [Targeting files](../configuring/targeting-files.md). |
| `run` | A shell command, handed to `sh -c` verbatim. Hector reads only its exit code. |

## The exit-code contract

Hector runs `run` once per matching file and looks at nothing but the exit code:

| Exit code | Outcome |
|-----------|---------|
| `2` | **Block** — the edit is rejected. |
| `0`, `1`, `3`–`125` | **Pass** — the edit is allowed. |
| `126` / `127` / killed by signal / timeout | **Internal error** — the gate is broken; it's reported, never a silent pass. |

Blocking is opt-in. A tool that exits `1` when it finds problems — `grep`, `eslint`, `phpstan` — is a **pass** to Hector unless your `run` remaps that to `exit 2`. This is the one thing to internalize: a gate must deliberately `exit 2` to block. The common shapes:

```sh
phpstan analyse "$HECTOR_FILE" || exit 2          # block if the tool fails
grep -q 'TODO' "$HECTOR_FILE" && exit 2 || true   # block if a pattern is present
! grep -q 'TODO' "$HECTOR_FILE" || exit 2         # same, in negated form
```

## What the gate's output becomes

When a gate exits `2`, Hector takes its combined stdout and stderr, trims them, and uses that verbatim as the block message the agent sees. Print whatever helps the agent fix the problem — a `file:line`, the failing rule, a suggested change. If the gate prints nothing, the message is `"<gate-id> blocked"`.

## The ABI: what every gate receives

Hector hands each gate the same four things. Nothing is spliced into the command text, so a path with spaces or shell metacharacters can't break out.

| Channel | Value |
|---------|-------|
| `$HECTOR_FILE` | Absolute path to the file under check. |
| `$HECTOR_ROOT` | Project root — also the gate's working directory. |
| `$HECTOR_EVENT` | What triggered the check: `edit`, `write`, `pre-commit`, or `manual`. |
| stdin | The proposed post-edit content of the file (may be empty). |

There is no `{file}` token. The path travels only as `$HECTOR_FILE`.

### Reading the proposed edit vs. the file on disk

This is the one subtlety worth understanding. When an adapter gates an edit *before* it lands, the new bytes arrive on **stdin**, while `$HECTOR_FILE` may still point at the old on-disk content. So:

- To check the **proposed** content, read stdin — `biome check --stdin-file-path "$HECTOR_FILE"`.
- To check the **on-disk tree** — a tool that needs a real, consistent file tree, like a dependency-graph check — ignore stdin and read `$HECTOR_FILE` or scan `$HECTOR_ROOT`.

Pick the one your tool needs. [Gate recipes](recipes.md) shows both.

## Inline command or script file

`run` can be an inline command (as above) or a path to a script:

```yaml
gates:
  biome:
    files: ["src/**/*.ts", "src/**/*.tsx"]
    run: ".hector/gates/biome.sh"
```

The shell makes no distinction. Keep a one-liner inline; move anything longer into `.hector/gates/` so it's readable and version-controlled. Scripts under `.hector/gates/` are covered by `hector trust`, so editing one re-triggers a blessing.

## See also

- [Gate recipes](recipes.md) — worked gates for common policies
- [Targeting files](../configuring/targeting-files.md) — the `files:` globs
- [Config schema](../reference/config-schema.md) — the exhaustive field reference
