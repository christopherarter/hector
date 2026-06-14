---
name: hector
description: Interprets hector PostToolUse hook output after Edit/Write -- fixes blocked-stderr violations.
metadata:
  author: dynamik-dev
  version: 1.0.0
  category: workflow-automation
  tags: [linting, hooks, code-quality, post-tool-use]
---

# Agentic Lint

Interpret and act on hector PostToolUse hook output. Not user-invocable.

## When blocked (hook exited 2)

Tool result stderr begins with a `Verdict` JSON whose `status` is `block`. Format:

```
{
  "status": "block",
  "violations": [
    {"rule_id": "no-debug", "file": "src/foo.rs", "line": 42, "message": "DEBUG marker", "severity": "error"}
  ],
  "passed_checks": ["no-todo"]
}
```

Fix every listed violation in the affected file before any other tool call. The hook re-fires on the next Edit and re-checks. Repeat until clear.

## passed_checks

`passed_checks` lists the rules already verified by the deterministic `script` and `ast` engines for this file. Do not re-investigate their concerns.
