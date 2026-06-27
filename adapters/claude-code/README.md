# Hector — Claude Code adapter

`PostToolUse` hook integration for Claude Code. Runs `hector check` on every
`Edit` or `Write` tool call, gating the edit against your project's `.hector.yml`
policy before it lands on disk.

> **Note:** this adapter installs via a direct settings patch (see below). The
> `.claude-plugin/` plugin-packaging layout under this directory is kept for
> users who prefer the marketplace plugin workflow, but `hector init` is the
> recommended path.

## Install

```bash
hector init --harness claude-code
```

This auto-detects Claude Code and patches `<project>/.claude/settings.json`
(or `~/.claude/settings.json` with `--global`) to register a `PostToolUse`
hook matching `Edit|Write`. The adapter artifacts are written atomically to
`~/.config/hector/adapters/claude-code/` and a `.hector-adapter.json` sidecar
(per-file sha256 + version) is placed alongside them. A backup of the prior
settings file is saved as `<settings>.bak` on the first write; re-runs are
idempotent (unchanged → "already present", changed artifact → "updated").

Verify the install:

```bash
hector doctor
```

To remove the hook:

```bash
hector init --uninstall --harness claude-code
```

This removes the hook entry, the materialized artifact, and the sidecar from
`~/.config/hector/adapters/claude-code/`. Your `.hector.yml` and trust store
are untouched.

## Manual fallback

Use these steps if the `hector` binary is not available:

1. Install the `hector` binary (`cargo install hector`, or use a release binary).
2. Add this plugin via your Claude Code plugin manager.
3. Run `hector init` in a project to scaffold `.hector.yml`.
4. Review the config and run `hector trust` to fingerprint it.
5. Edit any file — the PostToolUse hook will gate edits against the rules.

## Requirements

- `hector` binary on PATH.
- `bash`, `jq`, `awk` on PATH (required at hook runtime).

## How the hooks resolve

`hooks/hooks.json` dispatches the PostToolUse event to `"${CLAUDE_PLUGIN_ROOT}/hooks/hook.sh"`.
`CLAUDE_PLUGIN_ROOT` is set by Claude Code at hook-fire time and points to the
plugin's installed directory (wherever the plugin manager unpacked this adapter).
You do **not** set it yourself.

If a hook fails with `hook.sh: No such file or directory`, the plugin is not
installed where Claude Code expects. Reinstall via `hector init --harness claude-code`
or, for local development, symlink this directory under your plugins root. See
[`docs/adapters/claude-code.md`](../../docs/adapters/claude-code.md) for full
install paths and diagnostic steps.
