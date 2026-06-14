# Claude Code adapter

The Claude Code adapter runs your Hector rules every time Claude edits a file. When an edit breaks a rule, Claude Code rejects it on the spot, hands Claude the verdict, and Claude rewrites the change to comply. You stop having to remember to run `hector check` yourself; the gate is always on.

The adapter ships in this repo at `adapters/claude-code/`.

## Install the plugin

You need the `hector` binary and `jq` on your `PATH` first. Build Hector and check both are reachable:

```bash
cargo build --release   # produces ./target/release/hector
hector --version
jq --version
```

Then link the adapter into Claude Code's plugin directory and restart so it loads:

```bash
ln -sf "$(pwd)/adapters/claude-code" ~/.claude/plugins/data/hector
```

Once Hector is published to the plugin marketplace you can skip the symlink and run `/plugin install hector` instead.

## Set up a project

The adapter stays silent in any project that has no `.hector.yml`, so installing it globally is safe. To start gating a project, scaffold a config and sign it.

Run `/hector-init` in the project. Claude detects your stack (Rust, Node, Python) and writes a starter `.hector.yml`. Review the rules it generated, then trust the file:

```bash
hector trust
```

Hector runs the scripts in your config, so it refuses to run one it hasn't seen. `hector trust` writes a fingerprint into the config; any later edit invalidates it and you re-sign. See [The trust gate](../security/trust.md) for why.

## Watch it block an edit

Here is the whole point of the adapter, end to end. Suppose your `.hector.yml` bans `DEBUG` markers in source:

```yaml
rules:
  no-debug:
    description: "no DEBUG markers in source"
    engine: script
    scope: ["src/**/*"]
    severity: error
    script: "grep -nE 'DEBUG' {file} && exit 1 || exit 0"
```

Ask Claude to add a debug print to a file under `src/`. The instant Claude writes the edit, the adapter runs `hector check` against that file, the `no-debug` rule fires, and Claude Code rejects the edit. Claude reads the returned block message — a plain-text summary naming the rule and the tool's own complaint — sees that it broke `no-debug`, and rewrites the change without the marker. The retry happens in the transcript while you watch; you never touched the keyboard.

A clean edit, one that breaks no rule, lands normally and you see nothing at all. That silence is the adapter working.

## What runs, and when

Every adapter follows the [same lifecycle](README.md#what-adapters-do); here is how Claude Code wires it:

**After every edit.** When Claude finishes an `Edit` or `Write`, the adapter runs `hector check --file <path>`. A block rejects the edit and Claude retries. This is the gate you saw above.

## Author and review rules from inside Claude

The adapter ships the three standard policy skills — `/hector-init`, `/hector-author`, and `/hector-review`. See [Managing policy from inside the agent](README.md#managing-policy-from-inside-the-agent) for what each does.

## When edits aren't being gated

If Claude edits a file and nothing happens, walk through these in order:

1. Confirm the plugin landed where Claude Code expects it. The hook lives at `${CLAUDE_PLUGIN_ROOT}/hooks/hook.sh` and must be executable. If you see `hook.sh: No such file or directory`, reinstall or re-create the symlink above.
2. Confirm `hector --version` runs on your `PATH`.
3. Confirm `.hector.yml` exists in the project root.
4. Confirm the config is trusted by running `hector trust`.
5. Trace a single event end to end: `bash -x adapters/claude-code/hooks/hook.sh post-tool-use < event.json`.

For a one-shot health check, run [`hector doctor`](../operating/diagnostics.md). Its `adapter` check confirms the wiring without you tracing anything by hand.

## How it works

The adapter is one bash script that Claude Code calls on `PostToolUse` (matching `Edit` \| `Write`). It only ever shells out to the `hector` binary and holds no policy logic of its own, so changing a rule never means touching the adapter. It translates `hector check`'s exit codes into allow/reject per [the exit-code contract](README.md#the-exit-code-contract). The adapter gates edits and nothing else — it does not proxy Claude's `Read`, `Grep`, or `Glob` tools.

## See also

- [Adapters overview](README.md) — the fail-open contract every adapter shares
- [Running checks](../operating/running-checks.md) — the exit codes the adapter keys off
