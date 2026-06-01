# Idea: deferred ("eventual") gating for pre-write-only harnesses

**Date:** 2026-05-29
**Status:** Idea — not scheduled. Captured for future consideration.
**Relates to:** [`specs/2026-05-29-script-engine-prewrite-content.md`](./2026-05-29-script-engine-prewrite-content.md) (the pre-write content problem and the stdin primitive)

---

## The idea

Stop trying to evaluate proposed content before it lands. Instead, **let the edit land, evaluate it post-write (where the file is real on disk and every engine — including `script` — works), and if it should have been blocked, enforce on the *next* gating opportunity.**

Splits the problem into two halves that each play to a harness's strengths:
- **Detection** → a non-gating post-write hook (`PostToolUse`), where on-disk content is accurate.
- **Enforcement** → the next pre-write hook (`PreToolUse`), plus a session-end sweep (`Stop`).

## Mechanism

1. **PostToolUse (detect):** run `hector check --file <landed-file>`. On a block-severity verdict, append a fingerprint to a persisted queue (e.g. `.hector/pending-blocks.json`).
2. **PreToolUse (enforce):** before allowing the next tool call, if the queue is non-empty, return exit 2 with the prior violation ("previous edit to X violated Y — revert/fix before continuing").
3. **Stop (sweep):** at session end, evaluate the queue for anything with no subsequent write; surface it.

## Where it earns its place

The genuine niche is **tree-bound / whole-program tools** (`tsc`, `cargo check`, `go vet`, test runners) — the tools that *cannot* be gated pre-write at all because their verdict depends on files other than the one being edited (see the companion brief, §7). For those, the stdin primitive can't help; "land → detect post-write → block next action" is one of the only enforcement options. **This complements pre-write stdin gating; it does not replace it.**

## Why it is NOT a general replacement

- **The bad edit lands.** Hector's core promise is *physical prevention* — the violation never enters the tree. This admits it, then trips later. In the window, build/watchers/dev-server/other agents see the bad code.
- **The terminal edit can be unblockable.** If the bad write is the agent's last action, there's no "next write" to block. The `Stop` sweep only helps if `Stop` *gates* — in Reasonix it is **non-gating**, so the most important case (final on-disk state) leaks as a warning.
- **It blocks the wrong edit.** Edit B (possibly a different file) is refused because edit A was bad; the agent must infer it should revert A. The revert is itself an edit the queue would block unless "edits that resolve the queue" are special-cased — which needs pre-write evaluation again.
- **It adds stateful machinery.** Persisted queue, fingerprinting the blocked edit, reconciling whether a later write fixed it, concurrent-session safety. Compare the stdin primitive: ~10 lines, no state.

## Preconditions for it to be viable in a given harness

- A post-write hook that **fires** (even if non-gating) — for detection.
- A pre-write hook that **blocks** — for enforcement.
- Ideally a **gating** `Stop`/session-end event — without it, terminal violations can only be warned, not blocked.

## Open questions

1. How does an enforced block distinguish "an edit that resolves the queue" (allow) from "an unrelated edit" (block) without re-introducing pre-write evaluation?
2. Queue fingerprint granularity — per file? per `rule_id::file::line`? How does it reconcile with the existing `baseline` fingerprint (`rule_id::file::line`) and the `session` engine, which already aggregates a changeset on `Stop`?
3. Is this just the `session` engine with a pre-write tripwire bolted on? If so, scope it as a `session`-engine enforcement mode rather than a new subsystem.
4. UX: what does the agent see, and can it reliably recover (revert the offending edit) inside the harness's read-before-edit rules?
