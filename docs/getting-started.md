# Getting started

Hector reads a `.hector.yml` from your repo root and checks each file an agent edits against the checks you define. This page takes you from an empty repo to a check that blocks a real edit.

## Install

Build the binary from source:

```bash
cargo build --release
./target/release/hector --version
```

Put `./target/release/hector` on your `PATH` so the rest of this guide can call `hector` directly. (Prebuilt binaries and a one-line installer are in the [project README](../README.md).)

## Write your first check

Create a `.hector.yml` in your repo root:

```yaml
checks:
  no-debug:
    files: "src/**/*.ts"
    run: "! grep -n 'DEBUG'"
```

A check is two fields. `files` is the glob it watches; `run` is a shell command Hector runs against each matching file. Hector reads only the command's exit code: **any nonzero exit (1–125) blocks the edit**, `0` lets it through.

The `run` here negates a grep. `grep` exits `0` when it finds `DEBUG`, so `! grep …` succeeds when the proposed content is clean and fails when it isn't, and the nonzero exit blocks the edit. Grep reads the proposed post-edit content from stdin; `$HECTOR_FILE` carries the path under check — there is no `{file}` templating.

## Trust the config

Hector runs the commands in your config, so it refuses to run a config it hasn't been told to trust. Review the file, then bless it:

```bash
hector trust
```

This records a hash of the config and its `.hector/gates/` scripts in `~/.config/hector/trust.json`. Any later edit to either invalidates the hash, and `hector check` refuses to run until you re-bless. See [The trust store](security/trust.md) for why.

## Run a check

Point `hector check` at a file:

```bash
hector check --file src/app.ts
```

If `src/app.ts` contains a `DEBUG` marker, the check exits nonzero, and `hector check` prints the verdict and exits `2`. A clean file exits `0`. Those exit codes are the contract your agent adapter keys off — see [Running checks](operating/running-checks.md) for the full table.

To check the *proposed* content of an edit before it lands on disk, pipe it in:

```bash
printf 'const x = "DEBUG"\n' | hector check --file src/app.ts --content -
```

The content arrives on the check's stdin, so a check can inspect the new bytes without them ever touching disk.

## Scaffold and connect your agent

The blank page is optional, and so is wiring your agent by hand. From a fresh project, one command does both:

```bash
hector init
```

`hector init` detects your stack and writes a starter `.hector.yml` (the same shape as the check above, tuned for Rust, Node, or Python), trusts it for you, then detects your installed agents — Claude Code, Reasonix, pi, OpenCode — and, after you confirm, installs Hector's edit hook into each. From then on the check runs on every edit the agent makes; you never call `hector check` by hand.

Review the generated checks and adjust. If you change the config after init, re-run `hector trust`. Target a single agent with `--harness <name>`, wire all four with `--harness all`, or preview the writes with `--dry-run` — see the [CLI reference](reference/cli.md#hector-init) for every flag and the [adapter docs](adapters/README.md) for per-agent details.

## Where to go next

- [Anatomy of a check](writing-checks/README.md) — `files`, `run`, and the exit-code contract in depth
- [Check recipes](writing-checks/recipes.md) — grep checks, linters over stdin, whole-tree tools
- [Targeting files](configuring/targeting-files.md) — getting your `files:` globs right
- [Adapters overview](adapters/README.md) — wiring Hector into your coding agent
