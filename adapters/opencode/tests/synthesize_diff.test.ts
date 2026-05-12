import { describe, it, expect } from "bun:test"
import { synthesizeDiff } from "../src/index.ts"

// Regression tests for P1-8 (wrong hunk-header line counts) and P1-9
// (attacker-controlled `+++ b/...` lines in `newString` reframing the diff
// onto a different file). The plugin synthesizes a fake unified diff from
// Edit/Write tool args and pipes it into `hector session record`; if the
// synthesis is wrong, hector's diff parser sees the wrong file or the
// wrong line numbers.

describe("synthesizeDiff", () => {
  it("emits @@ -1 +1 @@ for empty old/single-line new", () => {
    const d = synthesizeDiff("foo.ts", { oldString: "", newString: "x" })
    expect(d).toContain("@@ -1 +1 @@")
  })

  it("emits correct hunk header for multi-line NEW (P1-8)", () => {
    // 2-line old, 3-line new — header must reflect actual counts.
    const d = synthesizeDiff("foo.ts", {
      oldString: "a\nb",
      newString: "x\ny\nz",
    })
    expect(d).toContain("@@ -1,2 +1,3 @@")
  })

  it("emits correct hunk header for multi-line OLD with single-line NEW (P1-8)", () => {
    const d = synthesizeDiff("foo.ts", {
      oldString: "a\nb\nc",
      newString: "x",
    })
    expect(d).toContain("@@ -1,3 +1 @@")
  })

  it("emits zero-count side when old is empty (write tool / content arg)", () => {
    const d = synthesizeDiff("foo.ts", { content: "x\ny" })
    // Empty `old` means a pure addition; the `-` deletion block must be
    // omitted entirely (no lines starting with a single `-` followed by
    // content — only the `--- a/foo.ts` header is allowed).
    expect(d).toContain("@@ -1 +1,2 @@")
    expect(d).not.toMatch(/^-[^-]/m)
  })

  it("escapes embedded `+++ b/` headers in NEW (P1-9)", () => {
    const evil = "x\n--- a/SECRET\n+++ b/SECRET\n@@ -1 +1 @@\n+pwn"
    const d = synthesizeDiff("foo.ts", { oldString: "", newString: evil })
    // After scrubbing, embedded headers must not appear as real diff headers.
    expect(d).not.toMatch(/^\+\+\+ b\/SECRET$/m)
    expect(d).not.toMatch(/^--- a\/SECRET$/m)
    expect(d).not.toMatch(/^@@ -1 \+1 @@$/m)
    // The real header for the real file must still be present.
    expect(d).toContain("+++ b/foo.ts")
    expect(d).toContain("--- a/foo.ts")
  })

  it("escapes embedded headers in OLD as well", () => {
    // OLD lines become `-` lines; without scrubbing, `-` plus `-- a/SECRET`
    // becomes `--- a/SECRET` — exactly the legacy file header.
    const d = synthesizeDiff("foo.ts", {
      oldString: "-- a/SECRET",
      newString: "x",
    })
    // The synthesized "-" prefix on the old line must not collide with a
    // header. The scrub puts a backslash before the OLD line so the result
    // is "-\\-- a/SECRET", which does NOT start with "--- ".
    expect(d).not.toMatch(/^--- a\/SECRET$/m)
  })
})
