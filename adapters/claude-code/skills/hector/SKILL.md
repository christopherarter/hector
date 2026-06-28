---
name: hector
description: Interprets hector PostToolUse hook output after Edit/Write -- fixes the blocked edits it reports.
metadata:
  author: dynamik-dev
  version: 2.0.0
  category: workflow-automation
  tags: [linting, hooks, code-quality, post-tool-use]
---

# Agentic Lint

Interpret and act on hector PostToolUse hook output. Not user-invocable.

## When blocked (hook exited 2)

The tool-result stderr is a `Verdict` JSON whose `status` is `block`. A gate is a
shell command; it blocked because it exited `2`. Shape:

```json
{
  "schema_version": 4,
  "status": "block",
  "blocks": [
    {"gate": "no-debug", "file": "src/foo.rs", "message": "src/foo.rs:42: DEBUG marker"}
  ],
  "errors": [],
  "passed": ["no-todo"]
}
```

Each entry in `blocks` is one gate that rejected the edit:

- `gate` — the gate id that blocked (defined in `.hector.yml`).
- `file` — the file it checked.
- `message` — the gate's own combined output, verbatim. This is your instruction
  for what to fix; if the gate emits line numbers (e.g. a `grep -n` or a linter),
  they're in here.

Fix every entry in `blocks` in the named file before any other tool call. The
hook re-fires on the next Edit and re-checks. Repeat until `blocks` is empty.

## passed

`passed` lists the gate ids that ran and passed for this file. Their concerns are
already satisfied — don't re-investigate them.

## errors

`errors` lists gates that *couldn't run* (not found, timed out, or killed) — each
is `{gate, file, reason}`, not a policy violation. By default the hook fails open
on these (the edit is allowed and nothing reaches you); you'll only see an error
note when the project set `HECTOR_FAIL_CLOSED_ON_INTERNAL=1`. A gate that couldn't
run is a broken gate, not a finding — surface it, don't try to satisfy it.
