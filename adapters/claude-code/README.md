# Hector — Claude Code adapter

`/plugin install` integration for Claude Code. Provides:

- `PostToolUse` hook: runs `hector check` on every `Edit` or `Write` tool call, gating the edit against the project's `script` + `ast` rules.
- Skills: `/hector-init`, `/hector-author`, `/hector-review`.

## Install

1. Install the `hector` binary (`cargo install hector`, or use a release binary).
2. Add this plugin via your Claude Code plugin manager.
3. Run `/hector-init` in a project to scaffold `.hector.yml`.
4. Review the config and run `hector trust` to fingerprint it.
5. Edit any file — the PostToolUse hook will gate edits against the rules.

## Requirements

- `hector` binary on PATH.
- `jq` on PATH (parse PostToolUse event payloads).
- `bash` (the hook script is bash).

## How the hooks resolve

`hooks/hooks.json` dispatches the PostToolUse event to `"${CLAUDE_PLUGIN_ROOT}/hooks/hook.sh"`.
`CLAUDE_PLUGIN_ROOT` is set by Claude Code at hook-fire time and points to the
plugin's installed directory (wherever the plugin manager unpacked this adapter).
You do **not** set it yourself.

If a hook fails with `hook.sh: No such file or directory`, the plugin is not
installed where Claude Code expects. Reinstall with `/plugin install` or, for
local development, symlink this directory under your plugins root. See
[`docs/adapters/claude-code.md`](../../docs/adapters/claude-code.md) for full
install paths and diagnostic steps.
