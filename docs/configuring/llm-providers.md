# LLM providers

The `semantic` and `session` engines send changes to an LLM. The `llm:` block in `.hector.yml` says which provider, model, and credentials to use:

```yaml
llm:
  provider: anthropic
  model: claude-sonnet-4-6
  api_key_env: ANTHROPIC_API_KEY
```

One `llm:` block per config serves all LLM rules. Without it, semantic and session rules error at evaluation.

## Fields

| Field | Required | Notes |
|-------|----------|-------|
| `provider` | yes | One of `anthropic`, `openrouter`, `ollama`, `claude-code-subagent`. |
| `model` | yes for direct-API providers | The model id. Optional (and ignored) for `claude-code-subagent`. |
| `api_key_env` | provider-dependent | Name of the environment variable holding the API key. |
| `base_url` | no | Override the provider's default endpoint. |
| `evaluator_model` | no | Only for `claude-code-subagent` — the model the in-session subagent runs under. |

Hector reads the key from the environment at check time — the key itself never goes in the config.

## Anthropic

```yaml
llm:
  provider: anthropic
  model: claude-sonnet-4-6
  api_key_env: ANTHROPIC_API_KEY
```

Calls the Anthropic API directly. Default endpoint `https://api.anthropic.com`; override with `base_url`.

## OpenRouter

```yaml
llm:
  provider: openrouter
  model: anthropic/claude-sonnet-4-6
  api_key_env: OPENROUTER_API_KEY
```

OpenRouter exposes many models behind an OpenAI-compatible API. Default endpoint `https://openrouter.ai/api/v1`; override with `base_url`.

## Ollama (local, no key)

```yaml
llm:
  provider: ollama
  model: llama3.1
```

Talks to a local Ollama server over its OpenAI-compatible endpoint — the one provider that runs offline and needs no API key. Default endpoint `http://localhost:11434/v1`; point `base_url` at a remote Ollama if it lives elsewhere.

## Claude Code subagent

```yaml
llm:
  provider: claude-code-subagent
  evaluator_model: haiku   # optional
```

Instead of calling an API, this defers semantic and session evaluation to a subagent inside the running Claude Code session, which uses the session's own model. There's no `api_key_env` and `model` is ignored — set `evaluator_model` if you want the subagent to run under a specific (e.g. cheaper) model. This path is driven by the Claude Code adapter; see [`--emit-semantic-payload`](../reference/emit-semantic-payload.md).

## When the key is missing

For a direct-API provider, a missing or empty `api_key_env` value doesn't crash Hector — LLM rules surface an internal error (exit `3`), which adapters fail-open on by default. Deterministic rules keep working. Run `hector doctor` to confirm your key resolves before relying on a semantic rule; see [Diagnostics](../operating/diagnostics.md).

## See also

- [Asking an LLM to judge a change](../writing-rules/asking-an-llm.md) — the `semantic` engine
- [Checking a whole edit session](../writing-rules/whole-session-checks.md) — the `session` engine
- [Diagnostics](../operating/diagnostics.md) — verify provider and key wiring
