# Getting started

Hector reads a `.hector.yml` from your repo root and checks each file an agent edits against the gates you define. This page takes you from an empty repo to a gate that blocks a real edit.

## Install

Build the binary from source:

```bash
cargo build --release
./target/release/hector --version
```

Put `./target/release/hector` on your `PATH` so the rest of this guide can call `hector` directly. (Prebuilt binaries and a one-line installer are in the [project README](../README.md).)

## Write your first gate

Create a `.hector.yml` in your repo root:

```yaml
gates:
  no-debug:
    files: "src/**/*.ts"
    run: "! grep -nH 'DEBUG' \"$HECTOR_FILE\" || exit 2"
```

A gate is two fields. `files` is the glob it watches; `run` is a shell command Hector runs against each matching file. Hector reads only the command's exit code: **exit `2` blocks the edit**, anything else lets it through.

The `run` here negates a grep. `grep` exits `0` when it finds `DEBUG`, so `! grep …` succeeds when the file is clean and fails when it isn't, and `|| exit 2` turns that failure into a block. The path under check arrives as `$HECTOR_FILE` — there is no `{file}` templating.

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

If `src/app.ts` contains a `DEBUG` marker, the gate exits `2`, and `hector check` prints the verdict and exits `2`. A clean file exits `0`. Those exit codes are the contract your agent adapter keys off — see [Running checks](operating/running-checks.md) for the full table.

To check the *proposed* content of an edit before it lands on disk, pipe it in:

```bash
printf 'const x = "DEBUG"\n' | hector check --file src/app.ts --content -
```

The content arrives on the gate's stdin, so a gate can inspect the new bytes without them ever touching disk.

## Scaffold instead of hand-writing

To skip the blank page, run:

```bash
hector init
```

It detects your project's stack and writes a starter `.hector.yml`, then blesses it for you. Review it and adjust.

## Where to go next

- [Anatomy of a gate](writing-gates/README.md) — `files`, `run`, and the exit-code contract in depth
- [Gate recipes](writing-gates/recipes.md) — grep checks, linters over stdin, whole-tree tools
- [Targeting files](configuring/targeting-files.md) — getting your `files:` globs right
- [Adapters overview](adapters/README.md) — wiring Hector into your coding agent
