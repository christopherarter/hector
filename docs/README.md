# Hector documentation

Hector is a policy-enforcement layer for AI coding agents. You write **checks** in a `.hector.yml`; when an agent edits a file, Hector runs the checks that match it and blocks the edits that break your policy.

A check is two fields — the files it watches and a shell command to run:

```yaml
# .hector.yml
checks:
  no-console:
    files: "**/*.ts"
    run: "! grep -n 'console.log'"  # proposed content arrives on stdin
```

Hector runs `run`, reads its exit code, and blocks the edit when the code is nonzero (1–125). That is the whole model — no engines, no severities, no rule DSL. The check owns the decision.

New here? Start with [Getting started](getting-started.md) — you'll have a check blocking a real edit in a few minutes.

Want the big picture first? See the [Visual elevator pitch](visual-elevator-pitch.md), then the [Architecture diagram](architecture.md).

## Writing checks

- [Anatomy of a check](writing-checks/README.md) — `files`, `run`, and the exit-code contract
- [Check recipes](writing-checks/recipes.md) — grep checks, linters over stdin, and whole-tree tools

## Configuring

- [Targeting files](configuring/targeting-files.md) — the `files:` globs each check matches
- [Disabling a check in-line](configuring/disabling.md) — `hector-disable:` directives
- [Sharing config with `extends:`](configuring/inheritance.md) — inherit checks across repos

## Connecting your agent

`hector init` detects your agents and wires the hook for you — start there, then reach for a page below for per-agent paths, scopes, and manual installs.

- [Adapters overview](adapters/README.md) — what an adapter is, the ABI it speaks, and the fail-open contract
- [Claude Code](adapters/claude-code.md)
- [OpenCode](adapters/opencode.md)
- [Reasonix](../adapters/reasonix/README.md)
- [pi](../adapters/pi/README.md)

## Running and inspecting

- [Running checks](operating/running-checks.md) — `hector check`, exit codes, fail-open
- [Inspecting your config](operating/inspecting-config.md) — `explain` and `show-resolved-config`
- [Diagnostics](operating/diagnostics.md) — `hector doctor`
- [Telemetry](operating/telemetry.md) — the `.hector/log.jsonl` check log

## Trust

- [The trust store](security/trust.md) — why Hector won't run an unblessed config, and how `hector trust` works

## Reference

Lookup material. The guides above link here; you don't need to read it front to back.

- [CLI](reference/cli.md) — every command and flag
- [Config schema](reference/config-schema.md) — the full `.hector.yml` shape
- [Verdict JSON](reference/verdict-json.md) — the machine-readable verdict and exit codes
- [`show-resolved-config` output](reference/show-resolved-config.md) — the TSV/YAML/JSON contract
