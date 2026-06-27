# Hector — Reasonix adapter

`PreToolUse` hook integration for [DeepSeek-Reasonix](https://esengine.github.io/DeepSeek-Reasonix/).

Runs `hector check --file <path> --content -` on every `write_file` or `edit_file` call **before** the edit lands on disk. Reasonix's `PreToolUse` is gating — exit 2 refuses the tool call, so a policy violation physically blocks the bad edit instead of just surfacing as a warning. (Reasonix's `PostToolUse` is non-gating; see [`specs/2026-05-25-reasonix-adapter.md`](../../specs/2026-05-25-reasonix-adapter.md) for why a `PostToolUse`-shaped hook would not work.)

## Install

```bash
hector init --harness reasonix
```

This patches `~/.reasonix/settings.json` (Reasonix only supports user-global
scope) to register a `PreToolUse` hook matching `^(write_file|edit_file|multi_edit)$`.
The adapter artifact is written atomically to
`~/.config/hector/adapters/reasonix/hook.sh` and a `.hector-adapter.json` sidecar
(per-file sha256 + version) is placed alongside it. A backup of the prior settings
file is saved as `<settings>.bak` on the first write; re-runs are idempotent
(unchanged → "already present", changed artifact → "updated").

Verify the install:

```bash
hector doctor
```

To remove the hook:

```bash
hector init --uninstall --harness reasonix
```

This removes the hook entry, the materialized artifact, and the sidecar from
`~/.config/hector/adapters/reasonix/`. Your `.hector.yml` and trust store are
untouched.

## Manual fallback

Use these steps if the `hector` binary is not available (e.g., bootstrapping a
fresh machine before you can build):

1. Build / install the `hector` binary:

   ```bash
   cargo install --path . # from the hector repo root
   ```

2. Add the hook to Reasonix's global settings (`~/.reasonix/settings.json`) or a project-local override (`<project>/.reasonix/settings.json`). Project scope takes precedence.

   Copy `hooks/settings.example.json` into the target settings file, merging with any existing `hooks` keys.

3. In each project you want hector to gate, run `hector init && hector trust` to scaffold and fingerprint a `.hector.yml`.

The hook is a silent no-op in any project that lacks `.hector.yml`, so installing globally is safe.

## Requirements

- `hector` on `PATH`
- `jq` on `PATH` (parses the Reasonix stdin payload)
- `bash`

## How it works

| Tool | Source of proposed content | Gating |
| --- | --- | --- |
| `write_file` | `toolArgs.content` (verbatim) | exit 2 blocks |
| `edit_file` | synthesize from `(path, search, replace)`; fail closed if `search` is not unique | exit 2 blocks |
| `multi_edit` | not currently gated (no-op) | follow-up; see spec §9.3 |

Per-edit content reaches hector via stdin (`--content -`), keeping argv free of large payloads. The `--file` path is the real on-disk path so scope globs, baseline matching, and AST language detection all key off the project's actual layout — not a tempfile.

### `engine: script` rules and pre-write gating

`engine: script` rules receive the proposed content on the command's **stdin**. Write the tool's stdin form in `.hector.yml` and the rule gates the proposed edit before it lands on disk — e.g.:

```yaml
script: "biome check --stdin-file-path={file}"
script: "ruff check --stdin-filename {file} -"
script: "eslint --stdin --stdin-filename {file}"
```

`{file}` is a path/extension hint (for config lookup and language detection); the content comes from stdin. A path-only command (`biome check {file}`) still reads the on-disk file and is silently wrong under PreToolUse.

**Per-tool boundary (not per-harness):** stdin-capable single-file tools (biome, eslint, ruff, prettier, shellcheck, …) can gate pre-write. Whole-program tools — tsc, cargo, test runners, anything that needs the full project tree — cannot gate a single proposed file meaningfully; run those post-write or in CI. This boundary is a property of the tool, not of this adapter or Reasonix.

### Limitation: `bash` tool shell-out

A `bash` tool call that writes a file via `cat > foo.ts` (or any shell redirection) does not match `write_file`/`edit_file` and bypasses the hook entirely. This is a known gap; matching `bash` and parsing arbitrary commands is too brittle to attempt here.

## Differences from the Claude Code adapter

| | Claude Code | Reasonix |
| --- | --- | --- |
| Settings file | `hooks/hooks.json` shipped with plugin | `~/.reasonix/settings.json` (user-edited) |
| Plugin root env | `${CLAUDE_PLUGIN_ROOT}` | none — use absolute paths |
| Gating lifecycle event | `PostToolUse` (blocks) | `PreToolUse` (blocks) |
| stdin field for path | `tool_input.file_path` | `toolArgs.path` |
| Edit tool names | `Edit`, `Write` | `edit_file`, `write_file`, `multi_edit` |

Both adapters run per-file `hector check` gates only; hector is a static `script` + `ast` gate.
