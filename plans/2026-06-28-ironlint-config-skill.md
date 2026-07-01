# ironlint-config authoring skill — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Teach every supported harness how to author ironlint gates by installing one canonical `ironlint-config` Agent-Skills `SKILL.md` during `ironlint init`, sourced from a single embedded guide, plus a `ironlint schema` command that prints it.

**Architecture:** A single `adapters/shared/ironlint-config/SKILL.md` is the only authoring source. It is `include_str!`-embedded into the binary, printed by a new `ironlint schema` command, and materialized as a real `SKILL.md` into each detected harness's skills directory by `ironlint init` (reusing the existing `adapter::materialize` primitives). The hand-maintained `ironlint-author` skill is retired.

**Tech Stack:** Rust (Cargo workspace: `ironlint-core` lib, `ironlint-cli` bin), clap, serde, `assert_cmd` for CLI tests, `tempfile` for fs tests.

## Global Constraints

- A gate is exactly `{ files, run }`; no engines, no severity, no `{file}` templating. The guide content must use the 0.3 gates vocabulary only (the drift guard forbids `engine:`, `severity`, `rule_id`, `passed_checks`, `violations`, `{file}`, `capabilities:`, `ironlint migrate`).
- Skill name and install dir are `ironlint-config`; skills install at `<skills_dir>/ironlint-config/SKILL.md`.
- CLI command name is `ironlint schema`.
- Per-harness skills dirs (project / global):
  - claude-code: `.claude/skills/` / `~/.claude/skills/`
  - pi: `.pi/skills/` / `~/.pi/agent/skills/`
  - opencode: `.opencode/skills/` / `~/.config/opencode/skills/`
  - reasonix: `.reasonix/skills/` / `~/.reasonix/skills/`
- opencode reads `.claude/skills/` too: when **both** claude-code and opencode are selected for **install**, skip opencode's skill write (dedup). Dedup applies to install only, not uninstall.
- Rust files under `crates/*/src/` must hit ≥80% region coverage; cognitive complexity per function ≤15 (clippy). Reuse `adapter::materialize` (`atomic_write`, `sha256_hex`, `write_sidecar`, `AdapterSidecar`, `CURRENT_ADAPTER_VERSION`) rather than re-implementing.
- `Cargo.lock` is gitignored — never commit it. Binary is `ironlint`.
- Commit message trailer on every commit:
  ```
  Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01KCX6mWmrFwyUeEA2FxhPVU
  ```

---

### Task 1: Canonical guide + retire `ironlint-author` + drift guard

**Files:**
- Create: `adapters/shared/ironlint-config/SKILL.md`
- Delete: `adapters/claude-code/skills/ironlint-author/SKILL.md` (and its now-empty `ironlint-author/` dir)
- Modify: `crates/ironlint-cli/tests/skills_gates_model.rs`

**Interfaces:**
- Produces: the file `adapters/shared/ironlint-config/SKILL.md` (later `include_str!`-ed by Tasks 2 and 3). Frontmatter `name: ironlint-config`. Body teaches the `{files, run}` gates model.

- [ ] **Step 1: Write the failing test**

Replace the body of `crates/ironlint-cli/tests/skills_gates_model.rs` with this (drops `ironlint-author` from the scanned set, adds the shared guide):

```rust
//! Drift guard: the shipped authoring skills must teach the 0.3 **gates**
//! model, never the retired pre-0.3 engine/severity/rules model.

use std::path::PathBuf;

fn repo_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn read(rel: &str) -> String {
    let path = repo_path(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Every authoring-related skill file shipped in the tree.
const SKILL_FILES: &[&str] = &[
    "adapters/shared/ironlint-config/SKILL.md",
    "adapters/claude-code/skills/ironlint/SKILL.md",
    "adapters/claude-code/skills/ironlint-init/SKILL.md",
    "adapters/claude-code/skills/ironlint-review/SKILL.md",
];

const RETIRED_TOKENS: &[&str] = &[
    "engine:",
    "severity",
    "rule_id",
    "passed_checks",
    "violations",
    "{file}",
    "capabilities:",
    "ironlint migrate",
];

#[test]
fn skills_contain_no_retired_engine_model_vocabulary() {
    for rel in SKILL_FILES {
        let body = read(rel);
        for token in RETIRED_TOKENS {
            assert!(
                !body.contains(token),
                "{rel} still teaches the retired model: contains `{token}`"
            );
        }
    }
}

#[test]
fn shared_guide_teaches_the_two_field_gate() {
    let body = read("adapters/shared/ironlint-config/SKILL.md");
    assert!(
        body.contains("name: ironlint-config"),
        "shared guide must carry Agent-Skills frontmatter `name: ironlint-config`"
    );
    for anchor in ["$IRONLINT_FILE", "run:", "files:", "exit 2"] {
        assert!(
            body.contains(anchor),
            "shared guide must teach the gates model: missing `{anchor}`"
        );
    }
}

#[test]
fn ironlint_author_skill_is_retired() {
    // The hand-maintained authoring skill was consolidated into the shared
    // guide; its file must be gone so there is no second source to drift.
    assert!(
        !repo_path("adapters/claude-code/skills/ironlint-author/SKILL.md").exists(),
        "ironlint-author/SKILL.md must be removed (consolidated into adapters/shared/ironlint-config)"
    );
}

#[test]
fn runtime_skill_describes_the_gates_verdict_shape() {
    let body = read("adapters/claude-code/skills/ironlint/SKILL.md");
    assert!(body.contains("blocks"), "ironlint/SKILL.md must describe the `blocks` verdict array");
    assert!(body.contains("\"gate\""), "ironlint/SKILL.md must key a block by `gate`");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p ironlint-cli --test skills_gates_model`
Expected: FAIL — `shared_guide_teaches_the_two_field_gate` panics reading the not-yet-created `adapters/shared/ironlint-config/SKILL.md`, and `ironlint_author_skill_is_retired` fails because the file still exists.

- [ ] **Step 3: Create the canonical guide**

Create `adapters/shared/ironlint-config/SKILL.md` with exactly this content:

```markdown
---
name: ironlint-config
description: Authors, modifies, or removes gates in an ironlint .ironlint.yml. Use when the user says "add an ironlint gate for X", "ban Y", "tighten <gate-id>", "stop gating <gate-id>", "remove <gate-id>", "change the scope of a gate", or asks how to write an ironlint config.
license: MIT
metadata:
  author: dynamik-dev
  version: 1.0.0
---

# Authoring ironlint gates

An ironlint policy lives in `.ironlint.yml` at the project root. A **gate** is exactly
two fields — there are no engines, severities, or output modes:

```yaml
gates:
  no-debug:
    files: "**/*.ts"          # glob, or a list of globs
    run: "! grep -nH 'DEBUG' \"$IRONLINT_FILE\" || exit 2"
```

- `files` — the glob(s) the gate watches. A bare pattern with no `/` (e.g.
  `*.py`) also matches at any depth.
- `run` — a shell command handed to `sh -c`. The gate **owns the verdict via its
  exit code**: exit `2` blocks the edit; `0` (and any other non-`2` code up to
  125) passes. `126`/`127`/timeout are treated as a broken gate, not a block.
- The path under check arrives as `$IRONLINT_FILE` (absolute). The proposed
  post-edit content arrives on **stdin**. `$IRONLINT_ROOT` (project root) and
  `$IRONLINT_EVENT` (`edit`/`write`/`pre-commit`/`manual`) are also set. There is
  no path templating — the path travels only as `$IRONLINT_FILE`, never spliced
  into `run`.
- On block, the gate's combined stdout+stderr becomes the message the agent
  sees, so make the command print why it blocked.

## Gate patterns

**Ban a pattern (grep).** Block when a forbidden string appears. `grep` exits `0`
on a match, `1` when clean, `≥2` on error — map those to the gate contract:

```yaml
  no-console-log:
    files: ["src/**/*.ts", "src/**/*.tsx"]
    run: "grep -nE 'console\\.log\\(' \"$IRONLINT_FILE\"; case $? in 0) exit 2;; 1) exit 0;; *) exit $?;; esac"
```

**Wrap a linter (stdin).** Feed the proposed content to a linter so the gate runs
pre-write. Most linters exit non-zero on findings; remap that to `2` to block:

```yaml
  ruff-check:
    files: ["**/*.py"]
    run: "ruff check --quiet --stdin-filename \"$IRONLINT_FILE\" - || exit 2"
```

**Multi-line scripts.** Use a YAML block scalar so newlines survive — a plain or
folded (`>`) scalar collapses them and can turn the whole script into one comment
that silently passes:

```yaml
  guard:
    files: "*.rs"
    run: |
      grep -q 'FORBIDDEN' "$IRONLINT_FILE" && exit 2
      exit 0
```

## Process

1. Read `.ironlint.yml` to see existing gates (if none exists, scaffold one with
   `ironlint init`).
2. Draft the gate: `files` scope + a `run` command that exits `2` to block.
3. Build two fixtures: a **dirty** file the gate should block, and a **clean** one
   it should pass.
4. Test each by feeding the fixture's content on stdin and isolating the gate:
   ```bash
   ironlint check --file dirty.py --content - --gate ruff-check < dirty.py ; echo "dirty exit: $?"   # expect 2
   ironlint check --file clean.py --content - --gate ruff-check < clean.py ; echo "clean exit: $?"   # expect 0
   ```
5. Verify the gate exits `2` on dirty input and `0` on clean input.
6. If both hold, write the gate into `.ironlint.yml`.
7. Run `ironlint trust` to re-bless the config — edits invalidate the trust
   fingerprint, so checks refuse to run until you do.

## Test before write

Always test the gate against a fixture BEFORE writing to `.ironlint.yml`. A gate
that doesn't exit `2` on dirty input is worse than no gate — it gives false
confidence. A gate that exits `2` on clean input blocks every edit in scope.
```

- [ ] **Step 4: Delete the retired skill**

Run: `git rm -r adapters/claude-code/skills/ironlint-author`
Expected: removes `adapters/claude-code/skills/ironlint-author/SKILL.md`.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p ironlint-cli --test skills_gates_model`
Expected: PASS (all four tests).

- [ ] **Step 6: Commit**

```bash
git add adapters/shared/ironlint-config/SKILL.md crates/ironlint-cli/tests/skills_gates_model.rs
git rm -r adapters/claude-code/skills/ironlint-author
git commit -m "$(cat <<'EOF'
feat(skills): add canonical ironlint-config authoring guide; retire ironlint-author

One harness-agnostic gate-authoring guide becomes the single source. The
hand-maintained claude-code ironlint-author skill is consolidated into it. The
drift guard now scans the shared guide.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KCX6mWmrFwyUeEA2FxhPVU
EOF
)"
```

---

### Task 2: `ironlint schema` command

**Files:**
- Create: `crates/ironlint-cli/src/commands/schema.rs`
- Modify: `crates/ironlint-cli/src/commands/mod.rs`
- Modify: `crates/ironlint-cli/src/cli.rs:16-131` (add the `Schema` variant)
- Modify: `crates/ironlint-cli/src/main.rs:12-66` (dispatch)
- Test: `crates/ironlint-cli/tests/cli_schema.rs` (new)

**Interfaces:**
- Consumes: `adapters/shared/ironlint-config/SKILL.md` (from Task 1) via `include_str!`.
- Produces: `commands::schema::run() -> anyhow::Result<i32>`; `Command::Schema`.

- [ ] **Step 1: Write the failing unit test**

Create `crates/ironlint-cli/src/commands/schema.rs` with ONLY the test module first (so the test compiles against a stub):

```rust
//! `ironlint schema` — print the canonical gate-authoring guide.
//!
//! Embeds `adapters/shared/ironlint-config/SKILL.md` and prints its body (YAML
//! frontmatter stripped) to stdout. Read-only; never loads or trusts a config.

use anyhow::Result;

const GUIDE: &str = include_str!("../../../../adapters/shared/ironlint-config/SKILL.md");

/// Strip a leading `--- ... ---` YAML frontmatter block, returning the body.
/// Returns the input unchanged when there is no frontmatter.
fn strip_frontmatter(s: &str) -> &str {
    let Some(rest) = s.strip_prefix("---\n") else {
        return s;
    };
    match rest.find("\n---\n") {
        Some(idx) => rest[idx + "\n---\n".len()..].trim_start_matches('\n'),
        None => s,
    }
}

pub fn run() -> Result<i32> {
    print!("{}", strip_frontmatter(GUIDE));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_frontmatter_block() {
        let doc = "---\nname: x\ndescription: y\n---\n\n# Body\ntext\n";
        assert_eq!(strip_frontmatter(doc), "# Body\ntext\n");
    }

    #[test]
    fn passes_through_when_no_frontmatter() {
        let doc = "# Body\nno frontmatter\n";
        assert_eq!(strip_frontmatter(doc), doc);
    }

    #[test]
    fn passes_through_on_unterminated_frontmatter() {
        let doc = "---\nname: x\nno closing fence\n";
        assert_eq!(strip_frontmatter(doc), doc);
    }

    #[test]
    fn embedded_guide_has_no_frontmatter_after_strip() {
        // The real guide starts with frontmatter; the printed body must not.
        assert!(!strip_frontmatter(GUIDE).starts_with("---"));
        assert!(strip_frontmatter(GUIDE).contains("$IRONLINT_FILE"));
    }
}
```

- [ ] **Step 2: Register the module and run the unit test (expect compile error first, then pass)**

Add to `crates/ironlint-cli/src/commands/mod.rs`:

```rust
pub mod schema;
```

Run: `cargo test -p ironlint-cli commands::schema`
Expected: the four `strip_frontmatter` unit tests PASS. (`run()` is unused for now — that's fine; it's `pub`.)

- [ ] **Step 3: Add the CLI variant**

In `crates/ironlint-cli/src/cli.rs`, add a new variant to `enum Command` (after `ShowResolvedConfig { … }`, before the closing `}` at line 131):

```rust
    /// Print the canonical gate-authoring guide (the `.ironlint.yml` schema and
    /// patterns). Read-only.
    Schema,
```

- [ ] **Step 4: Dispatch it**

In `crates/ironlint-cli/src/main.rs`, add an arm to the `match cli.command` (after the `ShowResolvedConfig` arm, before the closing `}` at line 66):

```rust
        Command::Schema => commands::schema::run()?,
```

- [ ] **Step 5: Write the failing CLI integration test**

Create `crates/ironlint-cli/tests/cli_schema.rs` (uses only `assert_cmd` + stdout
bytes — no `predicates` dependency needed):

```rust
use assert_cmd::Command;

fn schema_stdout() -> String {
    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .arg("schema")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).expect("schema output is utf8")
}

#[test]
fn schema_prints_the_authoring_guide() {
    let s = schema_stdout();
    assert!(s.contains("$IRONLINT_FILE"), "guide must mention $IRONLINT_FILE:\n{s}");
    assert!(s.contains("exit 2"), "guide must mention the exit-2 contract:\n{s}");
}

#[test]
fn schema_output_has_no_yaml_frontmatter() {
    let s = schema_stdout();
    assert!(
        !s.starts_with("---"),
        "frontmatter must be stripped from `ironlint schema` output, got:\n{s}"
    );
}
```

- [ ] **Step 6: Run the integration test to verify it passes**

Run: `cargo test -p ironlint-cli --test cli_schema`
Expected: PASS (both tests). `assert_cmd` is already a dev-dependency used across the CLI tests; no new dependency is required.

- [ ] **Step 7: Commit**

```bash
git add crates/ironlint-cli/src/commands/schema.rs crates/ironlint-cli/src/commands/mod.rs \
        crates/ironlint-cli/src/cli.rs crates/ironlint-cli/src/main.rs crates/ironlint-cli/tests/cli_schema.rs
git commit -m "$(cat <<'EOF'
feat(cli): `ironlint schema` prints the gate-authoring guide

Embeds the canonical ironlint-config guide and prints its body (frontmatter
stripped). Read-only; the universal authoring fallback for any harness.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KCX6mWmrFwyUeEA2FxhPVU
EOF
)"
```

---

### Task 3: Registry `SkillSpec` + per-harness skill dirs

**Files:**
- Modify: `crates/ironlint-core/src/adapter/registry.rs`
- Modify: `crates/ironlint-core/src/adapter/mod.rs:7-13,67-76` (re-export `SkillSpec`, `SKILL_NAME`; add `skill` field to `Harness`)

**Interfaces:**
- Consumes: `AdapterEnv` (fields `home`, `config_home`, `project_root`); `adapters/shared/ironlint-config/SKILL.md`.
- Produces:
  - `pub struct SkillSpec { pub dir_local: fn(&AdapterEnv) -> PathBuf, pub dir_global: fn(&AdapterEnv) -> PathBuf, pub source: &'static str }`
  - `pub const SKILL_NAME: &str = "ironlint-config";`
  - `Harness` gains a `pub skill: SkillSpec` field.
  - Each of the 4 harnesses in `all_harnesses()` carries its `skill`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/ironlint-core/src/adapter/registry.rs`:

```rust
    #[test]
    fn skill_dirs_resolve_per_harness() {
        let e = env_with("/home/u", "/home/u/proj");
        let by = |name: &str| {
            all_harnesses().into_iter().find(|h| h.name == name).unwrap().skill
        };
        // claude-code
        let c = by("claude-code");
        assert_eq!((c.dir_local)(&e), PathBuf::from("/home/u/proj/.claude/skills"));
        assert_eq!((c.dir_global)(&e), PathBuf::from("/home/u/.claude/skills"));
        // pi
        let p = by("pi");
        assert_eq!((p.dir_local)(&e), PathBuf::from("/home/u/proj/.pi/skills"));
        assert_eq!((p.dir_global)(&e), PathBuf::from("/home/u/.pi/agent/skills"));
        // opencode (global lives under config_home)
        let o = by("opencode");
        assert_eq!((o.dir_local)(&e), PathBuf::from("/home/u/proj/.opencode/skills"));
        assert_eq!((o.dir_global)(&e), PathBuf::from("/home/u/.config/opencode/skills"));
        // reasonix
        let r = by("reasonix");
        assert_eq!((r.dir_local)(&e), PathBuf::from("/home/u/proj/.reasonix/skills"));
        assert_eq!((r.dir_global)(&e), PathBuf::from("/home/u/.reasonix/skills"));
    }

    #[test]
    fn every_harness_ships_the_same_skill_source() {
        for h in all_harnesses() {
            assert!(h.skill.source.contains("name: ironlint-config"), "{} skill source wrong", h.name);
        }
    }
```

(Note: `env_with` already exists in this test module and sets `config_home = "{home}/.config"`.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p ironlint-core adapter::registry`
Expected: FAIL to compile — `Harness` has no `skill` field, `SkillSpec` undefined.

- [ ] **Step 3: Add `SkillSpec`, the source const, and the name const**

In `crates/ironlint-core/src/adapter/registry.rs`, after the existing embedded-artifact consts (around line 11), add:

```rust
const IRONLINT_CONFIG_SKILL: &str =
    include_str!("../../../../adapters/shared/ironlint-config/SKILL.md");
```

After the `PluginSpec` struct definition, add:

```rust
/// Where a harness discovers `SKILL.md` files, and the shared skill bytes.
#[derive(Clone, Copy)]
pub struct SkillSpec {
    pub dir_local: fn(&AdapterEnv) -> PathBuf,
    pub dir_global: fn(&AdapterEnv) -> PathBuf,
    pub source: &'static str,
}
```

- [ ] **Step 4: Define per-harness `SkillSpec` consts**

In `crates/ironlint-core/src/adapter/registry.rs`, add four consts (next to `CLAUDE`, `REASONIX`, `PI`, `OPENCODE`):

```rust
const CLAUDE_SKILL: SkillSpec = SkillSpec {
    dir_local: |e| e.project_root.join(".claude").join("skills"),
    dir_global: |e| e.home.join(".claude").join("skills"),
    source: IRONLINT_CONFIG_SKILL,
};
const REASONIX_SKILL: SkillSpec = SkillSpec {
    dir_local: |e| e.project_root.join(".reasonix").join("skills"),
    dir_global: |e| e.home.join(".reasonix").join("skills"),
    source: IRONLINT_CONFIG_SKILL,
};
const PI_SKILL: SkillSpec = SkillSpec {
    dir_local: |e| e.project_root.join(".pi").join("skills"),
    dir_global: |e| e.home.join(".pi").join("agent").join("skills"),
    source: IRONLINT_CONFIG_SKILL,
};
const OPENCODE_SKILL: SkillSpec = SkillSpec {
    dir_local: |e| e.project_root.join(".opencode").join("skills"),
    dir_global: |e| e.config_home.join("opencode").join("skills"),
    source: IRONLINT_CONFIG_SKILL,
};
```

- [ ] **Step 5: Add the `skill` field to `Harness` and populate it**

In `crates/ironlint-core/src/adapter/mod.rs`, add to `pub struct Harness` (lines 72-76):

```rust
pub struct Harness {
    pub name: &'static str,
    pub kind: HarnessKind,
    pub restart_hint: &'static str,
    pub skill: SkillSpec,
}
```

Update the re-export on line 13 to add `SkillSpec` and `SKILL_NAME`:

```rust
pub use registry::{all_harnesses, JsonHookSpec, PluginSpec, SkillSpec, SKILL_NAME};
```

In `crates/ironlint-core/src/adapter/registry.rs`, add the name const near the top:

```rust
/// Skill name and install-dir leaf for the authoring skill.
pub const SKILL_NAME: &str = "ironlint-config";
```

Then add `skill:` to each of the four `Harness { … }` literals in `all_harnesses()`:

```rust
        Harness {
            name: "claude-code",
            kind: HarnessKind::JsonHook(CLAUDE),
            restart_hint: "Reload Claude Code (or restart) — it picks up settings.json hooks.",
            skill: CLAUDE_SKILL,
        },
        Harness {
            name: "reasonix",
            kind: HarnessKind::JsonHook(REASONIX),
            restart_hint: "Restart Reasonix so it reloads settings.",
            skill: REASONIX_SKILL,
        },
        Harness {
            name: "pi",
            kind: HarnessKind::Plugin(PI),
            restart_hint: "Restart pi so it loads the new extension.",
            skill: PI_SKILL,
        },
        Harness {
            name: "opencode",
            kind: HarnessKind::Plugin(OPENCODE),
            restart_hint: "Restart opencode so it loads the new plugin.",
            skill: OPENCODE_SKILL,
        },
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p ironlint-core adapter::registry`
Expected: PASS (`skill_dirs_resolve_per_harness`, `every_harness_ships_the_same_skill_source`, plus the existing registry tests).

- [ ] **Step 7: Commit**

```bash
git add crates/ironlint-core/src/adapter/registry.rs crates/ironlint-core/src/adapter/mod.rs
git commit -m "$(cat <<'EOF'
feat(adapter): per-harness SkillSpec + embedded ironlint-config skill

Each harness gains its project/global SKILL.md discovery dir and the shared
authoring-skill bytes, embedded via include_str!.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KCX6mWmrFwyUeEA2FxhPVU
EOF
)"
```

---

### Task 4: Skill install / uninstall in `ops.rs`

**Files:**
- Modify: `crates/ironlint-core/src/adapter/ops.rs`
- Modify: `crates/ironlint-core/src/adapter/mod.rs:12` (re-export `install_skill`, `uninstall_skill`)

**Interfaces:**
- Consumes: `Harness` (with `skill: SkillSpec`), `SkillSpec`, `SKILL_NAME`, `Scope`, `AdapterEnv`, `InstallResult`, `InstallOutcome`, and `materialize::{atomic_write, sha256_hex, write_sidecar, AdapterSidecar}`, `CURRENT_ADAPTER_VERSION`.
- Produces:
  - `pub fn install_skill(h: &Harness, env: &AdapterEnv, scope: Scope, dry_run: bool) -> Result<InstallOutcome>`
  - `pub fn uninstall_skill(h: &Harness, env: &AdapterEnv, scope: Scope, dry_run: bool) -> Result<InstallOutcome>`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/ironlint-core/src/adapter/ops.rs` (the helpers `harness(name)` and `env(tmp)` already exist there):

```rust
    #[test]
    fn install_skill_writes_skill_md_and_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let out = install_skill(&harness("pi"), &e, Scope::Local, false).unwrap();
        assert!(matches!(out.result, InstallResult::Installed));
        let skill = e.project_root.join(".pi/skills/ironlint-config/SKILL.md");
        assert!(skill.exists(), "SKILL.md must land at {}", skill.display());
        assert!(crate::adapter::read_sidecar(skill.parent().unwrap()).unwrap().is_some());
    }

    #[test]
    fn install_skill_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install_skill(&harness("pi"), &e, Scope::Local, false).unwrap();
        let again = install_skill(&harness("pi"), &e, Scope::Local, false).unwrap();
        assert!(matches!(again.result, InstallResult::AlreadyPresent));
    }

    #[test]
    fn install_skill_changed_content_is_updated() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install_skill(&harness("pi"), &e, Scope::Local, false).unwrap();
        let f = e.project_root.join(".pi/skills/ironlint-config/SKILL.md");
        std::fs::write(&f, b"// tampered").unwrap();
        let again = install_skill(&harness("pi"), &e, Scope::Local, false).unwrap();
        assert!(matches!(again.result, InstallResult::Updated));
    }

    #[test]
    fn install_skill_dry_run_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let out = install_skill(&harness("pi"), &e, Scope::Local, true).unwrap();
        assert!(matches!(out.result, InstallResult::DryRun(_)));
        assert!(!e.project_root.join(".pi/skills/ironlint-config/SKILL.md").exists());
    }

    #[test]
    fn uninstall_skill_removes_the_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install_skill(&harness("pi"), &e, Scope::Local, false).unwrap();
        let dir = e.project_root.join(".pi/skills/ironlint-config");
        assert!(dir.exists());
        uninstall_skill(&harness("pi"), &e, Scope::Local, false).unwrap();
        assert!(!dir.exists(), "uninstall must remove the skill dir");
    }

    #[test]
    fn install_skill_global_uses_home_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install_skill(&harness("pi"), &e, Scope::Global, false).unwrap();
        // pi global skills dir is ~/.pi/agent/skills
        assert!(e.home.join(".pi/agent/skills/ironlint-config/SKILL.md").exists());
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p ironlint-core adapter::ops`
Expected: FAIL to compile — `install_skill` / `uninstall_skill` undefined.

- [ ] **Step 3: Implement the install/uninstall functions**

In `crates/ironlint-core/src/adapter/ops.rs`, add to the imports at the top:

```rust
use crate::adapter::registry::SkillSpec;
use crate::adapter::SKILL_NAME;
```

(Adjust the existing `use crate::adapter::registry::{JsonHookSpec, PluginSpec};` to `{JsonHookSpec, PluginSpec, SkillSpec};` rather than a second `use`.)

Add these functions (place them after `install_plugin`, near line 178):

```rust
fn skill_base(spec: &SkillSpec, env: &AdapterEnv, scope: Scope) -> PathBuf {
    match scope {
        Scope::Local => (spec.dir_local)(env),
        Scope::Global => (spec.dir_global)(env),
    }
}

pub fn install_skill(
    h: &Harness,
    env: &AdapterEnv,
    scope: Scope,
    dry_run: bool,
) -> Result<InstallOutcome> {
    let dir = skill_base(&h.skill, env, scope).join(SKILL_NAME);
    let file = dir.join("SKILL.md");
    let result = install_skill_file(&file, &dir, h.skill.source.as_bytes(), dry_run)?;
    Ok(InstallOutcome { harness: h.name, result, hint: h.restart_hint })
}

fn install_skill_file(
    file: &Path,
    dir: &Path,
    bytes: &[u8],
    dry_run: bool,
) -> Result<InstallResult> {
    if dry_run {
        return Ok(InstallResult::DryRun(vec![format!("write {}", file.display())]));
    }
    let existed = file.exists();
    if existed {
        if let Ok(cur) = std::fs::read(file) {
            if cur == bytes {
                return Ok(InstallResult::AlreadyPresent);
            }
        }
    }
    atomic_write(file, bytes)?;
    let mut files = BTreeMap::new();
    files.insert("SKILL.md".to_string(), sha256_hex(bytes));
    write_sidecar(dir, &AdapterSidecar { version: CURRENT_ADAPTER_VERSION, files })?;
    Ok(if existed { InstallResult::Updated } else { InstallResult::Installed })
}

pub fn uninstall_skill(
    h: &Harness,
    env: &AdapterEnv,
    scope: Scope,
    dry_run: bool,
) -> Result<InstallOutcome> {
    let dir = skill_base(&h.skill, env, scope).join(SKILL_NAME);
    let result = if dry_run {
        InstallResult::DryRun(vec![format!("remove {}", dir.display())])
    } else {
        let _ = std::fs::remove_dir_all(&dir);
        InstallResult::Installed
    };
    Ok(InstallOutcome { harness: h.name, result, hint: h.restart_hint })
}
```

- [ ] **Step 4: Re-export the new functions**

In `crates/ironlint-core/src/adapter/mod.rs`, update the `ops` re-export (line 12):

```rust
pub use ops::{
    install, install_skill, status, uninstall, uninstall_skill, HarnessStatus, InstallOutcome,
    InstallResult,
};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p ironlint-core adapter::ops`
Expected: PASS (six new tests + existing ops tests).

- [ ] **Step 6: Commit**

```bash
git add crates/ironlint-core/src/adapter/ops.rs crates/ironlint-core/src/adapter/mod.rs
git commit -m "$(cat <<'EOF'
feat(adapter): install/uninstall the ironlint-config skill

Materializes the shared SKILL.md into a harness's skills dir with an
integrity sidecar, idempotent on re-run; uninstall strips the dir. Reuses
the existing materialize primitives.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KCX6mWmrFwyUeEA2FxhPVU
EOF
)"
```

---

### Task 5: Onboard wiring + dedup + outcome reporting

**Files:**
- Modify: `crates/ironlint-cli/src/commands/init/onboard.rs`
- Test: `crates/ironlint-cli/tests/cli_init.rs` (add integration assertions)

**Interfaces:**
- Consumes: `install_skill`, `uninstall_skill` (Task 4); existing `run_hook_phase` structure, `format_outcome`, `print_outcome`, `InstallResult`.
- Produces: skill install folded into `run_hook_phase`; `should_install_skill(name, &selected) -> bool`; `format_skill_outcome(harness, &InstallResult, uninstalling) -> Vec<String>`.

- [ ] **Step 1: Write the failing unit tests**

Add to the `#[cfg(test)] mod tests` in `crates/ironlint-cli/src/commands/init/onboard.rs`:

```rust
    #[test]
    fn dedup_skips_opencode_skill_when_claude_present() {
        let sel = vec!["claude-code".to_string(), "opencode".to_string()];
        assert!(!should_install_skill("opencode", &sel));
        assert!(should_install_skill("claude-code", &sel));
        assert!(should_install_skill("pi", &sel));
    }

    #[test]
    fn dedup_installs_opencode_skill_when_claude_absent() {
        let sel = vec!["opencode".to_string(), "pi".to_string()];
        assert!(should_install_skill("opencode", &sel));
    }

    #[test]
    fn format_skill_outcome_covers_variants() {
        use ironlint_core::adapter::InstallResult::*;
        assert!(format_skill_outcome("pi", &Installed, false)[0].contains("skill installed"));
        assert!(format_skill_outcome("pi", &Installed, true)[0].contains("skill removed"));
        assert!(format_skill_outcome("pi", &Updated, false)[0].contains("skill updated"));
        assert!(format_skill_outcome("pi", &AlreadyPresent, false)[0].contains("skill already present"));
        let dr = format_skill_outcome("pi", &DryRun(vec!["write a".to_string()]), false);
        assert_eq!(dr.len(), 2);
        assert!(dr[0].contains("skill dry-run"));
        assert!(dr[1].contains("write a"));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p ironlint-cli commands::init::onboard`
Expected: FAIL to compile — `should_install_skill` / `format_skill_outcome` undefined.

- [ ] **Step 3: Add the dedup helper and the skill formatter**

In `crates/ironlint-cli/src/commands/init/onboard.rs`, add:

```rust
/// opencode is Claude-compatible and also reads `.claude/skills/`; when
/// claude-code is in the same install set, skip opencode's own skill write so
/// opencode doesn't load the same-named skill twice. Dedup applies to install
/// only.
fn should_install_skill(name: &str, selected: &[String]) -> bool {
    !(name == "opencode" && selected.iter().any(|n| n == "claude-code"))
}

fn format_skill_outcome(harness: &str, result: &InstallResult, uninstalling: bool) -> Vec<String> {
    match result {
        InstallResult::Installed if uninstalling => vec![format!("  {harness:<12} skill removed")],
        InstallResult::Installed => vec![format!("  {harness:<12} skill installed")],
        InstallResult::Updated => vec![format!("  {harness:<12} skill updated")],
        InstallResult::AlreadyPresent => vec![format!("  {harness:<12} skill already present")],
        InstallResult::Skipped(why) => vec![format!("  {harness:<12} skill skipped: {why}")],
        InstallResult::Failed(why) => vec![format!("  {harness:<12} skill failed: {why}")],
        InstallResult::DryRun(plan) => {
            let mut lines = vec![format!("  {harness:<12} skill dry-run:")];
            lines.extend(plan.iter().map(|l| format!("      {l}")));
            lines
        }
    }
}

fn print_skill_outcome(harness: &str, result: &InstallResult, uninstalling: bool) {
    for line in format_skill_outcome(harness, result, uninstalling) {
        println!("{line}");
    }
}
```

Update the imports at the top of `onboard.rs` to add `install_skill, uninstall_skill`:

```rust
use ironlint_core::adapter::{
    all_harnesses, detect, install, install_skill, uninstall, uninstall_skill, AdapterEnv,
    Harness, InstallResult, Scope,
};
```

- [ ] **Step 4: Fold skill install into `run_hook_phase`**

In `crates/ironlint-cli/src/commands/init/onboard.rs`, replace the per-harness loop body in `run_hook_phase` so each harness installs its hook AND (subject to dedup) its skill, folding both into `any_ok`/`any_fail`:

```rust
    let mut any_ok = false;
    let mut any_fail = false;
    for h in selected {
        // 1. Hook.
        let outcome = if opts.uninstall {
            uninstall(h, env, scope, opts.dry_run)
        } else {
            install(h, env, scope, opts.dry_run)
        };
        match outcome {
            Ok(o) => {
                any_ok = true;
                print_outcome(o.harness, &o.result, o.hint, opts.uninstall);
            }
            Err(e) => {
                any_fail = true;
                println!("  {:<12} failed: {e:#}", h.name);
            }
        }
        // 2. Authoring skill. Uninstall removes every harness's own dir; install
        //    dedups opencode against claude-code's copy.
        let do_skill = opts.uninstall || should_install_skill(h.name, &names);
        if do_skill {
            let s = if opts.uninstall {
                uninstall_skill(h, env, scope, opts.dry_run)
            } else {
                install_skill(h, env, scope, opts.dry_run)
            };
            match s {
                Ok(o) => {
                    any_ok = true;
                    print_skill_outcome(o.harness, &o.result, opts.uninstall);
                }
                Err(e) => {
                    any_fail = true;
                    println!("  {:<12} skill failed: {e:#}", h.name);
                }
            }
        }
    }
    Ok(if any_fail && !any_ok { 3 } else { 0 })
```

(`names` is the `Vec<String>` already computed earlier in `run_hook_phase`; `selected` is the `Vec<&Harness>` built from it. Both are already in scope.)

- [ ] **Step 5: Run the unit tests to verify they pass**

Run: `cargo test -p ironlint-cli commands::init::onboard`
Expected: PASS.

- [ ] **Step 6: Add an integration test for the dry-run plan**

Add to `crates/ironlint-cli/tests/cli_init.rs`:

```rust
#[test]
fn init_dry_run_plans_skill_installs_for_explicit_harnesses() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("ironlint")
        .unwrap()
        .args(["init", "--dir", dir.path().to_str().unwrap(),
               "--harness", "pi", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("pi") && s.contains("skill dry-run"),
        "dry-run must plan the pi skill install:\n{s}");
    assert!(s.contains("skills/ironlint-config/SKILL.md"),
        "dry-run must name the skill path:\n{s}");
}

#[test]
fn init_dedups_opencode_skill_when_claude_also_selected() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("ironlint")
        .unwrap()
        .args(["init", "--dir", dir.path().to_str().unwrap(),
               "--harness", "claude-code", "--harness", "opencode", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    // Assert on paths (spacing-insensitive): claude's skill is planned;
    // opencode's own skill dir is not (it reads claude's copy).
    assert!(
        s.contains(".claude/skills/ironlint-config/SKILL.md"),
        "claude-code skill must be planned:\n{s}"
    );
    assert!(
        !s.contains(".opencode/skills/ironlint-config"),
        "opencode skill must be deduped against claude's copy:\n{s}"
    );
}
```

(`--dry-run` still scaffolds `.ironlint.yml`; that's expected and harmless in a tempdir.)

- [ ] **Step 7: Run the integration tests**

Run: `cargo test -p ironlint-cli --test cli_init`
Expected: PASS. Then run `cargo clippy --all-targets -- -D warnings` and confirm `run_hook_phase` stays under cognitive-complexity 15 — if clippy flags it, extract the skill block into a helper `fn install_one_skill(h, env, scope, opts, names, any_ok, any_fail)` or return a small struct; keep flow readable.

- [ ] **Step 8: Commit**

```bash
git add crates/ironlint-cli/src/commands/init/onboard.rs crates/ironlint-cli/tests/cli_init.rs
git commit -m "$(cat <<'EOF'
feat(init): install the ironlint-config skill per harness

`ironlint init` now materializes the authoring skill into each wired harness's
skills dir, deduping opencode against claude-code's copy. Uninstall strips it.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KCX6mWmrFwyUeEA2FxhPVU
EOF
)"
```

---

### Task 6: Docs

**Files:**
- Modify: `docs/reference/cli.md` (add `ironlint schema`)
- Modify: `docs/adapters/README.md` (retire `ironlint-author` → `ironlint-config`; note init installs it for every agent)
- Modify: `docs/adapters/claude-code.md` (drop the `ironlint-author` reference; point authoring at the init-installed skill + `ironlint schema`)
- Modify: `README.md` ("Connect your agent" — mention init installs the authoring skill)

**Interfaces:**
- Consumes: the shipped behavior from Tasks 1–5 (command name `ironlint schema`; skill name `ironlint-config`; install per harness).

- [ ] **Step 1: Add `ironlint schema` to the CLI reference**

In `docs/reference/cli.md`, add a section (after `ironlint show-resolved-config`, before "Read-only commands"):

```markdown
## `ironlint schema`

Print the canonical gate-authoring guide — the `.ironlint.yml` `{files, run}`
schema, the exit-code contract, and the common gate patterns. Read-only; loads
no config. This is the same guide `ironlint init` installs into each agent as the
`ironlint-config` skill.

```
ironlint schema
```

**Exit codes:** always `0`.
```

Also add `schema` to the "Read-only commands" sentence if it enumerates commands.

- [ ] **Step 2: Update the adapters overview**

In `docs/adapters/README.md`, in "Managing policy from inside the agent", replace the `ironlint-author` bullet and the trailing "Claude Code ships all three…" paragraph with copy that reflects the new model: `ironlint init` installs a **`ironlint-config`** authoring skill into *every* detected agent (claude-code, pi, opencode, reasonix — all support Agent Skills), and `ironlint schema` prints the same guide on demand. `ironlint-init` and `ironlint-review` remain Claude Code plugin skills.

Exact replacement for the three-bullet list + closing paragraph:

```markdown
- **`ironlint-config`** is the authoring guide: the `{files, run}` gate schema, the
  exit-code contract, and the common patterns, with a fixture-test loop. `ironlint
  init` installs it as a real skill into every detected agent, and `ironlint schema`
  prints it on demand.
- **`/ironlint-init`** scaffolds a `.ironlint.yml` from your project's stack.
- **`/ironlint-review`** reads your telemetry log and reports which gates are noisy,
  which never fire, and which look dead.

`ironlint init` installs `ironlint-config` for every agent it wires (all four support
the Agent Skills spec). `/ironlint-init` and `/ironlint-review` ship with the Claude
Code plugin today; other agents gain them as their needs settle.
```

- [ ] **Step 3: Update the Claude Code adapter page**

In `docs/adapters/claude-code.md`, in "Author and review gates from inside Claude", replace the `ironlint-author` mention so it reads: authoring is the `ironlint-config` skill that `ironlint init --harness claude-code` installs into `.claude/skills/ironlint-config/` (and `ironlint schema` prints the same guide); `/ironlint-init` and `/ironlint-review` come with the plugin.

- [ ] **Step 4: Update the root README**

In `README.md`, in the "Connect your agent" section, add one sentence after the `ironlint init` description: that init also installs a `ironlint-config` authoring skill so the agent knows how to write and fix gates (or run `ironlint schema` to read the format yourself).

- [ ] **Step 5: Verify the docs build/read cleanly**

Run: `grep -rn "ironlint-author" docs/ README.md`
Expected: no matches (every reference migrated to `ironlint-config`). Then re-read each edited section for accuracy against the shipped behavior.

- [ ] **Step 6: Commit**

```bash
git add docs/reference/cli.md docs/adapters/README.md docs/adapters/claude-code.md README.md
git commit -m "$(cat <<'EOF'
docs: ironlint-config authoring skill + `ironlint schema`

`ironlint init` installs the ironlint-config authoring skill into every agent;
`ironlint schema` prints the guide. Retire the ironlint-author references.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KCX6mWmrFwyUeEA2FxhPVU
EOF
)"
```

---

### Task 7: Docker e2e — assert the skill lands

**Files:**
- Modify: `tests/e2e/init/run.sh` (host-side assertions)

**Interfaces:**
- Consumes: the shipped install behavior. The existing `drive.sh` already seeds `~/.reasonix`, `~/.pi`, `~/.config/opencode` and runs `ironlint init --yes` in `/work`; skills install project-local under `/work` (no `--global`), and claude-code is excluded (no `~/.claude` seeded), so opencode's skill is **not** deduped and lands in `.opencode/skills/`.

- [ ] **Step 1: Add skill assertions**

In `tests/e2e/init/run.sh`, after the existing per-harness artifact assertions (around the pi/opencode plugin checks), add:

```bash
# Authoring skill: ironlint init installs ironlint-config/SKILL.md into each wired
# agent's skills dir (project-local; claude-code excluded so opencode is not
# deduped here).
exists "$PROJ_DIR/.reasonix/skills/ironlint-config/SKILL.md" "reasonix authoring skill"
exists "$PROJ_DIR/.pi/skills/ironlint-config/SKILL.md" "pi authoring skill"
exists "$PROJ_DIR/.opencode/skills/ironlint-config/SKILL.md" "opencode authoring skill"
contains "$PROJ_DIR/.pi/skills/ironlint-config/SKILL.md" "name: ironlint-config" "pi skill has frontmatter"
exists "$PROJ_DIR/.pi/skills/ironlint-config/.ironlint-adapter.json" "pi skill sidecar"
```

- [ ] **Step 2: Run the Docker feature test**

Run: `bash tests/e2e/init/run.sh`
Expected: PASS — all prior assertions plus the new skill assertions hold. (Requires Docker; the first build compiles a Linux `ironlint`.)

- [ ] **Step 3: Commit**

```bash
git add tests/e2e/init/run.sh
git commit -m "$(cat <<'EOF'
test(e2e): assert ironlint init installs the ironlint-config skill

Extends the Docker onboarding test to verify the authoring SKILL.md + sidecar
land for reasonix, pi, and opencode.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KCX6mWmrFwyUeEA2FxhPVU
EOF
)"
```

---

## Final verification (after all tasks)

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test --workspace` (all green)
- [ ] `bash scripts/ci-coverage.sh` is CI-only locally (no llvm-tools in this env) — ensure new `src/` functions in `schema.rs`, `ops.rs` skill fns, and `onboard.rs` helpers have tests exercising every branch (≥80% region).
- [ ] `grep -rn "ironlint-author" docs/ README.md adapters/` returns nothing.
```
