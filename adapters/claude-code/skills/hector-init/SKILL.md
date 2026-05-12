---
name: hector-init
description: Bootstraps a project's .hector.yml by detecting the tech stack from manifest files, migrating rules from existing linting tools, and generating a baseline config. Use when user says "init hector", "set up hector", "bootstrap hector config", "create hector rules", "hector init", or asks to create or generate a hector configuration.
metadata:
  author: dynamik-dev
  version: 0.1.0
  category: workflow-automation
  tags: [linting, code-quality, config-generation, stack-detection]
---

# Hector Init

Generate a baseline `.hector.yml` by detecting the stack, wiring installed linters as passthrough rules, and routing project-specific rules to the right engine.

This skill is user-driven. Do not silently install tools or migrate rules. Every step below is a proposal the user accepts or declines.

## Step 1: Run `hector init`

The `hector init` command detects the stack from manifest files:

| Manifest | Stack |
|---|---|
| `Cargo.toml` | Rust |
| `package.json` | Node |
| `pyproject.toml` or `setup.py` | Python |
| (none) | Generic |

```bash
hector init
```

This creates `.hector.yml` with one or two starter rules for the detected stack. Review the output.

## Step 2: Add stack-specific linter passthroughs

For each linter the user has installed (ruff, biome, eslint, tsc, phpstan, clippy, …), propose a passthrough rule:

```yaml
ruff-check:
  description: "Code must pass ruff check."
  engine: script
  scope: ["**/*.py"]
  severity: error
  script: "ruff check --quiet {file}"
```

Test each candidate by running the script against a sample file. Only add rules that succeed without spurious errors.

## Step 3: Trust the config

After review, run:

```bash
hector trust
```

This writes a sha256 fingerprint of the canonicalized config into `.hector.yml`. Future runs verify the fingerprint and refuse to execute rules from an untrusted config.

## Step 4: Verify

Edit any in-scope file. The PostToolUse hook should run hector and either pass (clean) or block (with a violation message).

## Notes

- If `.hector.yml` already exists, this skill should not overwrite it. Suggest `hector migrate` if it's actually `.bully.yml`.
- For semantic rules, the user needs to add an `llm:` block with provider/model/api_key_env. Propose this only if they want plain-English rules.
- Telemetry lands at `.hector/log.jsonl`. The `/hector-review` skill consumes it.
