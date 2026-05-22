# `hector record-verdict`

Adapter-internal subcommand. Appends one `semantic_verdict` record to
`.hector/log.jsonl` so subagent-evaluated rules show up in coverage
reports (`hector coverage`, D2) instead of looking dead.

Consumed by the Claude Code adapter's interpreter skill after it parses
a subagent's pass/violation answer for a deferred semantic or session
rule.

## Synopsis

```
hector record-verdict --rule <id> --verdict <pass|violation> [--file <path>] [--dir <path>]
```

| Flag | Required | Default | Notes |
|---|---|---|---|
| `--rule` | yes | — | Rule id this verdict is for. Single occurrence. |
| `--verdict` | yes | — | Exactly `pass` or `violation`. Other values rejected at clap-parse time. |
| `--file` | no | omitted | File path the verdict pertains to. When absent, the on-disk record has no `file` field. |
| `--dir` | no | `.` | Directory containing `.hector/log.jsonl`. Created if it doesn't exist. |

## Wire format

Appends one line of the form:

```json
{"type":"semantic_verdict","ts":"2026-05-14T12:34:56.789Z","rule":"no-debug","verdict":"violation","file":"src/foo.rs"}
```

(`file` omitted when `--file` is not passed.)

The first invocation against a fresh `.hector/log.jsonl` stamps a
`session_init` record before the `semantic_verdict`. See
[`docs/telemetry.md`](./telemetry.md) for the full wire-format reference.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Record appended successfully. |
| `1` | Telemetry write failure (disk full, permissions, parent directory unwritable). Stderr carries the io::Error. |
| (`2`) | clap parse error — invalid `--verdict` value, missing `--rule`, etc. Not returned by our code. |

`record-verdict` is **not** a gate. It never returns `2` from our code
and the adapter does not treat a non-zero exit as a verdict signal — it
logs the failure and moves on.

## Trust model

None. No HMAC, no nonce, no signing. An attacker who can run
`hector record-verdict` can also write to `.hector/log.jsonl` directly.
The subcommand is convenience, not security. See `docs/security.md`
for the project's overall trust model.
