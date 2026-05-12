# Claude Code adapter

The Claude Code adapter ships in this repo at `adapters/claude-code/`. It exposes hector to Claude Code as a `/plugin install` plugin.

## What it does

| Event | Action |
|-------|--------|
| `PostToolUse` (Edit\|Write) | Record the edit in `.hector/session.json`, then run `hector check --file <path>`. Exit 2 on block; the edit is rejected and the agent retries. |
| `Stop` | Run `hector check --session` to evaluate session-engine rules. Cleans up `.hector/session.json`. |
| `SessionStart` | Clear stale `.hector/session.json` from a prior aborted run. |

## What it does NOT do

- No subagent. Bully used a Claude Code subagent for semantic eval; hector calls Anthropic directly. Per `specs/2026-05-11-hector-plan-and-0.1-design.md` §11.
- No `Read`/`Grep`/`Glob` proxying. The adapter only invokes the `hector` binary.

## Install paths

### Marketplace (preferred)

Once published: `/plugin install hector`.

### Local development

```bash
cd /path/to/hector
ln -sf "$(pwd)/adapters/claude-code" ~/.claude/plugins/data/hector
```

Restart Claude Code.

## Requirements

- `hector` binary on PATH (`cargo install hector` or release binary).
- `jq` on PATH.
- bash.

## Skills

- `/hector-init` — scaffold `.hector.yml`.
- `/hector-author` — add or modify a rule, with fixture testing.
- `/hector-review` — audit rule health from telemetry.

## Diagnostic

If hooks aren't firing:

1. Check `${CLAUDE_PLUGIN_ROOT}/hooks/hook.sh` is executable.
2. Check `hector --version` runs on PATH.
3. Check `.hector.yml` is present in the project root.
4. Check `.hector.yml` is trusted (run `hector trust`).
5. Trace: `bash -x adapters/claude-code/hooks/hook.sh post-tool-use < event.json`.
