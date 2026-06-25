# Hector

Policy-enforcement gate for AI coding agents. Rust rewrite of [dynamik-dev/bully](https://github.com/dynamik-dev/bully).

A **gate** is a glob and a shell command. When an agent edits a matching file, Hector runs the command and blocks the edit if it exits `2`:

```yaml
# .hector.yml
gates:
  no-console:
    files: "**/*.ts"
    run: "! grep -nH 'console.log' \"$HECTOR_FILE\" || exit 2"
```

No engines, no severities, no rule DSL — the gate owns the decision through its exit code.

## Status

0.3 "gates" redesign. The core engine, CLI, and the out-of-repo trust store are merged; `hector verify` and the expanded `doctor` are in progress, and the adapter ABI is being aligned to the gate model.

- **CLI:** `check`, `validate`, `init`, `explain`, `show-resolved-config`, `doctor`, `trust`.
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

## Documentation

Full docs are in [`docs/`](docs/README.md) — start with [Getting started](docs/getting-started.md), the [Visual elevator pitch](docs/visual-elevator-pitch.md), or the [Architecture diagram](docs/architecture.md).

## Adapters

Each adapter collapses one harness's edit hook into the gate ABI (`$HECTOR_FILE`, `$HECTOR_ROOT`, `$HECTOR_EVENT`, proposed content on stdin) and runs `hector check`.

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
| 0 | Pass — every matched gate passed |
| 1 | Config or load error — untrusted config, parse failure, missing file |
| 2 | Block — at least one gate exited `2` |
| 3 | InternalError — at least one gate crashed (not found, timeout, killed by signal) |

Adapters fail-open on exit 3 by default. Opt-in fail-closed: `HECTOR_FAIL_CLOSED_ON_INTERNAL=1`.

## Inspect

Read-only commands that never run a gate or write telemetry. Exit `0` on success, `1` on a config error — never `2`.

- `hector explain <file>` — show which gates are in scope for a file and their run commands. `--format human|json`.
- `hector show-resolved-config [--format tsv|yaml|json]` — print the post-`extends:` merged gate set, each gate annotated by the file that defined it. See [docs/reference/show-resolved-config.md](docs/reference/show-resolved-config.md).

Both honor `--config <path>` (default `.hector.yml`).

## Specs

- [`specs/2026-06-15-hector-gates-redesign-design.md`](specs/2026-06-15-hector-gates-redesign-design.md) — the 0.3 gates design (current)
- [`specs/overview.md`](specs/overview.md) — Hector at 1.0
