# pi adapter (tool_call gate) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `adapters/pi/` — a pi (`@earendil-works/pi-coding-agent`) extension that wires pi's `tool_call` / `tool_result` / `session_start` / `agent_end` lifecycle events to the `ironlint` CLI, gating file edits against project policy before they execute.

**Architecture:** A single TypeScript module (`src/index.ts`) default-exports a factory `(pi) => void` that registers four lifecycle handlers via `pi.on(...)`. The handlers are a pure translation layer — they compute the proposed post-edit content, shell out to the `ironlint` binary (`ironlint check --file <path> --content -` with the content piped on stdin), and translate ironlint's exit code into pi's block contract (`return { block: true, reason }`). No rule logic lives in the adapter. It mirrors the existing `adapters/opencode/` plugin feature-for-feature, adapted to pi's API, using the `--content -` pre-write check from `adapters/reasonix/`.

**Tech Stack:** TypeScript (loaded by pi via `jiti`, no build step), Node built-ins (`node:child_process`, `node:fs`, `node:path`), `node:test` test runner (zero extra runtime deps), the compiled `ironlint` Rust binary on `PATH`.

---

## Reference material (read before starting)

- **Design spec:** `docs/superpowers/specs/2026-05-28-pi-adapter-design.md` — the authoritative behavior contract. Every section number referenced below (§5, §6.1, etc.) points here.
- **Feature-parity reference:** `adapters/opencode/src/index.ts` and its tests `adapters/opencode/tests/plugin.test.ts`, `adapters/opencode/tests/synthesize_diff.test.ts`. The pi adapter reproduces this behavior on pi's API. `synthesizeDiff` is ported almost verbatim (extended for batch edits).
- **`--content -` precedent:** `adapters/reasonix/hooks/hook.sh` — the pre-write stdin check pattern.
- **CLI exit-code contract:** `crates/ironlint-cli/src/commands/check.rs` (function `run`, `exit_code`) and `crates/ironlint-cli/src/commands/session.rs` (function `record`). Flags defined in `crates/ironlint-cli/src/cli.rs`.
- **CI wiring reference:** the `opencode-adapter` job in `.github/workflows/ci.yml`.

### Exit-code contract (locked — do not reinterpret)

From `crates/ironlint-cli/src/commands/check.rs`:

| Exit | Meaning | Adapter behavior |
|------|---------|------------------|
| `0` | Pass or Warn | Allow (return nothing). |
| `2` | Block (≥1 error-severity violation) | `return { block: true, reason }`. |
| `3` | Engine internal error (missing API key, script spawn failure, AST refused diff) | Fail-open (log + allow) by default; fail-closed (block) when `IRONLINT_FAIL_CLOSED_ON_INTERNAL=1`. |
| `1` / other | Config/load error | Log to stderr, allow. |

---

## File Structure

| File | Responsibility |
|------|----------------|
| `adapters/pi/src/index.ts` | The whole adapter: type definitions, pure helpers (`normalizeEdits`, `synthesizeDiff`, `computeProposedContent`, `getPath`, `isPolicyFile`), the `ironlint` subprocess wrapper (`runIronLint`), and the default-exported factory that registers the four `pi.on(...)` handlers. Single focused file (~250 lines), matching the opencode adapter's single-file shape. |
| `adapters/pi/test/index.test.ts` | `node:test` suite. Drives the exported factory with a fake `pi` object and synthetic events, plus unit tests for the pure helpers, against the real `ironlint` binary on `PATH`. |
| `adapters/pi/package.json` | `@christopherarter/ironlint-pi`. `type: module`, `pi.extensions` discovery field, `test`/`typecheck` scripts, `@types/node` + `typescript` dev deps. |
| `adapters/pi/tsconfig.json` | Strict TS config mirroring opencode's, with `types: ["node"]`. |
| `adapters/pi/README.md` | Install paths, requirements, exit-code table, known gaps (§12, §13). |
| `.github/workflows/ci.yml` | Add a `pi-adapter` job mirroring `opencode-adapter`: download the built binary, set up Node, type-check, run the `node:test` suite with `ironlint` on `PATH`. |

**Design note on types:** the adapter source defines its own structural `PiExtensionAPI` interface rather than importing `@earendil-works/pi-coding-agent`. This keeps the adapter a zero-hard-dependency translation layer — `tsc` and `node:test` run without the pi package installed, and tests inject a fake `pi`. The pi package is declared as an *optional peer dependency* for documentation only (the real runtime types come from the host).

---

## Prerequisites (do once, before Task 1)

- [ ] **Build the `ironlint` binary** so the integration tests can invoke it:

```bash
cargo build --release
```

Expected: `./target/release/ironlint` exists.

- [ ] **Confirm Node ≥ 22.6** (for `node:test` + native TypeScript type-stripping):

```bash
node --version
```

Expected: `v22.6.0` or higher (type-stripping via `--experimental-strip-types` requires ≥ 22.6).

- [ ] **Run integration tests with the binary on PATH.** Every test command in this plan assumes:

```bash
export PATH="$(pwd)/target/release:${PATH}"
ironlint --version   # sanity: prints a version
```

---

## Task 1: Scaffold the adapter + resolve open questions

**Files:**
- Create: `adapters/pi/package.json`
- Create: `adapters/pi/tsconfig.json`
- Create: `adapters/pi/test/smoke.test.ts` (temporary — deleted in Task 2)

### Investigation (resolves spec §14 and the project-root question)

- [ ] **Step 1: Resolve spec §14 — does `pi.exec` support stdin + exit code?** Read `pi.exec`'s signature in the pi source (`earendil-works/pi`, `packages/coding-agent/src/`) or `pi.dev/docs/latest/extensions`.

  - **Decision rule:** This plan uses `node:child_process` `spawnSync` (deterministic stdin via `{ input }` + exit code via `.status`) regardless. `spawnSync` is the guaranteed-correct path and is what every code step below is written against. *If* the investigation proves `pi.exec` accepts stdin input **and** surfaces an exit code, you may later swap `runIronLint`'s internals to gain `ctx.signal` abort integration — but that is an optional enhancement, not required for v1. Do not block on it.

- [ ] **Step 2: Confirm how the extension learns the project root.** Read how pi passes the working directory to extensions (factory arg field, a `pi.cwd` property, or the per-handler `ctx`).

  - **Decision rule:** This plan resolves the root as `pi.cwd ?? pi.directory ?? process.cwd()`. `process.cwd()` is the correct fallback for a terminal agent (pi is launched from the project root). If pi exposes the cwd under a different field name, add it to the `??` chain in `resolveRoot` (defined in Task 4). Tests inject `cwd` on the fake `pi`, so the fallback chain is what matters for production.

### Scaffold

- [ ] **Step 3: Create `package.json`:**

```json
{
  "name": "@christopherarter/ironlint-pi",
  "version": "0.1.0",
  "description": "pi extension for IronLint — policy enforcement for AI coding agents. tool_call gate + session record + session check.",
  "type": "module",
  "main": "src/index.ts",
  "exports": {
    ".": "./src/index.ts"
  },
  "pi": {
    "extensions": ["./src/index.ts"]
  },
  "files": [
    "src",
    "README.md"
  ],
  "engines": {
    "node": ">=22.6"
  },
  "scripts": {
    "typecheck": "tsc --noEmit",
    "test": "node --experimental-strip-types --test test/*.test.ts"
  },
  "peerDependencies": {
    "@earendil-works/pi-coding-agent": "*"
  },
  "peerDependenciesMeta": {
    "@earendil-works/pi-coding-agent": {
      "optional": true
    }
  },
  "devDependencies": {
    "@types/node": "*",
    "typescript": "^5"
  },
  "keywords": [
    "pi",
    "pi-extension",
    "ironlint",
    "lint",
    "policy",
    "ai-agents"
  ],
  "license": "Apache-2.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/christopherarter/ironlint.git",
    "directory": "adapters/pi"
  },
  "homepage": "https://github.com/christopherarter/ironlint#readme",
  "bugs": "https://github.com/christopherarter/ironlint/issues"
}
```

- [ ] **Step 4: Create `tsconfig.json`:**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "lib": ["ES2022"],
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitOverride": true,
    "noFallthroughCasesInSwitch": true,
    "isolatedModules": true,
    "esModuleInterop": true,
    "resolveJsonModule": true,
    "skipLibCheck": true,
    "types": ["node"],
    "noEmit": true,
    "allowImportingTsExtensions": true
  },
  "include": ["src/**/*.ts", "test/**/*.ts"]
}
```

- [ ] **Step 5: Create a temporary smoke test** `adapters/pi/test/smoke.test.ts` to prove the toolchain runs:

```typescript
import { test } from "node:test"
import assert from "node:assert/strict"

test("toolchain smoke test", () => {
  assert.equal(1 + 1, 2)
})
```

- [ ] **Step 6: Install dev deps and run the smoke test:**

Run:
```bash
cd adapters/pi && npm install && node --experimental-strip-types --test test/*.test.ts
```
Expected: `npm install` succeeds (installs `typescript` + `@types/node`); the test run reports `1 passing` / `# pass 1`.

- [ ] **Step 7: Type-check:**

Run: `cd adapters/pi && npx tsc --noEmit`
Expected: no output, exit 0.

- [ ] **Step 8: Commit:**

```bash
git add adapters/pi/package.json adapters/pi/tsconfig.json adapters/pi/test/smoke.test.ts
git commit -m "feat(pi): scaffold pi adapter package + toolchain"
```

---

## Task 2: Pure helpers — `synthesizeDiff` and `normalizeEdits`

These are pure functions (no I/O, no `ironlint`), so they're unit-tested in isolation first. `synthesizeDiff` is ported from the opencode adapter and extended to emit **one hunk per edit** for pi's batch `edit` tool (§6.1). `normalizeEdits` is shared by both `synthesizeDiff` and (Task 3) `computeProposedContent`.

**Files:**
- Create: `adapters/pi/src/index.ts`
- Modify: `adapters/pi/test/index.test.ts` (create; `smoke.test.ts` is deleted in Step 5)

- [ ] **Step 1: Write the failing tests.** Create `adapters/pi/test/index.test.ts`:

```typescript
import { test } from "node:test"
import assert from "node:assert/strict"
import { synthesizeDiff, normalizeEdits } from "../src/index.ts"

// --- normalizeEdits -------------------------------------------------------

test("normalizeEdits: batch edits[] array", () => {
  const edits = normalizeEdits({
    edits: [
      { oldText: "a", newText: "x" },
      { oldText: "b", newText: "y" },
    ],
  })
  assert.deepEqual(edits, [
    { oldText: "a", newText: "x" },
    { oldText: "b", newText: "y" },
  ])
})

test("normalizeEdits: legacy top-level oldText/newText", () => {
  const edits = normalizeEdits({ oldText: "a", newText: "x" })
  assert.deepEqual(edits, [{ oldText: "a", newText: "x" }])
})

test("normalizeEdits: legacy oldText with missing newText defaults to empty", () => {
  const edits = normalizeEdits({ oldText: "a" })
  assert.deepEqual(edits, [{ oldText: "a", newText: "" }])
})

test("normalizeEdits: malformed (no edits, no oldText) returns null", () => {
  assert.equal(normalizeEdits({ content: "whatever" }), null)
})

test("normalizeEdits: edits[] with a non-string member returns null", () => {
  assert.equal(normalizeEdits({ edits: [{ oldText: "a" }] as never }), null)
})

test("normalizeEdits: empty edits[] returns null", () => {
  assert.equal(normalizeEdits({ edits: [] }), null)
})

// --- synthesizeDiff (P1-8 hunk counts, P1-9 injection scrub) --------------

test("synthesizeDiff: write tool, single-line content", () => {
  const d = synthesizeDiff("write", "foo.ts", { content: "x" })
  assert.match(d, /^--- a\/foo\.ts\n\+\+\+ b\/foo\.ts\n/)
  assert.ok(d.includes("@@ -1 +1 @@"))
})

test("synthesizeDiff: write tool, multi-line content emits zero-count old side (P1-8)", () => {
  const d = synthesizeDiff("write", "foo.ts", { content: "x\ny" })
  assert.ok(d.includes("@@ -1 +1,2 @@"))
  // Empty old side: no `-<content>` deletion lines (only the `--- a/` header).
  assert.doesNotMatch(d, /^-[^-]/m)
})

test("synthesizeDiff: edit tool, multi-line new emits correct counts (P1-8)", () => {
  const d = synthesizeDiff("edit", "foo.ts", { oldText: "a\nb", newText: "x\ny\nz" })
  assert.ok(d.includes("@@ -1,2 +1,3 @@"))
})

test("synthesizeDiff: edit tool, multi-line old single-line new (P1-8)", () => {
  const d = synthesizeDiff("edit", "foo.ts", { oldText: "a\nb\nc", newText: "x" })
  assert.ok(d.includes("@@ -1,3 +1 @@"))
})

test("synthesizeDiff: batch edit emits one hunk per edit", () => {
  const d = synthesizeDiff("edit", "foo.ts", {
    edits: [
      { oldText: "a", newText: "x" },
      { oldText: "b", newText: "y" },
    ],
  })
  // Exactly one file header, two @@ hunks.
  assert.equal(d.match(/^--- a\/foo\.ts$/gm)?.length, 1)
  assert.equal(d.match(/^@@ /gm)?.length, 2)
  assert.ok(d.includes("-a\n+x"))
  assert.ok(d.includes("-b\n+y"))
})

test("synthesizeDiff: escapes embedded +++/---/@@ headers in newText (P1-9)", () => {
  const evil = "x\n--- a/SECRET\n+++ b/SECRET\n@@ -1 +1 @@\n+pwn"
  const d = synthesizeDiff("edit", "foo.ts", { oldText: "", newText: evil })
  assert.doesNotMatch(d, /^\+\+\+ b\/SECRET$/m)
  assert.doesNotMatch(d, /^--- a\/SECRET$/m)
  assert.doesNotMatch(d, /^@@ -1 \+1 @@$/m)
  // The real headers for the real file remain.
  assert.ok(d.includes("--- a/foo.ts"))
  assert.ok(d.includes("+++ b/foo.ts"))
})

test("synthesizeDiff: escapes embedded headers in oldText (P1-9)", () => {
  // "-- a/SECRET" prefixed with "-" would become "--- a/SECRET" without scrubbing.
  const d = synthesizeDiff("edit", "foo.ts", { oldText: "-- a/SECRET", newText: "x" })
  assert.doesNotMatch(d, /^--- a\/SECRET$/m)
})
```

- [ ] **Step 2: Delete the temporary smoke test:**

Run: `rm adapters/pi/test/smoke.test.ts`

- [ ] **Step 3: Run the tests to verify they fail:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: FAIL — `Cannot find module '../src/index.ts'` / the imported names are undefined.

- [ ] **Step 4: Create `adapters/pi/src/index.ts` with the pure helpers:**

```typescript
// pi adapter for IronLint. A pure translation layer between pi's extension
// lifecycle and the `ironlint` CLI — it contains no rule logic. See
// docs/superpowers/specs/2026-05-28-pi-adapter-design.md.

/** The shape of the input payload pi passes for `write` / `edit` tool calls. */
export type PiToolInput = {
  path?: string
  // pi's renderer tolerates `file_path` as a `path` alias.
  file_path?: string
  // write tool: full post-write body.
  content?: string
  // edit tool: batch of replacements.
  edits?: Array<{ oldText?: string; newText?: string }>
  // edit tool: legacy single-edit form, normalized by pi into edits[].
  oldText?: string
  newText?: string
}

type Edit = { oldText: string; newText: string }

/**
 * Normalize an edit-tool input into a flat `{oldText,newText}[]`.
 *
 *   - `edits[]` (the canonical batch form) is validated member-by-member;
 *     any non-string `oldText`/`newText` poisons the whole batch -> null.
 *   - legacy top-level `{oldText,newText}` -> single-element array
 *     (missing `newText` defaults to "").
 *   - anything else (a write call, malformed input) -> null.
 *
 * Returns null when the input is not a recognizable edit (the caller then
 * skips the gate / falls back), never throws.
 */
export function normalizeEdits(input: PiToolInput): Edit[] | null {
  if (Array.isArray(input.edits)) {
    const out: Edit[] = []
    for (const e of input.edits) {
      if (typeof e?.oldText !== "string" || typeof e?.newText !== "string") {
        return null
      }
      out.push({ oldText: e.oldText, newText: e.newText })
    }
    return out.length > 0 ? out : null
  }
  if (typeof input.oldText === "string") {
    return [
      {
        oldText: input.oldText,
        newText: typeof input.newText === "string" ? input.newText : "",
      },
    ]
  }
  return null
}

// A line that looks like a real unified-diff header. Used to neutralize
// attacker-controlled old/new blocks (P1-9).
const DIFF_HEADER_RE = /^(---|\+\+\+|@@) /

/**
 * Prefix any line that mimics a diff header with a backslash so ironlint's
 * diff parser does not mistake user content for a real `--- a/...`,
 * `+++ b/...`, or `@@ ... @@` header (P1-9). We scrub the already-prefixed
 * block so a malicious old line `-- a/SECRET` (which becomes `--- a/SECRET`
 * after the `-` prefix) is also caught.
 */
function scrub(block: string): string {
  return block
    .split("\n")
    .map((l) => (DIFF_HEADER_RE.test(l) ? "\\" + l : l))
    .join("\n")
}

/**
 * Build a single `@@ ... @@` hunk from one (oldText, newText) pair.
 *
 * P1-8: a literal `@@ -1 +1 @@` is wrong the moment either side has more
 * than one line — ironlint's parser uses the header counts to number added
 * lines. Emit `1,N` form whenever a side has > 1 line, and omit a side's
 * block entirely when it is empty (a pure addition / pure deletion).
 */
function buildHunk(oldText: string, newText: string): string {
  const oldLines = oldText === "" ? 0 : oldText.split("\n").length
  const newLines = newText === "" ? 0 : newText.split("\n").length
  const hunkOld = oldLines <= 1 ? "1" : `1,${oldLines}`
  const hunkNew = newLines <= 1 ? "1" : `1,${newLines}`
  const oldBlock =
    oldText === "" ? "" : oldText.split("\n").map((l) => "-" + l).join("\n") + "\n"
  const newBlock =
    newText === "" ? "" : newText.split("\n").map((l) => "+" + l).join("\n") + "\n"
  return `@@ -${hunkOld} +${hunkNew} @@\n${scrub(oldBlock)}${scrub(newBlock)}`
}

/**
 * Build a synthetic unified diff for a write/edit tool call so
 * `ironlint session record` can ingest it. pi's tool events carry no real
 * diff. A `write` is the single-hunk `"" -> content` case; an `edit` is a
 * batch, so we emit one scrubbed hunk per `{oldText,newText}` under a single
 * file header.
 *
 * Exported for unit testing.
 */
export function synthesizeDiff(
  toolName: string,
  filePath: string,
  input: PiToolInput,
): string {
  const header = `--- a/${filePath}\n+++ b/${filePath}\n`
  if (toolName === "write") {
    const content = typeof input.content === "string" ? input.content : ""
    return header + buildHunk("", content)
  }
  const edits = normalizeEdits(input)
  if (edits === null) {
    // Unrecognizable edit — emit an empty single hunk so the call is still
    // a syntactically valid (no-op) diff rather than throwing.
    return header + buildHunk("", "")
  }
  return header + edits.map((e) => buildHunk(e.oldText, e.newText)).join("")
}
```

- [ ] **Step 5: Run the tests to verify they pass:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: PASS — all `normalizeEdits` and `synthesizeDiff` tests green.

- [ ] **Step 6: Type-check:**

Run: `cd adapters/pi && npx tsc --noEmit`
Expected: no output, exit 0.

- [ ] **Step 7: Commit:**

```bash
git add adapters/pi/src/index.ts adapters/pi/test/index.test.ts
git rm adapters/pi/test/smoke.test.ts
git commit -m "feat(pi): normalizeEdits + synthesizeDiff with P1-8/P1-9 hardening"
```

---

## Task 3: `computeProposedContent` (§5.1)

Computes the exact post-edit file body that pi is about to write, so the gate can pipe it to `ironlint check --content -`. Pure except for reading the target file from disk on the `edit` path. `write` → the full body; `edit` → apply each replacement, requiring each `oldText` to occur **exactly once** (mirrors pi's own contract); any miss → `null` (skip the gate, §3 / §5.1).

**Files:**
- Modify: `adapters/pi/src/index.ts`
- Modify: `adapters/pi/test/index.test.ts`

- [ ] **Step 1: Write the failing tests.** Append to `adapters/pi/test/index.test.ts`:

```typescript
import { mkdtempSync, writeFileSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { computeProposedContent } from "../src/index.ts"

test("computeProposedContent: write returns the full body (new file ok)", () => {
  assert.equal(
    computeProposedContent("write", "/nonexistent/new.ts", { content: "hello\n" }),
    "hello\n",
  )
})

test("computeProposedContent: write with non-string content returns null", () => {
  assert.equal(
    computeProposedContent("write", "/nonexistent/new.ts", {} as never),
    null,
  )
})

test("computeProposedContent: edit applies a single replacement", () => {
  const dir = mkdtempSync(join(tmpdir(), "ironlint-pi-cpc-"))
  try {
    const file = join(dir, "a.txt")
    writeFileSync(file, "hello world\n")
    assert.equal(
      computeProposedContent("edit", file, { oldText: "world", newText: "DEBUG" }),
      "hello DEBUG\n",
    )
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("computeProposedContent: edit applies a batch in order", () => {
  const dir = mkdtempSync(join(tmpdir(), "ironlint-pi-cpc-"))
  try {
    const file = join(dir, "a.txt")
    writeFileSync(file, "alpha beta\n")
    assert.equal(
      computeProposedContent("edit", file, {
        edits: [
          { oldText: "alpha", newText: "x" },
          { oldText: "beta", newText: "y" },
        ],
      }),
      "x y\n",
    )
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("computeProposedContent: edit returns null when file does not exist", () => {
  assert.equal(
    computeProposedContent("edit", "/nonexistent/missing.txt", {
      oldText: "a",
      newText: "b",
    }),
    null,
  )
})

test("computeProposedContent: edit returns null when oldText is missing from file", () => {
  const dir = mkdtempSync(join(tmpdir(), "ironlint-pi-cpc-"))
  try {
    const file = join(dir, "a.txt")
    writeFileSync(file, "hello world\n")
    assert.equal(
      computeProposedContent("edit", file, { oldText: "nope", newText: "x" }),
      null,
    )
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("computeProposedContent: edit returns null when oldText is non-unique", () => {
  const dir = mkdtempSync(join(tmpdir(), "ironlint-pi-cpc-"))
  try {
    const file = join(dir, "a.txt")
    writeFileSync(file, "a a\n")
    assert.equal(
      computeProposedContent("edit", file, { oldText: "a", newText: "x" }),
      null,
    )
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("computeProposedContent: unknown tool returns null", () => {
  assert.equal(computeProposedContent("read", "/whatever", {}), null)
})
```

- [ ] **Step 2: Run the tests to verify they fail:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: FAIL — `computeProposedContent` is not exported.

- [ ] **Step 3: Implement `computeProposedContent`.** Add the `node:fs` import at the top of `adapters/pi/src/index.ts` (just below the file header comment, before the type definitions):

```typescript
import { existsSync, readFileSync } from "node:fs"
```

Then append the function to the end of `adapters/pi/src/index.ts`:

```typescript
/**
 * Compute the file body pi is about to write, so the gate can pipe it to
 * `ironlint check --content -`. See spec §5.1.
 *
 *   - `write` -> `input.content` (the full body), even for a new file.
 *     Non-string content (malformed call) -> null; pi would reject it too.
 *   - `edit`  -> read the current file, apply each `{oldText,newText}` in
 *     order. Each `oldText` must occur EXACTLY ONCE in the working buffer
 *     (mirrors pi's contract); on any miss or non-unique match -> null.
 *     A non-existent file -> null.
 *
 * We deliberately do NOT reproduce pi's fuzzy-match fallback — diverging
 * there would feed ironlint content pi won't actually write, risking false
 * blocks. Returning null skips the gate (fail-open on simulate-failure).
 */
export function computeProposedContent(
  toolName: string,
  filePath: string,
  input: PiToolInput,
): string | null {
  if (toolName === "write") {
    return typeof input.content === "string" ? input.content : null
  }
  if (toolName === "edit") {
    const edits = normalizeEdits(input)
    if (edits === null) return null
    if (!existsSync(filePath)) return null
    let buf = readFileSync(filePath, "utf8")
    for (const { oldText, newText } of edits) {
      const first = buf.indexOf(oldText)
      if (first === -1) return null
      // Reject non-unique matches (and empty oldText, where first=0 and
      // last=buf.length) so we never guess which occurrence pi means.
      if (first !== buf.lastIndexOf(oldText)) return null
      buf = buf.slice(0, first) + newText + buf.slice(first + oldText.length)
    }
    return buf
  }
  return null
}
```

- [ ] **Step 4: Run the tests to verify they pass:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: PASS — all `computeProposedContent` tests green.

- [ ] **Step 5: Type-check:**

Run: `cd adapters/pi && npx tsc --noEmit`
Expected: no output, exit 0.

- [ ] **Step 6: Commit:**

```bash
git add adapters/pi/src/index.ts adapters/pi/test/index.test.ts
git commit -m "feat(pi): computeProposedContent for write/edit pre-write simulation"
```

---

## Task 4: The gate — `runIronLint` + factory + `tool_call` handler (§5)

The core. Registers the `tool_call` handler that gates `write`/`edit` against the proposed content. This task wires the factory, the subprocess wrapper, the small path helpers, and the happy-path + block behavior. (Edit-specific and edge-case gate tests come in Task 5; the other three handlers in Tasks 6–7.)

**Files:**
- Modify: `adapters/pi/src/index.ts`
- Modify: `adapters/pi/test/index.test.ts`

- [ ] **Step 1: Write the failing tests.** Append to `adapters/pi/test/index.test.ts`:

```typescript
import { execFileSync } from "node:child_process"
import { existsSync } from "node:fs"
import ironlintExtension from "../src/index.ts"

// Drive the exported factory with a fake `pi` that records handlers, then
// invoke them with synthetic pi-shaped events against the real `ironlint`
// binary (CI prepends target/release to PATH).

type Handler = (event: unknown, ctx?: unknown) => unknown
function loadExtension(root: string): Record<string, Handler> {
  const handlers: Record<string, Handler> = {}
  const pi = {
    on: (ev: string, h: Handler) => {
      handlers[ev] = h
    },
    cwd: root,
  }
  // Cast through unknown — the fake only implements the surface the factory uses.
  ironlintExtension(pi as unknown as Parameters<typeof ironlintExtension>[0])
  return handlers
}

const IRONLINT_YML = `schema_version: 2
rules:
  no-debug:
    description: "no DEBUG markers in source"
    engine: script
    scope: ["*.txt"]
    severity: error
    script: "grep -nE 'DEBUG' {file} && exit 1 || exit 0"
`

function makeProject(): string {
  const dir = mkdtempSync(join(tmpdir(), "ironlint-pi-proj-"))
  writeFileSync(join(dir, ".ironlint.yml"), IRONLINT_YML)
  execFileSync("ironlint", ["trust", "--config", join(dir, ".ironlint.yml")])
  return dir
}

test("tool_call: clean write passes (returns nothing), file never written", () => {
  const dir = makeProject()
  try {
    const file = join(dir, "clean.txt")
    const handlers = loadExtension(dir)
    const result = handlers.tool_call!(
      { toolName: "write", input: { path: file, content: "ok\n" } },
      {},
    )
    assert.equal(result, undefined)
    // --content - never writes to disk.
    assert.equal(existsSync(file), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: write introducing DEBUG blocks", () => {
  const dir = makeProject()
  try {
    const file = join(dir, "dirty.txt")
    const handlers = loadExtension(dir)
    const result = handlers.tool_call!(
      { toolName: "write", input: { path: file, content: "this has DEBUG\n" } },
      {},
    ) as { block?: boolean; reason?: string } | undefined
    assert.equal(result?.block, true)
    assert.ok(typeof result?.reason === "string" && result.reason.length > 0)
    assert.equal(existsSync(file), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})
```

- [ ] **Step 2: Run the tests to verify they fail:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: FAIL — the default export `ironlintExtension` does not exist yet.

- [ ] **Step 3: Implement the subprocess wrapper, path helpers, and factory.** First extend the `node:fs` import line and add `node:child_process` + `node:path` imports at the top of `adapters/pi/src/index.ts`:

Change:
```typescript
import { existsSync, readFileSync } from "node:fs"
```
to:
```typescript
import { spawnSync } from "node:child_process"
import { existsSync, readFileSync, rmSync } from "node:fs"
import { basename, join } from "node:path"
```

Then append to the end of `adapters/pi/src/index.ts`:

```typescript
// pi tools we gate. `bash` is intentionally not gated (shell redirections
// like `cat > foo` are too brittle to parse — universal adapter gap §13).
const GATED_TOOLS = new Set(["write", "edit"])

// R3: filenames ironlint treats as policy files. Edits to these short-circuit
// both the gate and session record — checking a mid-edit policy file fails
// the trust gate (sha mismatch) and surfaces a confusing internal error.
const POLICY_FILES = new Set([".ironlint.yml", ".bully.yml"])

/** R3: basename match covers both relative and absolute paths. */
export function isPolicyFile(filePath: string): boolean {
  return POLICY_FILES.has(basename(filePath))
}

/** pi uses `path`; `file_path` is tolerated as an alias (§2.4). */
export function getPath(input: PiToolInput): string | undefined {
  return input.path ?? input.file_path
}

type ExecResult = { exitCode: number; stdout: string; stderr: string }

/**
 * Invoke the `ironlint` binary (must be on PATH). Uses node:child_process
 * spawnSync for deterministic stdin (`input`) + exit code (`status`) — see
 * the spec §14 resolution. `status` is null only when the process was
 * killed by a signal; map that to -1 so it falls through to fail-open.
 */
export function runIronLint(args: string[], input = ""): ExecResult {
  const res = spawnSync("ironlint", args, { input, encoding: "utf8" })
  return {
    exitCode: typeof res.status === "number" ? res.status : -1,
    stdout: res.stdout ?? "",
    stderr: res.stderr ?? "",
  }
}

/**
 * Shared exit-3 (engine-internal-error) policy: fail-open (log + allow) by
 * default; fail-closed (return a block) under IRONLINT_FAIL_CLOSED_ON_INTERNAL=1.
 * A misconfigured ironlint must never brick the agent.
 */
function failOpenOrClosed(
  kind: string,
  stderr: string,
): { block: true; reason: string } | undefined {
  const suffix = stderr ? `: ${stderr}` : ""
  if (process.env["IRONLINT_FAIL_CLOSED_ON_INTERNAL"] === "1") {
    console.error(
      `ironlint: internal error during ${kind} — failing closed (IRONLINT_FAIL_CLOSED_ON_INTERNAL=1)${suffix}`,
    )
    return { block: true, reason: `ironlint: internal error during ${kind} — failing closed` }
  }
  console.error(
    `ironlint: internal error during ${kind} — allowing; see .ironlint/log.jsonl${suffix}`,
  )
  return undefined
}

/** Minimal structural view of the pi extension API the adapter relies on. */
export interface PiExtensionAPI {
  on(event: string, handler: (event: never, ctx?: never) => unknown): void
  cwd?: string
  directory?: string
}

interface ToolCallEvent {
  toolName?: string
  toolCallId?: string
  input?: PiToolInput
}

/** Resolve the project root. process.cwd() is the terminal-agent fallback. */
function resolveRoot(pi: PiExtensionAPI): string {
  return pi.cwd ?? pi.directory ?? process.cwd()
}

/**
 * IronLint pi extension. Registers four lifecycle handlers (the gate is wired
 * here; tool_result / session_start / agent_end are added in later tasks).
 */
export default function ironlintExtension(pi: PiExtensionAPI): void {
  const projectRoot = resolveRoot(pi)
  const configPath = join(projectRoot, ".ironlint.yml")
  const sessionStatePath = join(projectRoot, ".ironlint", "session.json")

  pi.on("tool_call", (event: ToolCallEvent) => {
    // Late existence check: the extension may load before `ironlint init`.
    // Re-checking here means mid-session init starts gating with no restart.
    if (!existsSync(configPath)) return
    const toolName = event?.toolName
    if (!toolName || !GATED_TOOLS.has(toolName)) return
    const input = event?.input ?? {}
    const filePath = getPath(input)
    if (!filePath) return
    if (isPolicyFile(filePath)) return // R3 self-edit short-circuit

    const proposed = computeProposedContent(toolName, filePath, input)
    if (proposed === null) return // can't faithfully simulate — skip the gate

    const res = runIronLint(
      ["check", "--file", filePath, "--content", "-", "--config", configPath, "--format", "json"],
      proposed,
    )
    if (res.exitCode === 0) return // pass/warn -> allow
    if (res.exitCode === 2) {
      return { block: true, reason: res.stdout.trim() || "rule violation" }
    }
    if (res.exitCode === 3) {
      return failOpenOrClosed("check", res.stderr.trim())
    }
    // exit 1 / other -> config error: log + allow.
    const suffix = res.stderr.trim() ? `: ${res.stderr.trim()}` : ""
    console.error(`ironlint: internal error checking ${filePath} (exit ${res.exitCode})${suffix}`)
    return
  })
}
```

> **Note on `PiExtensionAPI`:** if `@earendil-works/pi-coding-agent` exports an `ExtensionAPI` type whose `on` signature is compatible, you may `import type { ExtensionAPI }` and use it in place of the local `PiExtensionAPI`. The local interface keeps the adapter compiling with zero external type deps and is the supported default.

- [ ] **Step 4: Run the tests to verify they pass:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: PASS — clean write passes, DEBUG write blocks, no file written in either case.

- [ ] **Step 5: Type-check:**

Run: `cd adapters/pi && npx tsc --noEmit`
Expected: no output, exit 0.

- [ ] **Step 6: Commit:**

```bash
git add adapters/pi/src/index.ts adapters/pi/test/index.test.ts
git commit -m "feat(pi): tool_call gate via ironlint check --content -"
```

---

## Task 5: `tool_call` edge cases (§5, §9)

Exercises the gate's full decision surface against the implementation from Task 4: edit-introduces-violation, batch edits, legacy single-edit form, skip-on-unmatched, non-gated tools, missing path, late-config re-check, and R3 self-edit short-circuits. These drive no new source — if any test is red, the bug is in the Task 4 handler; fix it there.

**Files:**
- Modify: `adapters/pi/test/index.test.ts`

- [ ] **Step 1: Write the tests.** Append to `adapters/pi/test/index.test.ts`:

```typescript
import { readFileSync } from "node:fs"

test("tool_call: edit introducing DEBUG blocks; on-disk file untouched", () => {
  const dir = makeProject()
  try {
    const file = join(dir, "edit.txt")
    writeFileSync(file, "hello world\n")
    const handlers = loadExtension(dir)
    const result = handlers.tool_call!(
      { toolName: "edit", input: { path: file, oldText: "world", newText: "DEBUG" } },
      {},
    ) as { block?: boolean } | undefined
    assert.equal(result?.block, true)
    // --content - means the gate never writes; pi's real edit was blocked.
    assert.equal(readFileSync(file, "utf8"), "hello world\n")
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: multi-edit batch is simulated and blocks", () => {
  const dir = makeProject()
  try {
    const file = join(dir, "batch.txt")
    writeFileSync(file, "alpha beta\n")
    const handlers = loadExtension(dir)
    const result = handlers.tool_call!(
      {
        toolName: "edit",
        input: {
          path: file,
          edits: [
            { oldText: "alpha", newText: "x" },
            { oldText: "beta", newText: "DEBUG" },
          ],
        },
      },
      {},
    ) as { block?: boolean } | undefined
    assert.equal(result?.block, true)
    assert.equal(readFileSync(file, "utf8"), "alpha beta\n")
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: legacy top-level oldText/newText edit blocks", () => {
  const dir = makeProject()
  try {
    const file = join(dir, "legacy.txt")
    writeFileSync(file, "hello world\n")
    const handlers = loadExtension(dir)
    const result = handlers.tool_call!(
      { toolName: "edit", input: { path: file, oldText: "world", newText: "DEBUG" } },
      {},
    ) as { block?: boolean } | undefined
    assert.equal(result?.block, true)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: edit with unmatched oldText skips the gate (no false block)", () => {
  const dir = makeProject()
  try {
    const file = join(dir, "nomatch.txt")
    writeFileSync(file, "hello world\n")
    const handlers = loadExtension(dir)
    const result = handlers.tool_call!(
      { toolName: "edit", input: { path: file, oldText: "nonexistent", newText: "DEBUG" } },
      {},
    )
    assert.equal(result, undefined)
    assert.equal(readFileSync(file, "utf8"), "hello world\n")
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: non-gated tools (read, bash) are ignored", () => {
  const dir = makeProject()
  try {
    const handlers = loadExtension(dir)
    assert.equal(
      handlers.tool_call!({ toolName: "read", input: { path: "anything" } }, {}),
      undefined,
    )
    assert.equal(
      handlers.tool_call!({ toolName: "bash", input: {} }, {}),
      undefined,
    )
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: missing path is a no-op", () => {
  const dir = makeProject()
  try {
    const handlers = loadExtension(dir)
    assert.equal(handlers.tool_call!({ toolName: "edit", input: {} }, {}), undefined)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: gate activates after .ironlint.yml is created mid-session", () => {
  // Regression: the existence check runs per-invocation, so a project that
  // becomes an ironlint project after the extension loads starts gating with
  // no restart.
  const dir = mkdtempSync(join(tmpdir(), "ironlint-pi-late-"))
  try {
    const file = join(dir, "dirty.txt")
    const handlers = loadExtension(dir)
    // No config yet -> no gating.
    assert.equal(
      handlers.tool_call!(
        { toolName: "write", input: { path: file, content: "this has DEBUG\n" } },
        {},
      ),
      undefined,
    )
    // Create + trust the config, re-invoke the SAME handler closure.
    writeFileSync(join(dir, ".ironlint.yml"), IRONLINT_YML)
    execFileSync("ironlint", ["trust", "--config", join(dir, ".ironlint.yml")])
    const result = handlers.tool_call!(
      { toolName: "write", input: { path: file, content: "this has DEBUG\n" } },
      {},
    ) as { block?: boolean } | undefined
    assert.equal(result?.block, true)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: .ironlint.yml self-edit short-circuits (R3) — no ironlint invocation", () => {
  // Break the trust hash so ANY ironlint check would log an internal error.
  // A clean run proves the basename short-circuit fired before any check.
  const dir = mkdtempSync(join(tmpdir(), "ironlint-pi-policy-"))
  const errs: string[] = []
  const origErr = console.error
  console.error = (...args: unknown[]) => {
    errs.push(args.map(String).join(" "))
  }
  try {
    writeFileSync(join(dir, ".ironlint.yml"), IRONLINT_YML)
    execFileSync("ironlint", ["trust", "--config", join(dir, ".ironlint.yml")])
    const current = readFileSync(join(dir, ".ironlint.yml"), "utf8")
    writeFileSync(
      join(dir, ".ironlint.yml"),
      current.replace(/sha256:[0-9a-f]+/, "sha256:" + "0".repeat(64)),
    )
    const handlers = loadExtension(dir)
    const file = join(dir, ".ironlint.yml")
    assert.equal(
      handlers.tool_call!({ toolName: "write", input: { path: file, content: "x\n" } }, {}),
      undefined,
    )
    assert.ok(!errs.join("\n").includes("internal error"))
  } finally {
    console.error = origErr
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: .bully.yml self-edit short-circuits (R3)", () => {
  const dir = makeProject()
  try {
    const file = join(dir, ".bully.yml")
    const handlers = loadExtension(dir)
    assert.equal(
      handlers.tool_call!({ toolName: "write", input: { path: file, content: "x\n" } }, {}),
      undefined,
    )
    assert.equal(existsSync(file), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: bare relative .ironlint.yml self-edit short-circuits (R3)", () => {
  const dir = makeProject()
  try {
    const handlers = loadExtension(dir)
    assert.equal(
      handlers.tool_call!(
        { toolName: "write", input: { path: ".ironlint.yml", content: "x\n" } },
        {},
      ),
      undefined,
    )
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})
```

- [ ] **Step 2: Run the tests:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: PASS — all edge cases green. (If any fail, the bug is in the Task 4 `tool_call` handler; fix it and re-run.)

- [ ] **Step 3: Type-check:**

Run: `cd adapters/pi && npx tsc --noEmit`
Expected: no output, exit 0.

- [ ] **Step 4: Commit:**

```bash
git add adapters/pi/test/index.test.ts
git commit -m "test(pi): gate edge cases — batch/legacy edits, R3, late-config recheck"
```

---

## Task 6: `tool_result` session record (§6)

After a gated tool finishes (and did not error), record a synthetic diff into `.ironlint/session.json` for cross-edit (session-engine) rules. Best-effort: a flaky record never affects the agent. Skips on missing config, non-gated tools, `isError`, missing path, and policy files (R3).

**Files:**
- Modify: `adapters/pi/src/index.ts`
- Modify: `adapters/pi/test/index.test.ts`

- [ ] **Step 1: Write the failing tests.** Append to `adapters/pi/test/index.test.ts`:

```typescript
test("tool_result: records a write to session.json", () => {
  const dir = makeProject()
  try {
    const file = join(dir, "tracked.txt")
    writeFileSync(file, "ok\n")
    const handlers = loadExtension(dir)
    handlers.tool_result!(
      { toolName: "write", input: { path: file, content: "ok\n" }, isError: false },
      {},
    )
    const stateFile = join(dir, ".ironlint", "session.json")
    assert.equal(existsSync(stateFile), true)
    assert.ok(readFileSync(stateFile, "utf8").includes("tracked.txt"))
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_result: isError result records nothing", () => {
  const dir = makeProject()
  try {
    const file = join(dir, "failed.txt")
    const handlers = loadExtension(dir)
    handlers.tool_result!(
      { toolName: "write", input: { path: file, content: "x\n" }, isError: true },
      {},
    )
    assert.equal(existsSync(join(dir, ".ironlint", "session.json")), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_result: non-gated tool records nothing", () => {
  const dir = makeProject()
  try {
    const handlers = loadExtension(dir)
    handlers.tool_result!({ toolName: "read", input: { path: "x" } }, {})
    assert.equal(existsSync(join(dir, ".ironlint", "session.json")), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_result: policy-file edit records nothing (R3)", () => {
  const dir = makeProject()
  try {
    const handlers = loadExtension(dir)
    handlers.tool_result!(
      { toolName: "write", input: { path: join(dir, ".ironlint.yml"), content: "x\n" } },
      {},
    )
    assert.equal(existsSync(join(dir, ".ironlint", "session.json")), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})
```

- [ ] **Step 2: Run the tests to verify they fail:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: FAIL — `handlers.tool_result` is undefined (handler not registered).

- [ ] **Step 3: Implement the `tool_result` handler.** Add the event interface near `ToolCallEvent` in `adapters/pi/src/index.ts`:

```typescript
interface ToolResultEvent {
  toolName?: string
  input?: PiToolInput
  isError?: boolean
}
```

Then register the handler inside `ironlintExtension`, immediately after the `pi.on("tool_call", ...)` block:

```typescript
  pi.on("tool_result", (event: ToolResultEvent) => {
    if (!existsSync(configPath)) return
    const toolName = event?.toolName
    if (!toolName || !GATED_TOOLS.has(toolName)) return
    if (event?.isError) return // the edit failed; nothing landed
    const input = event?.input ?? {}
    const filePath = getPath(input)
    if (!filePath) return
    if (isPolicyFile(filePath)) return // R3

    // Best-effort: a flaky session record must never affect the agent.
    try {
      const diff = synthesizeDiff(toolName, filePath, input)
      runIronLint([
        "session", "record",
        "--dir", projectRoot,
        "--file", filePath,
        "--diff", diff,
      ])
    } catch {
      // intentional: session recording is best-effort.
    }
  })
```

- [ ] **Step 4: Run the tests to verify they pass:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: PASS — the write is recorded; isError / non-gated / policy-file cases record nothing.

- [ ] **Step 5: Type-check:**

Run: `cd adapters/pi && npx tsc --noEmit`
Expected: no output, exit 0.

- [ ] **Step 6: Commit:**

```bash
git add adapters/pi/src/index.ts adapters/pi/test/index.test.ts
git commit -m "feat(pi): tool_result records synthetic diff to session.json"
```

---

## Task 7: `session_start` clear + `agent_end` advisory check (§7, §8)

`session_start` deletes a stale `.ironlint/session.json` from a prior aborted run. `agent_end` runs `ironlint check --session` — advisory only, because the turn is already finished and cannot be retroactively blocked; it surfaces the verdict so the user (and next turn) see what to fix.

**Files:**
- Modify: `adapters/pi/src/index.ts`
- Modify: `adapters/pi/test/index.test.ts`

- [ ] **Step 1: Write the failing tests.** Append to `adapters/pi/test/index.test.ts`:

```typescript
import { mkdirSync } from "node:fs"

test("session_start: clears stale session.json", () => {
  const dir = makeProject()
  try {
    mkdirSync(join(dir, ".ironlint"), { recursive: true })
    writeFileSync(
      join(dir, ".ironlint", "session.json"),
      JSON.stringify({ session_id: "stale", started_at: "t", edits: [] }),
    )
    const handlers = loadExtension(dir)
    handlers.session_start!({}, {})
    assert.equal(existsSync(join(dir, ".ironlint", "session.json")), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("session_start: no-op when no session.json exists", () => {
  const dir = makeProject()
  try {
    const handlers = loadExtension(dir)
    handlers.session_start!({}, {}) // must not throw
    assert.equal(existsSync(join(dir, ".ironlint", "session.json")), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("agent_end: no-op without session.json", () => {
  const dir = makeProject()
  try {
    const handlers = loadExtension(dir)
    const notified: string[] = []
    handlers.agent_end!({}, { ui: { notify: (m: string) => notified.push(m) } })
    assert.equal(notified.length, 0)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test("agent_end: advisory surface when a session rule blocks", () => {
  // Configure a session rule that blocks when >0 edits are recorded, record
  // an edit, then run agent_end and assert the verdict is surfaced (advisory
  // — agent_end cannot block a finished turn, so it never returns block).
  const SESSION_YML = `schema_version: 2
rules:
  no-edits:
    description: "session must record zero edits"
    engine: session
    severity: error
    session:
      max_files_changed: 0
`
  const dir = mkdtempSync(join(tmpdir(), "ironlint-pi-session-"))
  try {
    writeFileSync(join(dir, ".ironlint.yml"), SESSION_YML)
    execFileSync("ironlint", ["trust", "--config", join(dir, ".ironlint.yml")])
    const file = join(dir, "x.txt")
    writeFileSync(file, "hi\n")
    // Record one edit so the session rule has something to fire on.
    execFileSync("ironlint", [
      "session", "record",
      "--dir", dir,
      "--file", file,
      "--diff", "--- a/x.txt\n+++ b/x.txt\n@@ -1 +1 @@\n-hi\n+yo\n",
    ])

    const notified: string[] = []
    const errs: string[] = []
    const origErr = console.error
    console.error = (...args: unknown[]) => {
      errs.push(args.map(String).join(" "))
    }
    try {
      const handlers = loadExtension(dir)
      const result = handlers.agent_end!(
        {},
        { ui: { notify: (m: string) => notified.push(m) } },
      )
      // Advisory: agent_end never returns a block.
      assert.equal(result, undefined)
      // The verdict is surfaced via console.error and ctx.ui.notify.
      assert.ok(errs.join("\n").includes("session check"))
      assert.ok(notified.length > 0)
    } finally {
      console.error = origErr
    }
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})
```

> **Confirm the session-rule fixture during implementation.** The `SESSION_YML` above assumes a session-engine field (`max_files_changed: 0`) that fires whenever any file is recorded. Verify the exact session-rule schema in `crates/ironlint-core/src/config` / the session engine (`crates/ironlint-core/src/engine`) and adjust the fixture so the rule reliably blocks with one recorded edit. The assertion (advisory surface, no block returned) is what matters; the fixture just needs to produce a `--session` exit code of 2.

- [ ] **Step 2: Run the tests to verify they fail:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: FAIL — `handlers.session_start` / `handlers.agent_end` are undefined.

- [ ] **Step 3: Implement both handlers.** Add the event/context interfaces near the others in `adapters/pi/src/index.ts`:

```typescript
interface PiContext {
  ui?: { notify?: (message: string) => void }
}
```

Then register both handlers inside `ironlintExtension`, after the `pi.on("tool_result", ...)` block:

```typescript
  pi.on("session_start", () => {
    // Clear stale state from a prior aborted run. Best-effort.
    if (!existsSync(sessionStatePath)) return
    try {
      rmSync(sessionStatePath, { force: true })
    } catch {
      // intentional: stale-state cleanup is best-effort.
    }
  })

  pi.on("agent_end", (_event: unknown, ctx?: PiContext) => {
    if (!existsSync(sessionStatePath)) return
    const res = runIronLint([
      "check", "--session",
      "--config", configPath,
      "--format", "json",
    ])
    if (res.exitCode === 2) {
      const verdict = res.stdout.trim() || "session rule violation"
      // agent_end fires after the turn — we cannot retroactively block.
      // Surface the verdict so the user sees what to fix next iteration.
      const msg = `ironlint: session check blocked:\n${verdict}`
      console.error(msg)
      ctx?.ui?.notify?.(msg)
      return
    }
    if (res.exitCode === 3) {
      // Advisory context: failOpenOrClosed logs (and would "block"), but a
      // finished turn can't be blocked, so we ignore its return value.
      failOpenOrClosed("session check", res.stderr.trim())
      return
    }
    if (res.exitCode !== 0) {
      const suffix = res.stderr.trim() ? `: ${res.stderr.trim()}` : ""
      console.error(`ironlint: internal error during session check (exit ${res.exitCode})${suffix}`)
    }
  })
```

- [ ] **Step 4: Run the tests to verify they pass:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: PASS — stale state cleared; agent_end no-ops without state and surfaces (without blocking) on a session block.

- [ ] **Step 5: Type-check:**

Run: `cd adapters/pi && npx tsc --noEmit`
Expected: no output, exit 0.

- [ ] **Step 6: Commit:**

```bash
git add adapters/pi/src/index.ts adapters/pi/test/index.test.ts
git commit -m "feat(pi): session_start stale-clear + agent_end advisory session check"
```

---

## Task 8: exit-3 fail-open / fail-closed behavior (§5.2, §9)

The default exit-3 posture is fail-open (allow); `IRONLINT_FAIL_CLOSED_ON_INTERNAL=1` flips it to a block. Drive this through the gate with a semantic rule that errors with no API key (exit 3 — "missing API key" per the CLI contract).

**Files:**
- Modify: `adapters/pi/test/index.test.ts`

- [ ] **Step 1: Write the tests.** Append to `adapters/pi/test/index.test.ts`:

```typescript
const SEMANTIC_YML = `schema_version: 2
rules:
  needs-llm:
    description: "semantic rule with no key -> engine internal error (exit 3)"
    engine: semantic
    scope: ["*.txt"]
    severity: error
    prompt: "Always block."
`

function makeSemanticProject(): string {
  const dir = mkdtempSync(join(tmpdir(), "ironlint-pi-sem-"))
  writeFileSync(join(dir, ".ironlint.yml"), SEMANTIC_YML)
  execFileSync("ironlint", ["trust", "--config", join(dir, ".ironlint.yml")])
  return dir
}

test("tool_call: exit-3 fails open by default (allows the edit)", () => {
  const dir = makeSemanticProject()
  const hadKey = process.env["ANTHROPIC_API_KEY"]
  delete process.env["ANTHROPIC_API_KEY"]
  delete process.env["IRONLINT_FAIL_CLOSED_ON_INTERNAL"]
  const origErr = console.error
  console.error = () => {}
  try {
    const file = join(dir, "x.txt")
    const handlers = loadExtension(dir)
    const result = handlers.tool_call!(
      { toolName: "write", input: { path: file, content: "anything\n" } },
      {},
    )
    assert.equal(result, undefined) // fail-open
  } finally {
    console.error = origErr
    if (hadKey !== undefined) process.env["ANTHROPIC_API_KEY"] = hadKey
    rmSync(dir, { recursive: true, force: true })
  }
})

test("tool_call: exit-3 fails closed under IRONLINT_FAIL_CLOSED_ON_INTERNAL=1", () => {
  const dir = makeSemanticProject()
  const hadKey = process.env["ANTHROPIC_API_KEY"]
  delete process.env["ANTHROPIC_API_KEY"]
  process.env["IRONLINT_FAIL_CLOSED_ON_INTERNAL"] = "1"
  const origErr = console.error
  console.error = () => {}
  try {
    const file = join(dir, "x.txt")
    const handlers = loadExtension(dir)
    const result = handlers.tool_call!(
      { toolName: "write", input: { path: file, content: "anything\n" } },
      {},
    ) as { block?: boolean } | undefined
    assert.equal(result?.block, true) // fail-closed
  } finally {
    console.error = origErr
    delete process.env["IRONLINT_FAIL_CLOSED_ON_INTERNAL"]
    if (hadKey !== undefined) process.env["ANTHROPIC_API_KEY"] = hadKey
    rmSync(dir, { recursive: true, force: true })
  }
})
```

> **Confirm the exit-3 trigger during implementation.** This assumes a `semantic` rule with no `ANTHROPIC_API_KEY` produces exit 3 (engine internal error). Verify against `crates/ironlint-cli/src/commands/check.rs` (`exit_code` maps `Status::InternalError` → 3) and the semantic engine's missing-key path. If a bare semantic rule needs more fields to reach the key check, adjust `SEMANTIC_YML` so the gate run returns exit 3. (You can confirm directly: `cd <tmp> && ironlint check --file x.txt --content - --config .ironlint.yml --format json <<<'hi'; echo $?` should print `3`.)

- [ ] **Step 2: Run the tests:**

Run: `cd adapters/pi && node --experimental-strip-types --test test/*.test.ts`
Expected: PASS — default allows on exit 3; env var blocks on exit 3.

- [ ] **Step 3: Commit:**

```bash
git add adapters/pi/test/index.test.ts
git commit -m "test(pi): exit-3 fail-open default + fail-closed under env"
```

---

## Task 9: README (§12, §13)

**Files:**
- Create: `adapters/pi/README.md`

- [ ] **Step 1: Write `adapters/pi/README.md`:**

````markdown
# IronLint — pi adapter

[pi](https://pi.dev) extension integration for IronLint. Mirrors the OpenCode and
Claude Code adapters: it gates `write` / `edit` tool calls against your
project's `.ironlint.yml` policy **before they execute**, records edits for
cross-edit (session) rules, and runs a session check at the end of each turn.

The extension is a pure translation layer between pi's lifecycle and the
`ironlint` binary — it contains no rule logic.

| pi event | Action |
|----------|--------|
| `tool_call` (`write` / `edit`) | Compute the proposed content, run `ironlint check --file <path> --content -`, and `return { block: true, reason }` on a policy violation (exit 2). The check runs against piped stdin — nothing is written to disk. |
| `tool_result` (`write` / `edit`) | Record a synthetic diff into `.ironlint/session.json` for session rules (best-effort). |
| `session_start` | Clear a stale `.ironlint/session.json` from a prior aborted run. |
| `agent_end` | Run `ironlint check --session`. **Advisory** — the turn is already over, so the verdict is surfaced (it cannot retroactively block). |

## Requirements

- The `ironlint` binary on `PATH` (`cargo install ironlint` or a release binary), ≥ 0.1.
- Node ≥ 22.6 (pi's runtime; also required for the bundled `node:test` suite).

## Install

The extension silently no-ops in any project without a `.ironlint.yml`, so a
global install is safe.

### Local development

Copy or symlink the source into a pi extensions directory:

```bash
# project-scoped
mkdir -p .pi/extensions
ln -sf "$(pwd)/../ironlint/adapters/pi/src/index.ts" .pi/extensions/ironlint.ts

# or global
mkdir -p ~/.pi/agent/extensions
ln -sf "/abs/path/to/ironlint/adapters/pi/src/index.ts" ~/.pi/agent/extensions/ironlint.ts
```

Or reference an absolute path in pi `settings.json`:

```json
{ "extensions": ["/abs/path/to/ironlint/adapters/pi/src/index.ts"] }
```

Ad-hoc load for one session: `pi -e ./adapters/pi/src/index.ts`. Hot-reload
with `/reload`.

### npm (once published)

`@christopherarter/ironlint-pi` ships a `"pi": { "extensions": ["./src/index.ts"] }`
field, so pi discovers it automatically once the package is installed.

## Initialise the project

```bash
ironlint init    # scaffold .ironlint.yml
ironlint trust   # fingerprint the config
```

## Exit-code contract

The extension honours the `ironlint` CLI exit-code contract
(`crates/ironlint-cli/src/commands/check.rs`):

| Exit | Behaviour |
|------|-----------|
| `0` (pass / warn) | Allow. |
| `2` (block) | `return { block: true, reason }` — pi cancels the tool call. |
| `3` (engine internal error) | Fail-open (log + allow) by default; set `IRONLINT_FAIL_CLOSED_ON_INTERNAL=1` to fail closed (block). |
| `1` / other (config error) | Log to stderr, allow. |

## Known gaps (v1)

- **`bash`-tool shell-out** (`cat > foo`, redirections) bypasses the gate — universal across all adapters; arbitrary commands are too brittle to parse.
- **`edit` fuzzy-match fallback** can't be faithfully simulated, so those edits skip the gate (fail-open on simulate-failure). Exact + unique `oldText` edits gate normally.
- **`engine: script` rules** read the pre-edit on-disk file under `--content -`. AST / semantic / `ironlint-disable` rules gate correctly against the proposed pre-write content.
- **pi subagents** are not specially handled (deferred).
- **The `agent_end` session check is advisory** — it cannot retroactively block a finished turn; it surfaces the verdict for the next iteration.

## Diagnostic

If the gate isn't firing:

1. `ironlint --version` runs on `PATH`.
2. `.ironlint.yml` is present in the project root.
3. `.ironlint.yml` is trusted: `ironlint trust`.
4. pi loaded the extension (check pi's extension discovery logs / `/reload`).
5. Run the bundled suite against your install:

   ```bash
   PATH="$(pwd)/target/release:${PATH}" \
     node --experimental-strip-types --test adapters/pi/test/*.test.ts
   ```
````

- [ ] **Step 2: Commit:**

```bash
git add adapters/pi/README.md
git commit -m "docs(pi): adapter README — install, exit-code table, known gaps"
```

---

## Task 10: CI wiring (§10)

Add a `pi-adapter` job to `.github/workflows/ci.yml` mirroring the existing `opencode-adapter` job: download the built `ironlint` artifact, set up Node, type-check, and run the `node:test` suite with `ironlint` on `PATH`.

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add the job.** Append this job to the `jobs:` map in `.github/workflows/ci.yml`, after the `opencode-adapter` job (keep the same indentation as the other jobs — two spaces for the job key):

```yaml
  pi-adapter:
    name: pi adapter
    runs-on: ubuntu-latest
    needs: rust
    steps:
      - uses: actions/checkout@v4

      - uses: actions/download-artifact@v4
        with:
          name: ironlint-ubuntu-latest
          path: target/release

      - run: chmod +x target/release/ironlint

      - uses: actions/setup-node@v4
        with:
          node-version: "22"

      - name: Install dev deps
        working-directory: adapters/pi
        run: npm install

      - name: TypeScript type-check
        working-directory: adapters/pi
        run: npx tsc --noEmit

      - name: Extension integration test
        run: PATH="$(pwd)/target/release:${PATH}" node --experimental-strip-types --test adapters/pi/test/*.test.ts
```

- [ ] **Step 2: Validate the workflow YAML parses.** Run a quick syntax sanity check:

Run:
```bash
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('ok')"
```
Expected: `ok`.

- [ ] **Step 3: Confirm the artifact name matches.** The `rust` job uploads `ironlint-ubuntu-latest` (see the existing `Upload release binary` step). Verify the `download-artifact` `name:` above matches it exactly.

Run: `grep -n "ironlint-ubuntu-latest\|name: ironlint-" .github/workflows/ci.yml`
Expected: the upload (`ironlint-${{ matrix.os }}`) and both adapter jobs' downloads reference the same `ironlint-ubuntu-latest` artifact.

- [ ] **Step 4: Commit:**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(pi): run pi adapter node:test suite against built ironlint"
```

---

## Task 11: Final verification & cleanup

**Files:** none (verification only).

- [ ] **Step 1: Run the full adapter suite + type-check from a clean PATH:**

Run:
```bash
cargo build --release && \
  PATH="$(pwd)/target/release:${PATH}" \
  bash -c 'cd adapters/pi && npx tsc --noEmit && node --experimental-strip-types --test test/*.test.ts'
```
Expected: `tsc` exits 0; every `node:test` case passes (`# pass N`, `# fail 0`).

- [ ] **Step 2: Confirm no stray files / shadow-writes leaked.** The `--content -` gate must never write to disk; temp project dirs are cleaned up by each test's `finally`.

Run: `git status --porcelain adapters/pi`
Expected: only the intended tracked files (`src/index.ts`, `test/index.test.ts`, `package.json`, `tsconfig.json`, `README.md`) — no `node_modules`, no stray `.txt` fixtures. If `node_modules` shows up, add `adapters/pi/node_modules/` to the repo `.gitignore`.

- [ ] **Step 3: Clean up the release build artifact** (per CLAUDE.md — drop artifacts this task created):

Run: `cargo clean -p ironlint-cli`

- [ ] **Step 4: Self-review against the spec.** Re-read `docs/superpowers/specs/2026-05-28-pi-adapter-design.md` §4–§9 and §11 and confirm each behavior has a passing test:
  - gate on `tool_call` with exit-code mapping (Tasks 4, 5, 8) ✓
  - `computeProposedContent` write + edit + batch + skip (Task 3) ✓
  - `synthesizeDiff` P1-8 counts + P1-9 scrub + per-edit batch (Task 2) ✓
  - `tool_result` session record with isError / R3 guards (Task 6) ✓
  - `session_start` stale clear (Task 7) ✓
  - `agent_end` advisory session check (Task 7) ✓
  - R3 self-edit + late-config re-check + missing path + non-gated tools (Task 5) ✓

- [ ] **Step 5: Request code review** from a separate agent (per CLAUDE.md: "After completing a coding task, request code review from a separate agent"). Use `superpowers:requesting-code-review`.

---

## Self-Review (author)

- **Spec coverage:** §4 lifecycle → Tasks 4–7; §5 gate algorithm → Task 4 (+ edges Task 5, exit-3 Task 8); §5.1 `computeProposedContent` → Task 3; §5.2 exit contract → Tasks 4/7/8; §6 + §6.1 session record + `synthesizeDiff` → Tasks 6/2; §7 session check → Task 7; §8 `session_start` → Task 7; §9 invariants (fail-open, best-effort, R3, late re-check) → Tasks 4–7; §10 files → all; §11 test cases → Tasks 2–8; §12/§13 docs → Task 9; §14 `pi.exec` open question → Task 1 (resolved to `spawnSync`); CI → Task 10.
- **Naming consistency:** `synthesizeDiff(toolName, filePath, input)`, `computeProposedContent(toolName, filePath, input)`, `normalizeEdits(input)`, `getPath(input)`, `isPolicyFile(filePath)`, `runIronLint(args, input?)`, `resolveRoot(pi)`, default export `ironlintExtension(pi)`, interface `PiExtensionAPI`, type `PiToolInput`. Used consistently across all tasks.
- **Two confirm-during-implementation notes** are flagged inline (Task 7 session-rule fixture, Task 8 exit-3 trigger) because they depend on the exact ironlint session/semantic schema; both include a concrete verification command and a fallback. The assertions they prove are unambiguous.
