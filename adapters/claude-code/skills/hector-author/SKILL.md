---
name: hector-author
description: Authors, modifies, or removes rules in .hector.yml. Use when the user says "add a hector rule for X", "ban Y", "tighten <rule-id>", "make <rule-id> a warning", "convert <rule-id> to semantic", "remove <rule-id>", "change the scope of <rule-id>", or asks to apply recommendations from /hector-review.
metadata:
  author: dynamik-dev
  version: 0.1.0
  category: workflow-automation
  tags: [linting, rule-authoring, config-editing]
---

# Hector Author

Interactive authoring for `.hector.yml`. Every proposed rule is tested against a fixture before being written.

If no `.hector.yml` exists, stop and tell the user to run `/hector-init` first.

## Triggers

The user wants to:
- Add a new rule ("ban X", "warn on Y").
- Modify an existing rule (tighten, change scope, change severity).
- Remove a rule ("drop X").
- Convert a rule between engines (script ↔ ast ↔ semantic).

## Engine routing

Pick the engine based on what the rule needs to detect:

| Rule shape | Engine |
|---|---|
| "Run this linter command" | `script` |
| "Match this AST shape exactly" (e.g., `as any`, `eval(...)`) | `ast` |
| "Code violates this plain-English policy" | `semantic` |
| "These changes need test changes in the same session" | `session` |

## Process

1. Read `.hector.yml` to see existing rules.
2. Draft the new rule with appropriate engine + scope + severity.
3. Build a fixture file that exercises the rule (a clean version + a dirty version).
4. Run hector against the fixture:
   ```bash
   hector check --file /tmp/dirty.<ext>
   ```
5. Verify the rule fires on dirty input and passes on clean input.
6. If the test passes, write the rule into `.hector.yml`.
7. Run `hector trust` to update the fingerprint.

## Required fields by engine

- `script`: `description`, `engine: script`, `scope`, `severity`, `script` (with `{file}` substitution).
- `ast`: `description`, `engine: ast`, `scope`, `severity`, `pattern`, `language`.
- `semantic`: `description` (= the policy in plain English), `engine: semantic`, `scope`, `severity`. Requires `llm:` block in config.
- `session`: same as semantic, plus `context: repo` is recommended.

## Capabilities (script + ast)

By default, all script rules run with `network: false, writes: none`. If a rule needs to write within the project, add:

```yaml
capabilities:
  network: false
  writes: cwd-only
```

Don't grant `network: true` or `writes: unrestricted` unless the user explicitly asks.

## Test before write

Always test the rule against a fixture BEFORE writing to `.hector.yml`. A rule that doesn't fire on dirty input is worse than no rule — it gives false confidence.
