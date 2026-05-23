# `hector check --emit-semantic-payload`

Adapter-internal flag for the Claude Code subagent path. When set, semantic
and session rules are collected into a `DeferredVerdict` JSON envelope
instead of being dispatched to the configured LLM.

Activated by either:
- `llm.provider: claude-code-subagent` in `.hector.yml` (end-user-facing),
- or the long-only `--emit-semantic-payload` CLI flag (adapter-internal,
  used for explicit invocations and tests).

## Envelope shape

`schema_version: 1`. Independent of `Verdict::SCHEMA_VERSION`.

```json
{
  "schema_version": 1,
  "deferred": true,
  "hector_version": "0.1.0",
  "passed_checks": ["det-rule-1", "det-rule-2"],
  "payload": {
    "file": "src/foo.rs",
    "diff": "@@ -1,1 +1,1 @@\n-old\n+new\n",
    "passed_checks": ["det-rule-1", "det-rule-2"],
    "evaluate": [
      {
        "id": "no-debug",
        "description": "no DEBUG prints in committed code",
        "severity": "error",
        "engine": "semantic"
      }
    ],
    "_evaluator_input": "<TRUSTED_POLICY>…</UNTRUSTED_EVIDENCE>"
  },
  "elapsed_ms": 42
}
```

## Exit-code semantics

| Outcome | Exit code | Stdout |
|---|---|---|
| Deterministic block | `2` | Standard `Verdict` |
| Pass + deferred non-empty | `0` | `DeferredVerdict` envelope |
| Pass + no deferred | `0` | Standard `Verdict` |

Deferred eval is not a block — the verdict is decided later by the
in-session subagent.

## Limitations (0.2.x)

- `--diff` combined with `--emit-semantic-payload` is rejected; multi-file
  envelope aggregation is a follow-up.
- The envelope assumes a single primary file. `engine: session` rules
  that span multiple changed files still produce one envelope; the
  subagent receives every session-rule definition but only the primary
  file/diff.
