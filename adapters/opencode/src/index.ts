import type { Plugin } from "@opencode-ai/plugin"
import { existsSync, rmSync } from "node:fs"
import { join } from "node:path"

// OpenCode tools we gate. `apply_patch` is intentionally not gated at 0.1d
// (P2-14, deferred) — the opencode plugin SDK does not currently surface
// an `apply_patch` tool through `tool.execute.after`, and its multi-file
// patch format would need per-file extraction (split on `+++ b/<path>`
// boundaries, reissue `hector check --file` per file). See
// docs/adapters/opencode.md → "What it does NOT do" for the known-gap
// note. Tracked until the apply_patch tool is wired through the adapter.
const GATED_TOOLS = new Set(["edit", "write"])

type FileToolArgs = {
  filePath?: string
  oldString?: string
  newString?: string
  content?: string
}

/**
 * Hector OpenCode plugin.
 *
 * Mirrors the Claude Code adapter:
 *   - `tool.execute.after` on `edit`/`write` → `hector check --file <path>`,
 *      with the same exit-code contract (0 = pass/warn, 2 = block).
 *   - `event` filtering on `session.created` → clear stale `.hector/session.json`.
 *   - `event` filtering on `session.idle` → `hector check --session`.
 *
 * Hector itself is invoked as a child process via Bun's `$` API. The
 * plugin contains no rule logic — it's purely a translation layer between
 * OpenCode's lifecycle and the `hector` CLI.
 */
const HectorPlugin: Plugin = async ({ $, directory, worktree }) => {
  const projectRoot = worktree || directory
  const configPath = join(projectRoot, ".hector.yml")
  const sessionStatePath = join(projectRoot, ".hector", "session.json")

  // If the project isn't a hector project, register no hooks. Installing the
  // plugin in a non-hector project is a free, fast no-op.
  if (!existsSync(configPath)) {
    return {}
  }

  return {
    "tool.execute.after": async (input) => {
      if (!GATED_TOOLS.has(input.tool)) return

      const args = input.args as FileToolArgs | undefined
      const filePath = args?.filePath
      if (!filePath) return

      // 1. Record the edit into session state. Non-fatal: a flaky session
      //    record must never block the agent. We swallow all errors here.
      try {
        const diff = synthesizeDiff(filePath, args)
        await $`hector session record --dir ${projectRoot} --file ${filePath} --diff ${diff}`
          .quiet()
          .nothrow()
      } catch {
        // intentional: session recording is best-effort.
      }

      // 2. Gate the edit. Exit code contract (commands/check.rs):
      //      0 → pass or warn  (allow)
      //      2 → block         (reject — throw to surface to the agent)
      //      1 → internal      (log to stderr, allow — agent shouldn't be
      //                         blocked by an unrelated hector failure)
      const result =
        await $`hector check --file ${filePath} --config ${configPath} --format json`
          .quiet()
          .nothrow()

      if (result.exitCode === 2) {
        const verdict = result.stdout.toString().trim() || "rule violation"
        throw new Error(`hector blocked this edit:\n${verdict}`)
      }
      if (result.exitCode !== 0) {
        const stderr = result.stderr.toString().trim()
        console.error(
          `hector: internal error checking ${filePath} (exit ${result.exitCode})${stderr ? `: ${stderr}` : ""}`,
        )
      }
    },

    event: async ({ event }) => {
      // Cross-version compatibility: the `event` object's discriminant is the
      // `type` field. We only react to two values; everything else is ignored.
      const type = (event as { type?: string }).type
      if (type === undefined) return

      if (type === "session.created") {
        if (existsSync(sessionStatePath)) {
          try {
            rmSync(sessionStatePath, { force: true })
          } catch {
            // intentional: stale-state cleanup is best-effort.
          }
        }
        return
      }

      if (type === "session.idle") {
        if (!existsSync(sessionStatePath)) return

        const result =
          await $`hector check --session --config ${configPath} --format json`
            .quiet()
            .nothrow()

        if (result.exitCode === 2) {
          const verdict = result.stdout.toString().trim() || "session rule violation"
          // session.idle fires after the agent's response — we can't
          // retroactively block the turn. Surface to stderr; OpenCode renders
          // this to the user so they see what to fix next iteration.
          console.error(`hector: session check blocked:\n${verdict}`)
          throw new Error(`hector session check blocked:\n${verdict}`)
        }
        if (result.exitCode !== 0) {
          const stderr = result.stderr.toString().trim()
          console.error(
            `hector: internal error during session check (exit ${result.exitCode})${stderr ? `: ${stderr}` : ""}`,
          )
        }
      }
    },
  }
}

/**
 * Build a synthetic unified diff for an Edit/Write tool invocation.
 *
 * The OpenCode tool events don't include a real diff; we fake one from
 * the (oldString, newString) pair so `hector session record` can ingest
 * it. Two correctness concerns:
 *
 * 1. **Hunk-header counts (P1-8).** A literal `@@ -1 +1 @@` is wrong as
 *    soon as either side has more than one line — hector's diff parser
 *    uses the header's `new_start` to number added lines, so wrong
 *    counts produce wrong line numbers in downstream violations. Count
 *    lines on each side and emit `1,N` form whenever N > 1.
 *
 * 2. **Injection scrub (P1-9).** `oldString`/`newString` are arbitrary
 *    user content. Without escaping, a `newString` containing
 *    `\n+++ b/SECRET\n` becomes a real `+++ b/SECRET` header in the
 *    synthesized diff, fooling hector's parser into thinking the edit
 *    targets a different file. We prefix any line in the user-provided
 *    blocks that *looks* like a diff header with a backslash, which the
 *    parser does not recognize.
 *
 * Exported for unit testing — see `tests/synthesize_diff.test.ts`.
 */
export function synthesizeDiff(filePath: string, args: FileToolArgs): string {
  const old = args.oldString ?? ""
  const neu = args.newString ?? args.content ?? ""

  // Neutralize attacker-controlled lines that mimic diff headers. We act
  // on the prefixed block (after `-`/`+` is applied) so a malicious OLD
  // string like `-- a/SECRET` (which would become `--- a/SECRET` after
  // the `-` prefix) is also caught.
  const scrub = (s: string) =>
    s
      .split("\n")
      .map((l) => (/^(---|\+\+\+|@@) /.test(l) ? "\\" + l : l))
      .join("\n")

  const oldLines = old === "" ? 0 : old.split("\n").length
  const newLines = neu === "" ? 0 : neu.split("\n").length
  // Diff parsers expect `0,0` (no lines on this side) or `<start>,<count>`
  // when count != 1, or bare `<start>` when count == 1.
  const hunkOld = oldLines <= 1 ? "1" : `1,${oldLines}`
  const hunkNew = newLines <= 1 ? "1" : `1,${newLines}`
  const oldBlock =
    old === "" ? "" : old.split("\n").map((l) => "-" + l).join("\n") + "\n"
  const newBlock =
    neu === "" ? "" : neu.split("\n").map((l) => "+" + l).join("\n") + "\n"

  return `--- a/${filePath}\n+++ b/${filePath}\n@@ -${hunkOld} +${hunkNew} @@\n${scrub(oldBlock)}${scrub(newBlock)}`
}

export default HectorPlugin
