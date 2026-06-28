# Claude Code adapter

The Claude Code adapter runs your Hector gates every time Claude edits a file. When an edit breaks a gate, Claude Code rejects it on the spot, hands Claude the verdict, and Claude rewrites the change to comply. You stop having to remember to run `hector check` yourself; the gate is always on.

The adapter ships in this repo at `adapters/claude-code/`. It predates the 0.3 gates redesign, so its alignment to the gate ABI (`$HECTOR_FILE`, the proposed post-edit content on stdin, exit `2` blocks) is still in progress under Plan 4; the `.hector.yml` gate format shown below is current regardless.

## Install

With the `hector` binary and `jq` on your `PATH`, one command wires the hook and scaffolds a trusted config:

```bash
hector init --harness claude-code
```

This patches `<project>/.claude/settings.json` (or `~/.claude/settings.json` with `--global`) to register a `PostToolUse` hook matching `Edit|Write`, and materializes the hook scripts to `~/.config/hector/adapters/claude-code/` with a `.hector-adapter.json` sidecar (per-file sha256 + version). A backup of the prior settings file is written as `<settings>.bak` on the first patch; re-runs are idempotent. Restart (or reload) Claude Code so it picks up the new hook, then verify:

```bash
hector doctor
```

To remove the hook, its artifacts, and the sidecar (leaving `.hector.yml` and the trust store):

```bash
hector init --uninstall --harness claude-code
```

This settings-hook install gives you the **gate** and installs the `hector-config` authoring skill into `.claude/skills/hector-config/`. `/hector-init` and `/hector-review` ship with the plugin package — see [Author and review gates from inside Claude](#author-and-review-gates-from-inside-claude) below.

If you wrote `.hector.yml` by hand instead of letting `hector init` scaffold it, trust it before checks will run:

```bash
hector trust
```

Hector runs the commands in your config, so it refuses to run one it hasn't seen. `hector trust` records the config in the trust store; any later edit invalidates it and you re-sign. See [The trust store](../security/trust.md) for why.

## Watch it block an edit

Here is the whole point of the adapter, end to end. Suppose your `.hector.yml` bans `DEBUG` markers in TypeScript:

```yaml
# .hector.yml
gates:
  no-debug:
    files: "**/*.ts"
    run: "! grep -nH 'DEBUG' \"$HECTOR_FILE\" || exit 2"
```

Ask Claude to add a `DEBUG` marker to a `.ts` file. The instant Claude writes the edit, the adapter runs `hector check` against that file, the `no-debug` gate exits `2`, and Claude Code rejects the edit. Claude reads the returned block message — the gate's own output — sees that it broke `no-debug`, and rewrites the change without the marker. The retry happens in the transcript while you watch; you never touched the keyboard.

A clean edit, one that breaks no gate, lands normally and you see nothing at all. That silence is the adapter working.

## What runs, and when

Every adapter follows the [same lifecycle](README.md#what-adapters-do); here is how Claude Code wires it:

**After every edit.** When Claude finishes an `Edit` or `Write`, the adapter runs `hector check --file <path>`. A block rejects the edit and Claude retries. This is the gate you saw above.

## Author and review gates from inside Claude

`hector init --harness claude-code` installs the **`hector-config`** authoring skill into `.claude/skills/hector-config/` — the gate schema, the exit-code contract, and common patterns with a fixture-test loop. Run `hector schema` any time to print the same guide at the terminal. `/hector-init` and `/hector-review` ship with the Claude Code **plugin** instead (see [Managing policy from inside the agent](README.md#managing-policy-from-inside-the-agent)).

The plugin layout lives in this repo at `adapters/claude-code/`. For local development, link it into Claude Code's plugin directory and restart:

```bash
ln -sf "$(pwd)/adapters/claude-code" ~/.claude/plugins/data/hector
```

Once Hector is published to the plugin marketplace you can skip the symlink and run `/plugin install hector` instead. The plugin registers the same `PostToolUse` gate, so install it *or* run `hector init --harness claude-code` — not both.

## When edits aren't being gated

If Claude edits a file and nothing happens, walk through these in order:

1. Confirm the hook is where Claude Code expects it. For an `init` install, that's the `PostToolUse` entry in `.claude/settings.json` (or `~/.claude/settings.json` with `--global`) pointing at `~/.config/hector/adapters/claude-code/hook.sh`, and that file must be executable. For a plugin install, the hook resolves to `${CLAUDE_PLUGIN_ROOT}/hooks/hook.sh`. A `hook.sh: No such file or directory` means the install didn't land — re-run `hector init --harness claude-code` or re-create the plugin symlink.
2. Confirm `hector --version` runs on your `PATH`.
3. Confirm `.hector.yml` exists in the project root.
4. Confirm the config is trusted (`hector init` does this; otherwise run `hector trust`).
5. Trace a single event end to end: `bash -x adapters/claude-code/hooks/hook.sh post-tool-use < event.json`.

For a one-shot health check, run [`hector doctor`](../operating/diagnostics.md). Its `claude-code` adapter row confirms the wiring without you tracing anything by hand.

## How it works

The adapter is one bash script that Claude Code calls on `PostToolUse` (matching `Edit` \| `Write`). It only ever shells out to the `hector` binary and holds no policy logic of its own, so changing a gate never means touching the adapter. It translates `hector check`'s exit codes into allow/reject per [the exit-code contract](README.md#the-exit-code-contract). The adapter gates edits and nothing else — it does not proxy Claude's `Read`, `Grep`, or `Glob` tools.

## See also

- [Adapters overview](README.md) — the fail-open contract every adapter shares
- [Running checks](../operating/running-checks.md) — the exit codes the adapter keys off
