# Adapters

An adapter wires Hector into a coding agent so policy runs automatically on every edit, instead of you calling `hector check` by hand. The adapter hooks the agent's edit events, runs `hector check`, and translates the exit code into "allow" or "reject this edit."

| Adapter | Agent | Language | Install |
|---------|-------|----------|---------|
| [Claude Code](claude-code.md) | Claude Code | bash + `jq` | `/plugin install` |
| [OpenCode](opencode.md) | OpenCode | TypeScript | File drop or npm |
| [Reasonix](../../adapters/reasonix/README.md) | DeepSeek-Reasonix | bash + `jq` + Python | Settings hook |
| [pi](../../adapters/pi/README.md) | pi | TypeScript | Extension |

*Aider, pre-commit, and MCP adapters are planned.*

## What adapters do

The exact hook names and coverage differ, but the contract is the same across agents:

1. **On each edit or proposed edit** — collapse the host's hook payload into Hector's ABI and run `hector check` against the file. On exit `2`, gating hooks reject the edit so the agent retries.

Every adapter normalizes its host into the same ABI, so one gate command runs unchanged everywhere:

| Channel | Value |
|---------|-------|
| `$HECTOR_FILE` | Absolute path to the file under check. |
| `$HECTOR_ROOT` | Project root — also the gate's working directory. |
| `$HECTOR_EVENT` | `edit`, `write`, `pre-commit`, or `manual`. |
| stdin | The proposed post-edit content. |

The adapter only shells out to the `hector` binary. It doesn't reimplement any policy logic.

## The exit-code contract

Adapters translate [`hector check`'s exit codes](../operating/running-checks.md) into agent actions:

| `hector` exit | Adapter action |
|---------------|----------------|
| `0` (pass) | Allow the edit. |
| `2` (block) | Reject the edit; the agent retries. |
| `1` (config error) | **Fail-open** — log and allow. An unrelated problem, like a broken config, shouldn't block the agent's work. |
| `3` (internal error) | **Fail-open by default** — log and allow. Set `HECTOR_FAIL_CLOSED_ON_INTERNAL=1` to make internal errors block where the host lifecycle can still block. |

The fail-open default on internal errors is deliberate: a rule that *couldn't run* is not a rule that *found a problem*. To make internal errors blocking instead — for a strict CI-style gate — set `HECTOR_FAIL_CLOSED_ON_INTERNAL=1`. See [Running checks](../operating/running-checks.md).

## Requirements

Every adapter needs:

- the `hector` binary on `PATH`,
- a `.hector.yml` in the project root,
- a trusted config (`hector trust`).

If Claude Code hooks aren't firing, run [`hector doctor`](../operating/diagnostics.md) — its `adapter` check confirms Claude Code wiring. Other adapters document their own diagnostics in their adapter pages.

## Managing policy from inside the agent

Adapters that support skills ship three for managing policy without leaving the session:

- **`/hector-init`** scaffolds a `.hector.yml` from your project's stack, migrating checks from existing linters where it can.
- **`/hector-author`** adds, tightens, or removes a gate, and tests it against fixtures before you commit. Reach for it with requests like "ban `unwrap()` in `src/`" or "stop gating `no-debug`."
- **`/hector-review`** reads your telemetry log and reports which gates are noisy, which never fire, and which look dead, so you can prune them.

Claude Code ships all three today; other adapters wire them up as their skill-discovery paths settle.

## See also

- [Claude Code adapter](claude-code.md)
- [OpenCode adapter](opencode.md)
