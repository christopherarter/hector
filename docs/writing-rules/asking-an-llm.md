# Asking an LLM to judge a change

The `semantic` engine sends a change to an LLM and asks whether it violates the rule. Use it for policies that need judgment — intent, meaning, naming — that a grep or AST pattern can't capture.

```yaml
llm:
  provider: anthropic
  model: claude-sonnet-4-6
  api_key_env: ANTHROPIC_API_KEY

rules:
  no-real-secrets:
    description: "No real credentials in source. Test fixtures and obvious placeholders are fine."
    engine: semantic
    scope: ["src/**/*"]
    severity: error
    context: diff
```

When `hector check` runs this, Hector sends the rule's `description` and the changed code to the LLM, which returns a pass or a violation. The `description` *is* the prompt — write it as a precise instruction, including the edge cases you want allowed, like the fixture exception above.

## You need an `llm:` block

Semantic rules need a configured LLM. Without an `llm:` block, a semantic rule errors at evaluation time. The block names a provider, a model, and the environment variable holding the API key:

```yaml
llm:
  provider: anthropic
  model: claude-sonnet-4-6
  api_key_env: ANTHROPIC_API_KEY
```

Anthropic, OpenRouter, and Ollama (local, no key) are supported, along with a Claude Code subagent mode that runs evaluation inside the agent's own session. See [LLM providers](../configuring/llm-providers.md).

## Control what the LLM sees with `context`

The `context` field decides how much code goes into the prompt:

| `context` | The LLM sees | Use when |
|-----------|--------------|----------|
| `diff` (default) | Only the changed lines | The policy is about the change itself. |
| `file` | The whole file under check | The change only makes sense with surrounding code. |
| `repo` | Broader repository context | The judgment depends on how the change fits the codebase. |

Start with `diff`. It's the cheapest and keeps the model focused on what actually changed; widen to `file` or `repo` only when a rule misfires for lack of context.

## Writing a good description

The `description` is the whole instruction the model acts on. Vague descriptions get vague judgments.

```yaml
# Too vague — the model has to guess what counts.
description: "no bad logging"

# Precise — states the rule and the exceptions.
description: >
  No logging of secrets or PII (passwords, tokens, full credit-card or
  SSN values). Logging a user id or a redacted/last-4 value is allowed.
```

State the rule, then the exceptions. The model treats your description as policy and the code as evidence.

## Cost and offline behavior

Each semantic rule that matches a file is one LLM call. That has consequences:

- It costs money and adds latency per check.
- It can't run offline (except with a local Ollama model).
- A missing API key surfaces as an internal error (exit `3`), which adapters fail-open on by default — the check is skipped, not failed. See [Running checks](../operating/running-checks.md).

Reserve semantic rules for policies a deterministic engine genuinely can't express. A `script` or `ast` rule, where one fits, is faster, free, and offline.

## Debugging the prompt

To see the exact prompt a semantic rule would send without spending an API call, run:

```bash
hector check --file src/auth.rs --print-prompt
```

It renders the prompt for every semantic rule in scope to stdout and exits `0` without calling the LLM.

## Deferred evaluation

In the Claude Code subagent mode, semantic rules aren't sent to a direct API — they're collected into an envelope and evaluated by a subagent inside the running session, using the session's own model. That path is driven by the adapter and documented in [`--emit-semantic-payload`](../reference/emit-semantic-payload.md). You don't need it for direct-API providers.

## See also

- [Checking a whole edit session](whole-session-checks.md) — for policies that span multiple edits
- [LLM providers](../configuring/llm-providers.md) — provider setup and defaults
- [`--emit-semantic-payload`](../reference/emit-semantic-payload.md) — the deferred-evaluation envelope
