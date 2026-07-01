# `ironlint init` interactive onboarding — design

**Date:** 2026-07-01
**Status:** approved, pre-implementation
**Touches:** `ironlint-core::adapter` (new `git` module, `registry`/`ops` scope
handling), `ironlint-cli::commands::init` (`mod`/`onboard`/`render`), `ironlint-cli::cli`
**Builds on:** `docs/superpowers/specs/2026-07-01-init-onboarding-clarity-design.md`
(the plan-and-confirm flow this extends)

## Problem

The onboarding-clarity work made `ironlint init` *show* a per-harness plan before
installing, but the interactive path is still coarse:

- **No subset selection.** In a TTY with no `--harness`, init resolves the set
  to *every detected harness* and offers only a whole-plan `Proceed? [Y/n]`.
  There is no way to interactively install for a subset — the only per-harness
  control is the non-interactive `--harness` flag.
- **Scope is flag-only.** `Scope::Local` (project) is the default and `--global`
  the only lever. The interactive flow never surfaces the choice, so "install
  for this project only" is invisible in the UI. Worse, under local scope a
  harness with no per-project settings (reasonix) *silently* patches the global
  `~/.reasonix/settings.json` — "project only" is a quiet lie for it.
- **`on: [pre-commit]` never fires out of the box.** `ironlint check --event
  pre-commit --diff <diff>` works and dispatches once over the changed set, but
  nothing installs a git hook to invoke it. A user who adds a `pre-commit` check
  gets no enforcement until they hand-wire `.git/hooks/pre-commit` themselves.

## Goals

1. **Interactive harness multi-select.** In a TTY, present all four harnesses as
   checkboxes (detected ones pre-checked) and install exactly the chosen subset.
2. **Scope choice in the UI.** Offer `this project only` (default) vs `global`,
   and make "project only" *honest* (don't silently write global).
3. **Opt-in git pre-commit hook.** Install a `.git/hooks/pre-commit` that runs
   `ironlint check --event pre-commit`, so the pre-commit lifecycle works out of
   the box — without clobbering an existing hook.

Non-goals: changing the exit-code contract, the trust model, the verdict JSON,
or *what* a harness install writes. No new VCS beyond git. We *detect and skip*
framework-managed hooks (husky/lefthook/`core.hooksPath`); we do not manage them.

## Design

Every change is gated to the **interactive TTY path**. The non-interactive
paths — explicit `--harness`, `--yes`, and non-TTY — keep today's behavior
exactly, plus honor the two new flags. dialoguer is only ever called when both
stdin and stdout are terminals.

### 1. The interactive picker (CLI)

Add `dialoguer` (workspace dep) and a new pure-ish selection step that runs
*before* the plan is built, only when `opts.harnesses.is_empty() && !opts.yes &&
stdin+stdout are TTYs`:

1. **`MultiSelect`** — all four harnesses, each labeled `name` + `(detected)`
   when `is_detected`. Detected entries start checked. The user toggles with
   space, confirms with enter. Result: the selected harness names, tagged
   `Source::Requested` (an interactive pick is an explicit request).
2. **`Select` scope** — `this project only` (default, index 0) / `global`.
   **Skipped** when `--global` is passed (flag wins) or when the selected set is
   empty. Produces the `Scope` for this run.
3. **`Confirm` git hook** — shown only when the project root is a git repo
   (see §3) and neither `--git-hook` nor `--no-git-hook` was passed. Default
   *yes*. Produces `want_git_hook: bool`.

After the picker, the existing pipeline runs unchanged: build the plan → render
it (now with a git section, §4) → `Proceed? [Y/n]` (the existing `confirm_gate`
+ `parse_confirm`, preserving "preview exact writes before touching disk") →
apply. The two-step (pick, then confirm the rendered plan) is deliberate: the
picker chooses *intent*, the plan shows the *exact files*, the final Y/n gates
the write.

**Boundary & testability.** The dialoguer calls are a thin I/O shell —
untested, `IsTerminal`-gated, exactly like today's `read_line` in
`confirm_gate`. All decision logic lives in pure helpers that take plain data
and are unit-tested:

```rust
// pure: given flags + selection, resolve the run's scope
fn resolve_scope(global_flag: bool, picked: Option<Scope>) -> Scope

// pure: given flags + repo?, decide whether to attempt the git hook
fn want_git_hook(flag: Option<bool>, in_repo: bool, picked: Option<bool>) -> bool
```

This keeps each function under the cognitive-complexity cap and holds the ≥80%
region gate without trying to drive an interactive widget in tests.

### 2. Scope honesty (core + render)

Reasonix has no per-project settings file (`settings_local: |_| None`), so under
`Scope::Local` `settings_path` falls back to the global file. Under an explicit
"this project only" choice that is misleading.

**Decision:** under `Scope::Local`, a harness whose `settings_local` is `None`
is **skipped** with a surfaced reason rather than silently patched globally. The
plan renders it as a skip line, e.g.:

```
  reasonix     requested
    └ skipped   no per-project settings — re-run with global scope
```

Mechanism: `plan_install` already knows the harness kind; extend it to emit a
new `PlanStep::Skip { reason }` when local scope has no local target, and have
`apply` short-circuit that harness to an `InstallResult::Skipped(reason)`
without writing. `Scope::Global` is unaffected — reasonix installs normally
there. This makes the local/global distinction the plan shows match what lands.

*(Explicit `--harness reasonix` without `--global` follows the same rule, so the
flag and the picker agree.)*

### 3. Git pre-commit hook (new `ironlint-core::adapter::git`)

A standalone module — the git hook is harness-agnostic (it is about git, not any
coding agent), so it is **not** a `Harness` in the registry. Public surface:

```rust
pub enum GitHookAction { Install, Skip(String), Uninstall }

pub struct GitHookPlan { pub path: PathBuf, pub action: GitHookAction }

// resolve `.git/hooks` honoring worktrees + core.hooksPath; None if not a repo
pub fn hooks_dir(project_root: &Path) -> Option<PathBuf>;

// pure: the hook script body, with the absolute ironlint path baked in
pub fn precommit_script(ironlint_bin: &Path) -> String;

pub fn plan_git_hook(project_root: &Path, uninstall: bool) -> GitHookPlan;
pub fn install_git_hook(project_root: &Path, ironlint_bin: &Path) -> Result<InstallResult>;
pub fn uninstall_git_hook(project_root: &Path) -> Result<InstallResult>;
```

- **Repo + hooks dir resolution.** Shell out to `git rev-parse --git-path
  hooks` (run with `current_dir(project_root)`). It resolves the real hooks
  directory across worktrees/submodules *and* honors `core.hooksPath`, so a
  husky/lefthook setup that redirects hooks is seen. `git` absent or "not a
  repository" → `None` → the hook step is skipped everywhere (picker question is
  not shown; a passed `--git-hook` prints a one-line "not a git repo" notice).

- **Script.** Built by the pure `precommit_script(ironlint_bin)`:

  ```sh
  #!/bin/sh
  # ironlint pre-commit hook (managed by `ironlint init`; safe to delete)
  command -v "<ironlint_bin>" >/dev/null 2>&1 || exit 0
  diff=$(mktemp) || exit 0
  trap 'rm -f "$diff"' EXIT
  git diff --cached --no-color > "$diff"
  [ -s "$diff" ] || exit 0                 # nothing staged → pass
  "<ironlint_bin>" check --event pre-commit --diff "$diff"
  code=$?
  [ "$code" -eq 2 ] && exit 1              # block → abort commit
  if [ "$code" -eq 3 ] && [ "${IRONLINT_FAIL_CLOSED_ON_INTERNAL:-0}" = "1" ]; then
    exit 1
  fi
  exit 0                                    # 0/1, or fail-open 3 → allow commit
  ```

  The absolute `ironlint_bin` is resolved once at install time via
  `std::env::current_exe()` (mirrors how the claude JSON hook bakes its absolute
  `hook.sh` path). The exit map mirrors the claude adapter: only a real block
  (`2`) aborts the commit; internal errors (`3`) fail open unless the opt-in env
  is set; ironlint's own config error (`1`) and empty diff never block. The
  first-line marker identifies our hook for idempotent update + safe uninstall.

- **Conflict policy** (per the chosen "skip + snippet" rule):
  | on-disk `pre-commit` | action |
  |---|---|
  | absent | write ours → `Installed` |
  | present, **has our marker** | rewrite if bytes differ (`Updated`) else `AlreadyPresent` |
  | present, **no marker** (foreign) | **do not touch**; `Skipped("existing pre-commit hook")` + print a paste-in snippet |

  The snippet (printed once, after the result lines) tells the user how to chain
  ironlint into husky/lefthook or a hand-rolled hook:

  ```
  git · an existing pre-commit hook was left untouched. To enable ironlint at
        commit time, add this to it (or your husky/lefthook config):
          ironlint check --event pre-commit --diff <(git diff --cached); [ $? -ne 2 ]
  ```

- **Scope.** The git hook is always project-local — it lives in the repo's
  `.git/`. It is independent of the harness scope choice (a global harness
  install can still install a local git hook). `chmod +x` on write.

- **Uninstall.** `ironlint init --uninstall` calls `uninstall_git_hook`, which
  removes the file **only when our marker is present** — a foreign hook is never
  deleted.

### 4. Rendering (CLI `render.rs`)

After the harness tree, render one `git` section from the `GitHookPlan`:

```
  git          this project
    └ hook      ./.git/hooks/pre-commit
```

or, on conflict / not-a-repo, a skip line carrying the reason. The renderer
stays pure (structured plan in, string out) and TTY-gated for color, matching
the existing `render_plan`. `GitHookPlan` is threaded alongside the harness
plans; the skip reason is shown inline so the preview is truthful before the
final confirm.

### 5. CLI surface (`cli.rs`, `init/mod.rs`)

- New flags on `Init`: `--git-hook` and `--no-git-hook` (mutually exclusive,
  like `--no-hook`/`--hook-only`). `Options` gains `git_hook: Option<bool>`
  (`None` = ask/default, `Some(true/false)` = forced).
- Resolution: `--no-git-hook` → never; `--git-hook` → always (subject to §3's
  repo check); neither → default *yes* interactively in a repo, *no*
  non-interactively (so `--harness X` alone stays hook-only unless `--git-hook`
  is added).
- `dialoguer` added to `[workspace.dependencies]` and referenced from
  `ironlint-cli` (`.workspace = true`).

### 6. Baseline scaffold (`init/mod.rs`)

Add a *commented* pre-commit example to `BASELINE`, so the freshly-installed
hook has a documented path to usefulness — a check reading `$IRONLINT_FILES`
rather than stdin:

```yaml
# A pre-commit check runs once over the whole staged set. Unlike write checks,
# stdin is empty and the file list arrives in $IRONLINT_FILES:
#
#   no-todo-precommit:
#     files: ["src/**/*"]
#     on: [pre-commit]
#     run: 'for f in $IRONLINT_FILES; do ! grep -nE "TODO" "$f" || exit 1; done'
```

The default `no-fixme` / `no-merge-markers` checks stay **write-only** — they
grep stdin, which is empty at pre-commit, so promoting them would be a silent
no-op. The commented example is the correct pattern.

## Testing

- **Pure/unit (core):** `precommit_script` — asserts the marker, `--event
  pre-commit`, the `2 → exit 1` / `3 → fail-open` / empty-diff-guard mapping,
  and the baked bin path. `hooks_dir` — via a real `git init` tempdir (repo →
  `Some`, non-repo → `None`). Conflict classification — absent / marked /
  foreign against a tempdir hooks dir. `plan_git_hook` writes nothing.
- **Pure/unit (CLI):** `resolve_scope` and `want_git_hook` truth tables;
  reasonix-under-local yields a `Skip` plan step + `Skipped` result; render of
  the git section (install / skip / uninstall lines, color on+off).
- **E2E (`assert_cmd`, tempdir `git init`):**
  - `init --harness claude-code --git-hook --yes` → `.git/hooks/pre-commit`
    exists, is executable, contains the marker; harness install still lands.
  - Re-run is idempotent (`AlreadyPresent`, bytes identical).
  - Pre-seed a foreign `pre-commit` → run skips it, prints the snippet on
    stdout, and leaves the foreign bytes untouched.
  - `--uninstall` removes our hook but leaves a foreign one.
  - **End-to-end fire:** a config with an `on: [pre-commit]` check that blocks a
    staged `FIXME`, driven through `ironlint check --event pre-commit --diff`
    exactly as the installed hook would, returns exit 2 → proves the event path.
  - `--no-git-hook` writes no hook; `--git-hook` outside a git repo prints the
    notice and exits cleanly.
- The dialoguer interactive loop is not driven in tests (untested thin shell,
  `IsTerminal`-gated); logic coverage comes from the pure helpers above.

## Documented limitations

- Pre-commit checks read **working-tree** content, not the exact staged blob
  (existing `run_diff` behavior — a partially-staged file is checked as it is on
  disk). Out of scope to change here.
- We detect and skip framework-managed hooks; we do not integrate with or manage
  husky/lefthook. The printed snippet is the escape hatch.
- If `ironlint` is not on `PATH` (or at the baked path) when a GUI git client
  commits, the hook no-ops (`command -v` guard) — a safe fail-open, but silent.
