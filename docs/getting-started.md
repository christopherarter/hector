# Getting started

Hector reads a `.hector.yml` from your repo root, then checks files against the rules you define. This page takes you from an empty repo to a rule that blocks a real edit.

## Install

Build the binary from source:

```bash
cargo build --release
./target/release/hector --version
```

Put `./target/release/hector` on your `PATH` so the rest of this guide can call `hector` directly.

## Write your first rule

Create a `.hector.yml` in your repo root:

```yaml
schema_version: 2

rules:
  no-debug:
    description: "no DEBUG markers in source"
    engine: script
    scope: ["src/**/*"]
    severity: error
    script: "grep -nE 'DEBUG' {file} && exit 1 || exit 0"
```

This is a `script` rule: Hector runs the shell command against each file in `scope`, and a non-zero exit means the rule fired. The `{file}` token expands to the path under check. `severity: error` makes a hit a hard block.

## Trust the config

Hector runs the scripts in your config, so it refuses to run one it hasn't seen. Review the file, then sign it:

```bash
hector trust
```

This writes a fingerprint into the `trust:` block. Any later edit to the config invalidates the fingerprint, and `hector check` will refuse to run until you re-sign. See [The trust gate](security/trust.md) for why.

## Run a check

Point `hector check` at a file:

```bash
hector check --file src/foo.rs
```

If `src/foo.rs` contains a `DEBUG` marker, the command prints a verdict and exits `2`. A clean file exits `0`. Those exit codes are the contract your agent adapter keys off — see [Running checks](operating/running-checks.md) for the full table.

## Scaffold instead of hand-writing

To skip the blank page, run:

```bash
hector init
```

It detects your stack (Rust, Node, Python) and writes a starter `.hector.yml`. Review it, then run `hector trust`.

## Add an LLM rule

Some policies can't be expressed as a grep or an AST pattern — "don't log secrets", "this comment doesn't match the code". Those need a `semantic` rule, which sends the change to an LLM. Add an `llm:` block:

```yaml
llm:
  provider: anthropic
  model: claude-sonnet-4-6
  api_key_env: ANTHROPIC_API_KEY
```

Then write `engine: semantic` (or `engine: session`) rules. Hector reads the key from the named environment variable at check time. See [Asking an LLM to judge a change](writing-rules/asking-an-llm.md) and [LLM providers](configuring/llm-providers.md).

## Where to go next

- [Rules overview](writing-rules/README.md) — pick the right engine for each policy
- [Targeting files](configuring/targeting-files.md) — get `scope:` globs right
- [Adapters overview](adapters/README.md) — wire Hector into your coding agent
