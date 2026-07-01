import { test, expect, beforeAll, afterAll } from "bun:test"
import { mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { $ } from "bun"
import IronLintPlugin from "../src/index.ts"

// End-to-end test of the OpenCode adapter plugin.
// Drives the plugin hooks directly with synthetic OpenCode-shaped input,
// against a real `.ironlint.yml` and the real `ironlint` binary on PATH.
//
// Requirements:
//   - `ironlint` binary on PATH (CI prepends target/release before running).

let project: string

const IRONLINT_YML = `checks:
  no-debug:
    files: ["*.txt"]
    run: "! grep -nE 'DEBUG'"
`

function fakeCtx(root: string) {
  // Cast through unknown — PluginInput has fields the plugin doesn't read
  // (`client`, `project`, `serverUrl`, `experimental_workspace`). The plugin
  // only touches `$`, `directory`, and `worktree`.
  return {
    $,
    directory: root,
    worktree: root,
  } as unknown as Parameters<typeof IronLintPlugin>[0]
}

beforeAll(async () => {
  project = mkdtempSync(join(tmpdir(), "ironlint-opencode-"))
  writeFileSync(join(project, ".ironlint.yml"), IRONLINT_YML)
  await $`ironlint trust --config ${join(project, ".ironlint.yml")}`.quiet()
})

afterAll(() => {
  rmSync(project, { recursive: true, force: true })
})

test("hooks no-op when .ironlint.yml is absent at load time", async () => {
  // Hooks are always registered so that a project that becomes an ironlint
  // project mid-session starts gating without an opencode restart. When
  // .ironlint.yml is missing, every invocation short-circuits silently.
  const empty = mkdtempSync(join(tmpdir(), "ironlint-opencode-empty-"))
  try {
    const hooks = await IronLintPlugin(fakeCtx(empty))
    expect(hooks["tool.execute.before"]).toBeDefined()
    const file = join(empty, "anything.txt")
    await expect(
      hooks["tool.execute.before"]!(
        { tool: "write", sessionID: "s", callID: "c" },
        { args: { filePath: file, content: "this has DEBUG\n" } },
      ),
    ).resolves.toBeUndefined()
    expect(existsSync(file)).toBe(false) // shadow-write never happened
  } finally {
    rmSync(empty, { recursive: true, force: true })
  }
})

test("gate activates when .ironlint.yml is created after plugin load", async () => {
  // Regression test for the silent-disable bug: opencode loads plugins once
  // at startup. If `.ironlint.yml` doesn't exist yet, the original plugin
  // returned `{}` and the gate was dead for the rest of the session. The
  // existsSync check now runs per-invocation, so late-init `ironlint init`
  // starts gating immediately.
  const root = mkdtempSync(join(tmpdir(), "ironlint-opencode-late-"))
  try {
    const hooks = await IronLintPlugin(fakeCtx(root))
    const file = join(root, "dirty.txt")

    // Sanity: no gating yet.
    await expect(
      hooks["tool.execute.before"]!(
        { tool: "write", sessionID: "s", callID: "c" },
        { args: { filePath: file, content: "this has DEBUG\n" } },
      ),
    ).resolves.toBeUndefined()

    // Now create + trust the config and re-invoke the SAME hook closure.
    writeFileSync(join(root, ".ironlint.yml"), IRONLINT_YML)
    await $`ironlint trust --config ${join(root, ".ironlint.yml")}`.quiet()

    await expect(
      hooks["tool.execute.before"]!(
        { tool: "write", sessionID: "s", callID: "c" },
        { args: { filePath: file, content: "this has DEBUG\n" } },
      ),
    ).rejects.toThrow(/ironlint blocked this edit/)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test("module exposes both default and named IronLintPlugin exports", async () => {
  // The opencode plugin docs consistently show named exports
  // (`export const MyPlugin = ...`). The published Claude Code adapter and
  // our own tests use the default import. Keep both alive so neither
  // loader pattern silently no-ops.
  const mod = await import("../src/index.ts")
  expect(typeof mod.default).toBe("function")
  expect(typeof mod.IronLintPlugin).toBe("function")
  expect(mod.default).toBe(mod.IronLintPlugin)
})

test("before-hook on clean Write content passes", async () => {
  const file = join(project, "clean-write.txt")
  rmSync(file, { force: true })
  const hooks = await IronLintPlugin(fakeCtx(project))
  await expect(
    hooks["tool.execute.before"]!(
      { tool: "write", sessionID: "s", callID: "c" },
      { args: { filePath: file, content: "ok\n" } },
    ),
  ).resolves.toBeUndefined()
  // Shadow-write must be undone on the pass path so opencode's real
  // write is what lands on disk.
  expect(existsSync(file)).toBe(false)
})

test("before-hook on Write with DEBUG blocks and leaves no file behind", async () => {
  const file = join(project, "dirty-write.txt")
  rmSync(file, { force: true })
  const hooks = await IronLintPlugin(fakeCtx(project))
  await expect(
    hooks["tool.execute.before"]!(
      { tool: "write", sessionID: "s", callID: "c" },
      { args: { filePath: file, content: "this has DEBUG\n" } },
    ),
  ).rejects.toThrow(/ironlint blocked this edit/)
  expect(existsSync(file)).toBe(false)
})

test("before-hook on Edit that would introduce DEBUG blocks; file is unchanged", async () => {
  const file = join(project, "edit-introduce-debug.txt")
  writeFileSync(file, "hello world\n")
  const hooks = await IronLintPlugin(fakeCtx(project))

  await expect(
    hooks["tool.execute.before"]!(
      { tool: "edit", sessionID: "s", callID: "c" },
      { args: { filePath: file, oldString: "world", newString: "DEBUG" } },
    ),
  ).rejects.toThrow(/ironlint blocked this edit/)

  expect(readFileSync(file, "utf8")).toBe("hello world\n")
})

test("before-hook handles opencode's native find/replace arg shape", async () => {
  // Regression: opencode's edit tool ships `find` / `replace` (and
  // `replaceAll`), not `oldString` / `newString`. The plugin was silently
  // falling into the Write branch with empty content and never seeing the
  // proposed content.
  const file = join(project, "edit-find-replace.txt")
  writeFileSync(file, "hello world\n")
  const hooks = await IronLintPlugin(fakeCtx(project))

  await expect(
    hooks["tool.execute.before"]!(
      { tool: "edit", sessionID: "s", callID: "c" },
      { args: { filePath: file, find: "world", replace: "DEBUG" } },
    ),
  ).rejects.toThrow(/ironlint blocked this edit/)

  expect(readFileSync(file, "utf8")).toBe("hello world\n")
})

test("before-hook honours replaceAll", async () => {
  const file = join(project, "edit-replace-all.txt")
  writeFileSync(file, "clean clean clean\n")
  const hooks = await IronLintPlugin(fakeCtx(project))

  await expect(
    hooks["tool.execute.before"]!(
      { tool: "edit", sessionID: "s", callID: "c" },
      { args: { filePath: file, find: "clean", replace: "DEBUG", replaceAll: true } },
    ),
  ).rejects.toThrow(/ironlint blocked this edit/)

  expect(readFileSync(file, "utf8")).toBe("clean clean clean\n")
})

test("before-hook on clean Edit passes and leaves file unchanged (opencode writes next)", async () => {
  const file = join(project, "edit-clean.txt")
  writeFileSync(file, "hello world\n")
  const hooks = await IronLintPlugin(fakeCtx(project))

  await expect(
    hooks["tool.execute.before"]!(
      { tool: "edit", sessionID: "s", callID: "c" },
      { args: { filePath: file, oldString: "world", newString: "there" } },
    ),
  ).resolves.toBeUndefined()

  // The shadow content must be restored so opencode's own write is the
  // canonical one. We only assert pre-state here; opencode does the real
  // write after the before-hook returns.
  expect(readFileSync(file, "utf8")).toBe("hello world\n")
})

test("before-hook skips gate when Edit's oldString is not in the file", async () => {
  // If we can't simulate the edit, we can't produce a faithful proposed
  // content — and opencode's Edit will fail anyway. Skip the gate rather
  // than write garbage.
  const file = join(project, "edit-no-match.txt")
  writeFileSync(file, "hello world\n")
  const hooks = await IronLintPlugin(fakeCtx(project))
  await expect(
    hooks["tool.execute.before"]!(
      { tool: "edit", sessionID: "s", callID: "c" },
      { args: { filePath: file, oldString: "nonexistent", newString: "DEBUG" } },
    ),
  ).resolves.toBeUndefined()
  expect(readFileSync(file, "utf8")).toBe("hello world\n")
})

test("before-hook ignores non-gated tools", async () => {
  const hooks = await IronLintPlugin(fakeCtx(project))
  await expect(
    hooks["tool.execute.before"]!(
      { tool: "read", sessionID: "s", callID: "c" },
      { args: { filePath: "anything" } },
    ),
  ).resolves.toBeUndefined()
  await expect(
    hooks["tool.execute.before"]!(
      { tool: "bash", sessionID: "s", callID: "c" },
      { args: { command: "ls" } },
    ),
  ).resolves.toBeUndefined()
})

test("before-hook no-ops when filePath is missing", async () => {
  const hooks = await IronLintPlugin(fakeCtx(project))
  await expect(
    hooks["tool.execute.before"]!(
      { tool: "edit", sessionID: "s", callID: "c" },
      { args: {} },
    ),
  ).resolves.toBeUndefined()
})

test("before-hook skips self-check of .ironlint.yml (R3)", async () => {
  // R3: editing the policy file itself used to invoke ironlint check on
  // a mid-edit file whose on-disk sha no longer matched `trust:`, which
  // failed the trust gate (exit 1) and surfaced a confusing "internal
  // error" to the user. The plugin must short-circuit by basename
  // before any ironlint invocation runs.
  //
  // To prove no ironlint invocation ran, we deliberately break the trust
  // hash so any `ironlint check` would log an "internal error" line to
  // console.error. A clean run means the basename short-circuit fired.
  const root = mkdtempSync(join(tmpdir(), "ironlint-opencode-policy-"))
  const errs: string[] = []
  const origErr = console.error
  console.error = (msg: unknown) => {
    errs.push(String(msg))
  }
  try {
    writeFileSync(join(root, ".ironlint.yml"), IRONLINT_YML)
    await $`ironlint trust --config ${join(root, ".ironlint.yml")}`.quiet()
    const current = readFileSync(join(root, ".ironlint.yml"), "utf8")
    writeFileSync(
      join(root, ".ironlint.yml"),
      current.replace(/sha256:[0-9a-f]+/, "sha256:0".repeat(64)),
    )

    const hooks = await IronLintPlugin(fakeCtx(root))
    const file = join(root, ".ironlint.yml")
    const beforeBytes = readFileSync(file, "utf8")

    await expect(
      hooks["tool.execute.before"]!(
        { tool: "write", sessionID: "s", callID: "c" },
        { args: { filePath: file, content: "anything\n" } },
      ),
    ).resolves.toBeUndefined()

    // No ironlint invocation: no "internal error" log, no trust-verify log.
    expect(errs.join("\n")).not.toContain("internal error")
    expect(errs.join("\n")).not.toContain("trust verify")
    // File untouched (shadow-write never happened).
    expect(readFileSync(file, "utf8")).toBe(beforeBytes)
  } finally {
    console.error = origErr
    rmSync(root, { recursive: true, force: true })
  }
})

test("before-hook skips self-check of .bully.yml (R3)", async () => {
  // Same R3 short-circuit, applied to the migration-source filename.
  // The fixture project has no .bully.yml on disk; the plugin must
  // recognize the basename and exit before attempting any shadow-write.
  const file = join(project, ".bully.yml")
  rmSync(file, { force: true })
  const hooks = await IronLintPlugin(fakeCtx(project))

  await expect(
    hooks["tool.execute.before"]!(
      { tool: "write", sessionID: "s", callID: "c" },
      { args: { filePath: file, content: "anything\n" } },
    ),
  ).resolves.toBeUndefined()

  // The plugin returned before shadow-writing the proposed content.
  expect(existsSync(file)).toBe(false)
})

test("before-hook skips self-check of bare relative .ironlint.yml (R3)", async () => {
  // Basename match must work even when filePath is a bare filename
  // (no directory component).
  const hooks = await IronLintPlugin(fakeCtx(project))
  await expect(
    hooks["tool.execute.before"]!(
      { tool: "write", sessionID: "s", callID: "c" },
      { args: { filePath: ".ironlint.yml", content: "anything\n" } },
    ),
  ).resolves.toBeUndefined()
})
