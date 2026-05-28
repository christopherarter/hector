# Checking a whole edit session

The `session` engine evaluates every edit an agent made across a session together, not one file at a time. Use it for problems that only exist in aggregate — a function renamed in one file but still called from another, a config key added without the code that reads it.

```yaml
llm:
  provider: anthropic
  model: claude-sonnet-4-6
  api_key_env: ANTHROPIC_API_KEY

rules:
  no-orphaned-callers:
    description: >
      If a function or method was renamed or removed in this session, no
      edit may leave a call to the old name behind.
    engine: session
    scope: ["src/**/*"]
    severity: error
```

A per-file rule can't catch this: each file looks fine on its own. The `session` engine sees all the edits at once and can reason across them.

## How a session accumulates

Hector records each edit an agent makes into `.hector/session.json` as the session runs. Your adapter does this for you — the Claude Code and OpenCode adapters record on every edit and trigger the session check when the agent finishes a turn.

When the session check runs, Hector frames every recorded edit into one aggregate (file path, timestamp, and diff per edit), sends it to the LLM along with your session rules, and returns a verdict over the whole batch. A session rule reports at most one violation per rule.

To run it yourself:

```bash
hector check --session
```

This evaluates the `engine: session` rules against the accumulated `.hector/session.json`.

## Scope still applies

A session rule's `scope:` filters which edits are included in the aggregate. A rule scoped to `src/**/*` only sees edits to files under `src/`. Edits outside scope are left out of the framing.

## It needs an `llm:` block

Like `semantic`, the `session` engine sends evidence to an LLM, so it requires a configured `llm:` block. Without one, a session rule errors at evaluation. See [LLM providers](../configuring/llm-providers.md).

## Cost

One session check is one LLM call covering all in-scope edits — cheaper per-edit than a semantic rule that fires on every file, but it still costs an API call and can't run offline (except with local Ollama). Use session rules only for cross-edit policies; anything checkable on a single file belongs in a `script`, `ast`, or `semantic` rule.

## See also

- [Asking an LLM to judge a change](asking-an-llm.md) — the per-file LLM engine
- [Adapters overview](../adapters/README.md) — how edits get recorded into a session
- [Telemetry](../operating/telemetry.md) — the session and check records Hector writes
