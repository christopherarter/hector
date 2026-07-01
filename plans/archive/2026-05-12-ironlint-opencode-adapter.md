# IronLint — OpenCode Adapter Implementation Plan

**Goal:** Ship the OpenCode adapter at parity with the Claude Code adapter. After this, a user can install IronLint as an OpenCode plugin and get the same UX: edit/write tool results are gated against `.ironlint.yml`; session-engine rules fire at session idle.

**Architecture:** A new `adapters/opencode/` directory contains an `@opencode-ai/plugin`-typed TypeScript module that hooks `tool.execute.after` (for per-edit checks) and the generic `event` hook (for `session.created` / `session.idle`). The plugin shells out to the `ironlint` binary via Bun's `$` API and translates the verdict to a thrown `Error` (which OpenCode surfaces back to the agent). The `ironlint` binary itself does **not** change — the adapter consumes the existing CLI surface (`ironlint check`, `ironlint session record`).

**Why `tool.execute.after`, not `before`:** OpenCode's `tool.execute.before` fires before the edit is applied to disk. IronLint's engines (`script`, `ast`, the file-context shape of `semantic`) read the file from disk. Running on `before` would require synthesising the post-edit content in memory and writing a temp file — complex and lossy for `apply_patch`. `tool.execute.after` matches Claude Code's `PostToolUse` semantics exactly: the file is on disk, ironlint checks it, the throw surfaces a rejection back to the agent. The file stays modified — same as Claude Code.

**Tech Stack:** TypeScript (typed against `@opencode-ai/plugin`), Bun runtime (OpenCode's host), JSON (manifest). No new Rust workspace dependencies, no shell scripts.

---

## File structure

```
ironlint/
├── adapters/
│   └── opencode/
│       ├── package.json                              ← NEW: npm metadata + peer dep on @opencode-ai/plugin
│       ├── tsconfig.json                             ← NEW: TS strict, for in-repo type-check
│       ├── src/
│       │   └── index.ts                              ← NEW: the plugin (tool.execute.after + event)
│       ├── tests/
│       │   └── plugin.test.ts                        ← NEW: Bun test of the plugin against a temp project
│       └── README.md                                 ← NEW: install + usage
└── docs/
    └── adapters/
        └── opencode.md                               ← NEW: long-form integration doc
```

No changes to `crates/` — the `ironlint` binary already exposes everything the adapter needs (`check --file`, `check --session`, `session record`).

---

## Phase 1 — Plugin source

### Task 1: `adapters/opencode/src/index.ts`

The plugin exports a default `Plugin`-typed async function. It:

1. No-ops silently when `.ironlint.yml` is absent (so installing the plugin in a non-ironlint project is a free, fast no-op).
2. On `tool.execute.after` for `edit` / `write`:
   - Extracts `input.args.filePath`.
   - Records the edit into `.ironlint/session.json` (non-fatal — failures are swallowed so a flaky session record never blocks the agent).
   - Runs `ironlint check --file <path> --config .ironlint.yml --format json`.
   - Exit 0 → return (allow). Exit 2 → throw `Error(verdict)` (block). Other → `console.error` and return (allow; internal ironlint error should not block the agent on unrelated work, mirrors how the Claude Code hook surfaces internal error to stderr but the contract is the same: blocks come only from exit 2).
3. On `event` with `event.type === "session.created"`: clear stale `.ironlint/session.json`.
4. On `event` with `event.type === "session.idle"`: if `.ironlint/session.json` exists, run `ironlint check --session`. Throw to surface session-rule violations.

**Tool-name routing:** OpenCode's `edit` and `write` tools use camelCase `filePath` (per source: `output.args.filePath` in the `.env` protection example). The plugin filters on `input.tool === "edit" || input.tool === "write"`. `apply_patch` is out of scope at this milestone — its multi-file patch format would need per-file extraction; document as a known gap.

**Working directory:** Plugin context provides `worktree` (git worktree path) and `directory` (cwd). We use `worktree ?? directory` as the project root, matching how OpenCode treats workspace-scoped files.

---

## Phase 2 — Packaging

### Task 2: `adapters/opencode/package.json`

```jsonc
{
  "name": "@christopherarter/ironlint-opencode",
  "version": "0.1.0",
  "description": "OpenCode plugin for IronLint — policy enforcement for AI coding agents.",
  "type": "module",
  "main": "src/index.ts",
  "exports": { ".": "./src/index.ts" },
  "files": ["src", "README.md"],
  "peerDependencies": { "@opencode-ai/plugin": "*" },
  "devDependencies": { "@opencode-ai/plugin": "*", "@types/node": "*" },
  "keywords": ["opencode", "opencode-plugin", "ironlint", "lint", "policy"],
  "license": "Apache-2.0",
  "repository": "https://github.com/christopherarter/ironlint",
  "homepage": "https://github.com/christopherarter/ironlint"
}
```

`main` points at `.ts`: OpenCode runs Bun, which loads TypeScript natively. No build step is needed for local use; if/when we publish, Bun's bundler can emit a `.js` if necessary.

### Task 3: `adapters/opencode/tsconfig.json`

Strict TS so the plugin source type-checks against `@opencode-ai/plugin` types in the repo.

---

## Phase 3 — README

### Task 4: `adapters/opencode/README.md`

Sections:
- **What it does** — parity table with the Claude Code adapter.
- **Install (local)** — drop `src/index.ts` into `.opencode/plugins/ironlint.ts`, or symlink for development.
- **Install (npm)** — once published, `"plugin": ["@christopherarter/ironlint-opencode"]` in `opencode.json`.
- **Requirements** — `ironlint` binary on PATH; Bun ≥ 1.1 (shipped with OpenCode).
- **How hooks resolve** — explain `tool.execute.after` semantics and the `event` filter.

---

## Phase 4 — Test

### Task 5: `adapters/opencode/tests/plugin.test.ts`

Bun-based test. Imports the plugin module directly, fakes a minimal `PluginInput` (with a real working `$` shell — Bun's), and drives the `tool.execute.after` hook against a fixture project.

```typescript
import { test, expect, beforeAll, afterAll } from "bun:test"
import { mkdtempSync, writeFileSync, rmSync, existsSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { $ } from "bun"
import IronLintPlugin from "../src/index.ts"

let project: string

beforeAll(async () => {
  project = mkdtempSync(join(tmpdir(), "ironlint-opencode-"))
  writeFileSync(join(project, ".ironlint.yml"), `
schema_version: 2
rules:
  no-debug:
    description: "no DEBUG markers"
    engine: script
    scope: ["*.txt"]
    severity: error
    script: "grep -nE 'DEBUG' {file} && exit 1 || exit 0"
`)
  await $`ironlint trust --config ${join(project, ".ironlint.yml")}`.quiet()
})

afterAll(() => { rmSync(project, { recursive: true, force: true }) })

test("clean file passes", async () => {
  const file = join(project, "clean.txt")
  writeFileSync(file, "ok\n")
  const hooks = await IronLintPlugin({ /* fake ctx */ } as any)
  await expect(hooks["tool.execute.after"]!(
    { tool: "edit", sessionID: "s", callID: "c", args: { filePath: file } },
    { title: "", output: "", metadata: {} },
  )).resolves.toBeUndefined()
})

test("dirty file blocks (throws)", async () => {
  const file = join(project, "dirty.txt")
  writeFileSync(file, "has DEBUG\n")
  const hooks = await IronLintPlugin({ /* fake ctx */ } as any)
  await expect(hooks["tool.execute.after"]!(
    { tool: "edit", sessionID: "s", callID: "c", args: { filePath: file } },
    { title: "", output: "", metadata: {} },
  )).rejects.toThrow(/ironlint/)
})

test("session.created clears stale session.json", async () => { /* … */ })
```

The test runs `ironlint` via PATH (CI must build the release binary and prepend `target/release` to PATH, same as the Claude Code adapter test).

---

## Phase 5 — Long-form doc + README + verification

### Task 6: `docs/adapters/opencode.md`

Mirror `docs/adapters/claude-code.md`: parity table, install paths, requirements, skills note (skills are not ported at 0.1c — document as a known gap with workaround), diagnostic.

### Task 7: Top-level README

Add OpenCode to the Adapters section:

```markdown
- **OpenCode** — `adapters/opencode/`. `tool.execute.after` + `session.idle` plugin. See [docs/adapters/opencode.md](docs/adapters/opencode.md).
```

Update Status to mention OpenCode.

### Task 8: Verify

```bash
cargo build --release
cargo test --workspace
PATH="$(pwd)/target/release:${PATH}" \
  bun test adapters/opencode/tests/plugin.test.ts
# Optional: bunx tsc --noEmit -p adapters/opencode/tsconfig.json
```

Acceptance:
- All cargo tests pass.
- Bun test passes (clean file passes, dirty file throws, session.created clears state).
- TypeScript type-checks (no `any` leaks in the plugin source other than `args: any` which is the hook signature).

---

## Known compromises at 0.1d

- **No skills.** The Claude Code adapter ships `/ironlint-init`, `/ironlint-author`, `/ironlint-review`. OpenCode has native skills via the `skill` tool but the discovery path is unsettled across versions. Skills are deferred; a follow-up can either (a) port the SKILL.md files into a shared `skills/` directory consumed by both adapters or (b) ship them via `@malhashemi/opencode-skills` install instructions.
- **No `apply_patch` interception.** Multi-file patches would need per-file extraction; this skip is documented in `docs/adapters/opencode.md` so the user knows large refactors via `apply_patch` are not gated.
- **`session.idle` throw → user-visible only.** Unlike Claude Code's Stop hook (exit 2 blocks the response), OpenCode's `session.idle` fires when the agent is already done. The plugin surfaces violations via `console.error` and a thrown error, which OpenCode renders to the user — but it does not retroactively prevent the response from being sent. This matches the practical UX of "session rules tell you what to fix next time."

## Hand-off

If a future adapter (Cursor, Continue, Codex) follows OpenCode's plugin model (JS/TS + lifecycle hooks), the structure here is the template. If the new host is MCP-based, defer to the `ironlint serve --mcp` work planned for 0.3.
