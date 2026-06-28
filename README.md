# Hector

Local CI for agents. A Rust rewrite of [dynamik-dev/bully](https://github.com/dynamik-dev/bully).

GitHub Actions runs CI in the cloud after you push. Hector runs the same kind of checks locally, on every edit your agent makes, before the code lands — and it can refuse. Any check that exits nonzero stops the write.

```yaml
# .hector.yml
checks:
  no-console:
    files: "**/*.ts"
    run: "! grep -n 'console.log'"    # proposed content arrives on stdin

  lint-and-format:
    files: "src/**/*.py"
    on: [write, pre-commit]           # write: per file; pre-commit: once, with $HECTOR_FILES
    steps:
      - name: ruff
        run: "ruff check --quiet --stdin-filename \"$HECTOR_FILE\" -"
      - name: no-todo
        run: "! grep -n 'TODO' $HECTOR_FILES"
```

No engines, no severities, no DSL — the check owns the decision through its exit code. Nonzero (1–125) blocks; 0 passes.

## Status

0.4 "checks pipeline" redesign merged. The core engine, CLI, trust store, and adapter onboarding are merged; `hector verify` and the expanded `doctor` are planned for a later phase.

- **CLI:** `check`, `validate`, `init`, `explain`, `show-resolved-config`, `doctor`, `trust`, `schema`.
- **Adapters:** Claude Code, OpenCode, Reasonix, pi.

## Install

Prebuilt binaries for macOS (Apple Silicon and Intel), Linux (x86-64), and Windows (x86-64). The installer downloads the right binary, drops it in `~/.cargo/bin`, and puts it on your `PATH` — no Rust toolchain required:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/christopherarter/hector/releases/latest/download/hector-cli-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/christopherarter/hector/releases/latest/download/hector-cli-installer.ps1 | iex"
```

Or build from source (needs a Rust toolchain):

```sh
cargo install --git https://github.com/christopherarter/hector hector-cli
```

Then run `hector --version`.

## Connect your agent

`hector init` takes you from clone to gated in one command. It scaffolds a starter `.hector.yml`, trusts it, then detects your coding agents and wires Hector's edit hook into each — so policy runs on every edit without you calling `hector check` by hand:

```sh
hector init
```

It detects Claude Code, Reasonix, pi, and OpenCode, asks before touching anything, and installs the hook. Target one explicitly, wire them all, or patch your user account instead of the project:

```sh
hector init --harness opencode   # just one agent
hector init --harness all        # every supported agent
hector init --global             # user-level settings, not the project
```

It also installs a `hector-config` authoring skill so the agent knows how to write checks; run `hector schema` to read the format yourself.

`hector doctor` verifies the wiring (one row per agent); `hector init --uninstall --harness <name>` removes it. Per-agent paths, scopes, and manual fallbacks are in the [adapter docs](docs/adapters/README.md).

## Lifecycles

A check fires on `write` (default), `pre-commit`, or both.

| Event | Trigger | stdin | `$HECTOR_FILE` | `$HECTOR_FILES` |
|-------|---------|-------|----------------|-----------------|
| `write` | Every agent edit | proposed content | the edited file | same, single entry |
| `pre-commit` | Once before a commit | empty | not set | all staged files, newline-joined |

Use `on: [write, pre-commit]` to fire at both. No duplication — hector keys by check, not by event. This is the same inversion that separates it from lefthook (see [vs lefthook](#vs-lefthook)).

## ABI

Every check receives:

- `$HECTOR_FILE` — absolute path of the file under check (set for `write`; not set for `pre-commit`).
- `$HECTOR_FILES` — newline-joined list of all files (single entry for `write`; all staged files for `pre-commit`).
- `$HECTOR_ROOT` — project root (the check's cwd).
- `$HECTOR_EVENT` — `write` or `pre-commit`.
- **stdin** — proposed post-edit content (`write`) or empty (`pre-commit`).

Read proposed content from stdin, not from `$HECTOR_FILE`. On harnesses that gate before the write lands (e.g. reasonix, pi), the file on disk still holds the old content.

## Disable a check

Add `# hector-disable: <check-id>` anywhere in a file to suppress that check for the whole file.

## vs lefthook

At the `pre-commit` boundary, hector is a near line-for-line swap for lefthook's gate role:

```
# lefthook.yml                         # .hector.yml
pre-commit:                            checks:
  commands:                              prettier:
    prettier:                              files: '**/*.{ts,css,md}'
      glob: "*.{ts,css,md}"               on: [pre-commit]
      run: prettier --check {staged_files} run: prettier --check $HECTOR_FILES
```

Mapping: `pre-commit:` → `on: [pre-commit]`, `commands.<id>` → `checks.<id>`, `glob:` → `files:`, `{staged_files}` → `$HECTOR_FILES`, `run:` → `run:`. **Absorbed:** the gate role. **Declined:** `parallel`, `stage_fixed`, and the fixer/restager half. **Added:** the `write` lifecycle — lefthook's earliest reach is `pre-commit`, after the agent already wrote the file; hector fires on the write itself.

## Documentation

Full docs are in [`docs/`](docs/README.md) — start with [Getting started](docs/getting-started.md) or the [Architecture diagram](docs/architecture.md).

> Note: the in-depth `docs/` guides are being migrated to the 0.4 checks model and may still show the older `gates:` syntax.

## Adapters

Each adapter collapses one harness's edit hook into the check ABI (`$HECTOR_FILE`, `$HECTOR_ROOT`, `$HECTOR_EVENT`, proposed content on stdin) and runs `hector check`. `hector init` installs whichever of these it detects — the per-adapter pages cover the mechanics, scopes, and manual installs.

- **Claude Code** — `adapters/claude-code/`. PostToolUse hook, plus three skills. See [docs/adapters/claude-code.md](docs/adapters/claude-code.md).
- **OpenCode** — `adapters/opencode/`. `tool.execute.before` gates proposed edits. See [docs/adapters/opencode.md](docs/adapters/opencode.md).
- **Reasonix** — `adapters/reasonix/`. PreToolUse hook for `write_file` / `edit_file`. See [adapters/reasonix/README.md](adapters/reasonix/README.md).
- **pi** — `adapters/pi/`. `tool_call` hook gates proposed edits before they're written. See [adapters/pi/README.md](adapters/pi/README.md).
- *Aider, pre-commit, MCP — planned.*

## Build

```bash
cargo build --release
./target/release/hector --version
```

## Exit codes (`hector check`)

| Code | Meaning |
|------|---------|
| 0 | Pass — every matched check passed |
| 1 | Config or load error — untrusted config, parse failure, missing file |
| 2 | Block — at least one check exited nonzero (1–125) |
| 3 | InternalError — at least one check crashed (not found, timeout, killed by signal) |

Adapters fail-open on exit 3 by default. Opt-in fail-closed: `HECTOR_FAIL_CLOSED_ON_INTERNAL=1`.

## Inspect

Read-only commands that never run a check or write telemetry. Exit `0` on success, `1` on a config error — never `2`.

- `hector explain <file>` — show which checks are in scope for a file and their run commands. `--format human|json`.
- `hector show-resolved-config [--format tsv|yaml|json]` — print the post-`extends:` merged check set, each check annotated by the file that defined it. See [docs/reference/show-resolved-config.md](docs/reference/show-resolved-config.md).
- `hector schema` — print the check-authoring guide (`hector-config` skill body).

Both honor `--config <path>` (default `.hector.yml`).

## Specs

- [`specs/2026-06-28-hector-checks-pipeline-design.md`](specs/2026-06-28-hector-checks-pipeline-design.md) — the 0.4 checks pipeline design (current)
- [`specs/overview.md`](specs/overview.md) — Hector at 1.0
