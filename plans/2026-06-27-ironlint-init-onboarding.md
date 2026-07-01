# `ironlint init` Harness Onboarding — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fold rtk-style, one-command harness onboarding into `ironlint init` — detect installed coding agents, confirm, and atomically wire IronLint's hook into each.

**Architecture:** A new `ironlint-core::adapter` module owns a 4-entry harness registry, materializes the existing `adapters/<harness>/` artifacts (embedded via `include_str!`) to stable paths, and patches each harness's settings idempotently. `ironlint init` gains a hook phase (detect → confirm → install/summary) after its existing config-scaffold phase; `ironlint doctor` gains an adapters section.

**Tech Stack:** Rust (Cargo workspace, two crates), clap, serde_json, serde_yaml, sha2, tempfile, `std::io::IsTerminal`. CLI tests via `assert_cmd`.

**Design spec:** `specs/2026-06-27-ironlint-init-onboarding-design.md`.

## Global Constraints

- Scope is the **four hook-capable harnesses only**: `claude-code`, `reasonix`, `pi`, `opencode`. No rules-only harnesses, no new harnesses.
- Reuse the existing adapter files under `adapters/` as the single source of truth — embed via `include_str!`, never copy bytes into Rust source.
- Every file under `crates/*/src/` must clear **≥80% region coverage** (`bash scripts/ci-coverage.sh`). Code added without bringing its file to the gate breaks CI.
- Per-function **cognitive complexity ≤ 15** (clippy, enforced). Decompose; don't `#[allow]`.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt` must pass.
- `IronLintEngine::load` and the trust store stay untouched. Hook integrity is a separate, lighter concern (a content-hash sidecar), not a trust decision.
- All settings/artifact writes are **atomic** (temp sibling + rename) and back up a pre-existing settings file to `<settings>.bak` once (preserve the first pristine copy).
- The materialized hook command references the **hook script path**, not the `ironlint` binary — the hook finds `ironlint` on `PATH` itself (matches today's adapters).
- `Cargo.lock` is gitignored — never commit it.
- Commit after each task. Branch is `feat/init-onboarding` (already created).

## Key Type & Function Inventory (defined across tasks; listed here for cross-reference)

All in `crates/ironlint-core/src/adapter/`:

```rust
// mod.rs
pub const CURRENT_ADAPTER_VERSION: u32 = 1;
pub enum Scope { Local, Global }
pub struct AdapterEnv { pub home: PathBuf, pub config_home: PathBuf, pub project_root: PathBuf }
pub enum HarnessKind { JsonHook(JsonHookSpec), Plugin(PluginSpec) }
pub struct Harness { pub name: &'static str, pub kind: HarnessKind, pub restart_hint: &'static str }
pub enum InstallResult { Installed, AlreadyPresent, Updated, Skipped(String), Failed(String), DryRun(Vec<String>) }
pub struct InstallOutcome { pub harness: &'static str, pub result: InstallResult, pub hint: &'static str }
pub struct HarnessStatus { pub harness: &'static str, pub detected: bool, pub installed: bool,
                           pub registered: bool, pub intact: Option<bool>, pub current: Option<bool> }
pub fn all_harnesses() -> Vec<Harness>;
pub fn adapters_dir(env: &AdapterEnv) -> PathBuf;            // <config_home>/ironlint/adapters
pub fn detect(env: &AdapterEnv) -> Vec<(&'static str, bool)>;
pub fn install(h: &Harness, env: &AdapterEnv, scope: Scope, dry_run: bool) -> Result<InstallOutcome>;
pub fn uninstall(h: &Harness, env: &AdapterEnv, scope: Scope, dry_run: bool) -> Result<InstallOutcome>;
pub fn status(h: &Harness, env: &AdapterEnv, scope: Scope) -> Result<HarnessStatus>;

// materialize.rs
pub fn sha256_hex(bytes: &[u8]) -> String;                  // "sha256:<hex>"
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()>;
pub fn backup_once(path: &Path) -> Result<()>;              // copy to <path>.bak if absent
pub struct AdapterSidecar { pub version: u32, pub files: BTreeMap<String, String> }
pub fn sidecar_path(dir: &Path) -> PathBuf;                 // dir/.ironlint-adapter.json
pub fn write_sidecar(dir: &Path, sidecar: &AdapterSidecar) -> Result<()>;
pub fn read_sidecar(dir: &Path) -> Result<Option<AdapterSidecar>>;

// json_settings.rs
pub enum PatchResult { Added, AlreadyPresent }
pub fn sync_hook_array(settings: &mut Value, key: &str, desired: Value, marker: &str) -> PatchResult;
pub fn remove_from_hook_array(settings: &mut Value, key: &str, marker: &str) -> bool;

// registry.rs
pub struct JsonHookSpec {
    pub settings_local: fn(&AdapterEnv) -> Option<PathBuf>,
    pub settings_global: fn(&AdapterEnv) -> PathBuf,
    pub array_key: &'static str,                 // "PostToolUse" | "PreToolUse"
    pub entry_arg: &'static str,                 // "post-tool-use" | "pre-tool-use"
    pub primary: &'static str,                   // "hook.sh"
    pub files: &'static [(&'static str, &'static str)],   // (filename, embedded bytes)
    pub build_entry: fn(command: &str) -> Value,
}
pub struct PluginSpec {
    pub dir_local: fn(&AdapterEnv) -> Option<PathBuf>,
    pub dir_global: fn(&AdapterEnv) -> Option<PathBuf>,
    pub filename: &'static str,                  // "ironlint.ts"
    pub source: &'static str,                    // include_str! of index.ts
    pub detect: fn(&AdapterEnv) -> bool,
    pub json_detect_unused: (),                  // (placeholder removed — see Task 3)
}
```

CLI (in `crates/ironlint-cli/src/commands/init/`):

```rust
pub struct Options { pub harnesses: Vec<String>, pub global: bool, pub yes: bool,
                     pub no_hook: bool, pub hook_only: bool, pub uninstall: bool, pub dry_run: bool }
pub fn run(dir: &Path, opts: Options) -> Result<i32>;       // mod.rs (extended)
pub fn run_hook_phase(env: &AdapterEnv, opts: &Options) -> Result<i32>;   // onboard.rs
```

---

### Task 1: Materialize primitives (atomic write, hashing, sidecar)

**Files:**
- Create: `crates/ironlint-core/src/adapter/mod.rs` (module decl only this task)
- Create: `crates/ironlint-core/src/adapter/materialize.rs`
- Modify: `crates/ironlint-core/src/lib.rs` (add `pub mod adapter;` after line 5's `pub mod config;` block)

**Interfaces:**
- Produces: `sha256_hex`, `atomic_write`, `backup_once`, `AdapterSidecar`, `sidecar_path`, `write_sidecar`, `read_sidecar` (signatures in inventory above).

- [ ] **Step 1: Register the module**

In `crates/ironlint-core/src/lib.rs`, add after the existing `pub mod` block (keep alphabetical-ish order, place after `pub mod config;`):

```rust
pub mod adapter;
```

Create `crates/ironlint-core/src/adapter/mod.rs` with just:

```rust
//! Harness onboarding: materialize IronLint's hook into supported coding agents.
mod materialize;
pub use materialize::{
    atomic_write, backup_once, read_sidecar, sha256_hex, sidecar_path, write_sidecar,
    AdapterSidecar,
};
```

- [ ] **Step 2: Write the failing tests**

Create `crates/ironlint-core/src/adapter/materialize.rs` ending with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn sha256_hex_is_prefixed_and_stable() {
        assert_eq!(
            sha256_hex(b"hello"),
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn atomic_write_creates_parents_and_content() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a/b/c.sh");
        atomic_write(&p, b"#!/bin/sh\n").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"#!/bin/sh\n");
    }

    #[test]
    fn backup_once_preserves_first_original_only() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, b"original").unwrap();
        backup_once(&p).unwrap();
        std::fs::write(&p, b"changed").unwrap();
        backup_once(&p).unwrap(); // must NOT overwrite the pristine backup
        assert_eq!(std::fs::read(p.with_extension("json.bak")).unwrap(), b"original");
    }

    #[test]
    fn backup_once_noop_when_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("missing.json");
        backup_once(&p).unwrap();
        assert!(!p.with_extension("json.bak").exists());
    }

    #[test]
    fn sidecar_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut files = BTreeMap::new();
        files.insert("hook.sh".to_string(), "sha256:abc".to_string());
        let sc = AdapterSidecar { version: 1, files };
        write_sidecar(tmp.path(), &sc).unwrap();
        let back = read_sidecar(tmp.path()).unwrap().unwrap();
        assert_eq!(back.version, 1);
        assert_eq!(back.files.get("hook.sh").unwrap(), "sha256:abc");
    }

    #[test]
    fn read_sidecar_absent_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_sidecar(tmp.path()).unwrap().is_none());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ironlint-core adapter::materialize`
Expected: FAIL — `materialize` items not defined / module body empty.

- [ ] **Step 4: Implement**

Prepend to `crates/ironlint-core/src/adapter/materialize.rs` (above the test module):

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `"sha256:<lowercase-hex>"` of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{:x}", h.finalize())
}

/// Write `bytes` to `path` atomically (temp sibling + rename), creating parents.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("ironlint")
    ));
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

/// Copy `path` to `<path>.bak` only if the file exists and no backup exists yet.
pub fn backup_once(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let bak = path.with_extension(format!(
        "{}.bak",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    if bak.exists() {
        return Ok(());
    }
    std::fs::copy(path, &bak).with_context(|| format!("backing up {}", path.display()))?;
    Ok(())
}

/// Per-harness integrity record, written beside the materialized artifacts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterSidecar {
    pub version: u32,
    /// filename -> "sha256:<hex>"
    pub files: BTreeMap<String, String>,
}

/// `<dir>/.ironlint-adapter.json`.
pub fn sidecar_path(dir: &Path) -> PathBuf {
    dir.join(".ironlint-adapter.json")
}

pub fn write_sidecar(dir: &Path, sidecar: &AdapterSidecar) -> Result<()> {
    let json = serde_json::to_string_pretty(sidecar)?;
    atomic_write(&sidecar_path(dir), json.as_bytes())
}

pub fn read_sidecar(dir: &Path) -> Result<Option<AdapterSidecar>> {
    match std::fs::read_to_string(sidecar_path(dir)) {
        Ok(s) => Ok(Some(
            serde_json::from_str(&s).with_context(|| "parsing adapter sidecar")?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("reading adapter sidecar"),
    }
}
```

Note: `path.with_extension` on `settings.json` yields `settings.json.bak` here because we pass the *full* extension string; verify the test's `with_extension("json.bak")` expectation matches (it does: `settings` + `.json.bak`). For `hook.sh` backups the same pattern yields `hook.sh.bak`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ironlint-core adapter::materialize`
Expected: PASS (6 tests).

- [ ] **Step 6: Lint + commit**

Run: `cargo fmt && cargo clippy -p ironlint-core --all-targets -- -D warnings`
```bash
git add crates/ironlint-core/src/lib.rs crates/ironlint-core/src/adapter/
git commit -m "feat(adapter): materialize primitives — atomic write, sha256, sidecar"
```

---

### Task 2: JSON settings patch/unpatch primitives

**Files:**
- Create: `crates/ironlint-core/src/adapter/json_settings.rs`
- Modify: `crates/ironlint-core/src/adapter/mod.rs` (add `mod json_settings;` + re-export)

**Interfaces:**
- Consumes: nothing from prior tasks.
- Produces: `PatchResult`, `sync_hook_array`, `remove_from_hook_array`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ironlint-core/src/adapter/json_settings.rs` ending with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claude_entry(cmd: &str) -> serde_json::Value {
        json!({"matcher": "Edit|Write",
               "hooks": [{"type": "command", "command": cmd}]})
    }

    #[test]
    fn sync_inserts_into_empty_settings() {
        let mut s = json!({});
        let cmd = "\"/h/adapters/claude-code/hook.sh\" post-tool-use";
        let r = sync_hook_array(&mut s, "PostToolUse", claude_entry(cmd), "/h/adapters/claude-code/");
        assert!(matches!(r, PatchResult::Added));
        assert_eq!(s["hooks"]["PostToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn sync_is_idempotent_for_identical_entry() {
        let cmd = "\"/h/adapters/reasonix/hook.sh\" pre-tool-use";
        let entry = json!({"command": cmd, "match": "^(write_file|edit_file|multi_edit)$",
                           "description": "ironlint", "timeout": 30000});
        let mut s = json!({});
        sync_hook_array(&mut s, "PreToolUse", entry.clone(), "/h/adapters/reasonix/");
        let r = sync_hook_array(&mut s, "PreToolUse", entry, "/h/adapters/reasonix/");
        assert!(matches!(r, PatchResult::AlreadyPresent));
        assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn sync_strips_stale_ironlint_entry_and_keeps_foreign() {
        let mut s = json!({"hooks": {"PreToolUse": [
            {"command": "\"/h/adapters/reasonix/hook.sh\" pre-tool-use", "match": "old"},
            {"command": "other-tool guard", "match": "x"}
        ]}});
        let new_cmd = "\"/h/adapters/reasonix/hook.sh\" pre-tool-use";
        let entry = json!({"command": new_cmd, "match": "^(write_file|edit_file|multi_edit)$"});
        let r = sync_hook_array(&mut s, "PreToolUse", entry, "/h/adapters/reasonix/");
        assert!(matches!(r, PatchResult::Added));
        let arr = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2); // foreign kept, single ironlint entry refreshed
        assert!(arr.iter().any(|e| e["command"] == "other-tool guard"));
        assert!(arr.iter().any(|e| e["match"] == "^(write_file|edit_file|multi_edit)$"));
    }

    #[test]
    fn remove_drops_only_ironlint_entries() {
        let mut s = json!({"hooks": {"PostToolUse": [
            claude_entry("\"/h/adapters/claude-code/hook.sh\" post-tool-use"),
            {"matcher": "Edit", "hooks": [{"type": "command", "command": "keep me"}]}
        ]}});
        let removed = remove_from_hook_array(&mut s, "PostToolUse", "/h/adapters/claude-code/");
        assert!(removed);
        let arr = s["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["hooks"][0]["command"], "keep me");
    }

    #[test]
    fn remove_returns_false_when_absent() {
        let mut s = json!({"hooks": {"PostToolUse": []}});
        assert!(!remove_from_hook_array(&mut s, "PostToolUse", "/h/adapters/claude-code/"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ironlint-core adapter::json_settings`
Expected: FAIL — items not defined.

- [ ] **Step 3: Implement**

Prepend to `crates/ironlint-core/src/adapter/json_settings.rs`:

```rust
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchResult {
    Added,
    AlreadyPresent,
}

/// True if any string anywhere in `v` contains `marker`.
fn contains_marker(v: &Value, marker: &str) -> bool {
    match v {
        Value::String(s) => s.contains(marker),
        Value::Array(a) => a.iter().any(|e| contains_marker(e, marker)),
        Value::Object(o) => o.values().any(|e| contains_marker(e, marker)),
        _ => false,
    }
}

/// Mutable reference to `settings["hooks"][key]` as an array, creating the
/// `hooks` object and the array if missing.
fn hook_array<'a>(settings: &'a mut Value, key: &str) -> &'a mut Vec<Value> {
    if !settings.is_object() {
        *settings = Value::Object(serde_json::Map::new());
    }
    let obj = settings.as_object_mut().expect("just ensured object");
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !hooks.is_object() {
        *hooks = Value::Object(serde_json::Map::new());
    }
    let arr = hooks
        .as_object_mut()
        .expect("just ensured object")
        .entry(key)
        .or_insert_with(|| Value::Array(Vec::new()));
    if !arr.is_array() {
        *arr = Value::Array(Vec::new());
    }
    arr.as_array_mut().expect("just ensured array")
}

/// Insert `desired` into `settings.hooks[key]`, replacing any existing
/// ironlint-owned entry (identified by `marker`). Idempotent: if the only
/// ironlint entry already equals `desired`, returns `AlreadyPresent`.
pub fn sync_hook_array(settings: &mut Value, key: &str, desired: Value, marker: &str) -> PatchResult {
    let arr = hook_array(settings, key);
    let ironlint: Vec<&Value> = arr.iter().filter(|e| contains_marker(e, marker)).collect();
    if ironlint.len() == 1 && ironlint[0] == &desired {
        return PatchResult::AlreadyPresent;
    }
    arr.retain(|e| !contains_marker(e, marker));
    arr.push(desired);
    PatchResult::Added
}

/// Remove every ironlint-owned entry from `settings.hooks[key]`. Returns whether
/// anything was removed.
pub fn remove_from_hook_array(settings: &mut Value, key: &str, marker: &str) -> bool {
    let arr = hook_array(settings, key);
    let before = arr.len();
    arr.retain(|e| !contains_marker(e, marker));
    arr.len() != before
}
```

Update `crates/ironlint-core/src/adapter/mod.rs`:

```rust
mod json_settings;
pub use json_settings::{remove_from_hook_array, sync_hook_array, PatchResult};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ironlint-core adapter::json_settings`
Expected: PASS (5 tests).

- [ ] **Step 5: Lint + commit**

Run: `cargo fmt && cargo clippy -p ironlint-core --all-targets -- -D warnings`
```bash
git add crates/ironlint-core/src/adapter/
git commit -m "feat(adapter): idempotent serde_json hook-array patch/unpatch"
```

---

### Task 3: Harness registry, embedded artifacts, detection

**Files:**
- Create: `crates/ironlint-core/src/adapter/registry.rs`
- Modify: `crates/ironlint-core/src/adapter/mod.rs` (define `AdapterEnv`, `Scope`, `Harness`, `HarnessKind`, `CURRENT_ADAPTER_VERSION`, `all_harnesses`, `adapters_dir`, `detect`; re-export specs)
- Modify: `crates/ironlint-core/src/trust.rs:133` (make `config_home` reusable)

**Interfaces:**
- Consumes: nothing yet (registry is data).
- Produces: `JsonHookSpec`, `PluginSpec`, `Harness`, `HarnessKind`, `Scope`, `AdapterEnv`, `CURRENT_ADAPTER_VERSION`, `all_harnesses()`, `adapters_dir()`, `detect()`.

- [ ] **Step 1: Expose the config-home resolver**

In `crates/ironlint-core/src/trust.rs`, change line 133 from `fn config_home()` to `pub fn config_home()` so the adapter module can resolve `<config_home>` identically to the trust store. Leave `config_home_from` private (already tested via `store_path_joins_under_config_home`).

- [ ] **Step 2: Write the failing tests**

Append to `crates/ironlint-core/src/adapter/registry.rs` (file starts empty; tests first):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{AdapterEnv, HarnessKind};
    use std::path::PathBuf;

    fn env_with(home: &str, project: &str) -> AdapterEnv {
        AdapterEnv {
            home: PathBuf::from(home),
            config_home: PathBuf::from(format!("{home}/.config")),
            project_root: PathBuf::from(project),
        }
    }

    #[test]
    fn four_harnesses_registered() {
        let names: Vec<_> = all_harnesses().iter().map(|h| h.name).collect();
        assert_eq!(names, vec!["claude-code", "reasonix", "pi", "opencode"]);
    }

    #[test]
    fn embedded_artifacts_are_nonempty() {
        for h in all_harnesses() {
            match &h.kind {
                HarnessKind::JsonHook(s) => {
                    assert!(!s.files.is_empty(), "{} has no files", h.name);
                    for (name, bytes) in s.files {
                        assert!(!bytes.is_empty(), "{}/{} empty", h.name, name);
                    }
                }
                HarnessKind::Plugin(p) => assert!(!p.source.is_empty(), "{} plugin empty", h.name),
            }
        }
    }

    #[test]
    fn claude_entry_points_at_command_and_matcher() {
        let e = claude_build_entry("\"/x/hook.sh\" post-tool-use");
        assert_eq!(e["matcher"], "Edit|Write");
        assert_eq!(e["hooks"][0]["command"], "\"/x/hook.sh\" post-tool-use");
    }

    #[test]
    fn reasonix_entry_matches_write_tools() {
        let e = reasonix_build_entry("\"/x/hook.sh\" pre-tool-use");
        assert_eq!(e["match"], "^(write_file|edit_file|multi_edit)$");
        assert_eq!(e["command"], "\"/x/hook.sh\" pre-tool-use");
    }

    #[test]
    fn detect_reports_presence_per_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        std::fs::create_dir_all(format!("{home}/.claude")).unwrap();
        std::fs::create_dir_all(format!("{home}/.pi")).unwrap();
        let env = env_with(home, home);
        let found: std::collections::BTreeMap<_, _> = crate::adapter::detect(&env).into_iter().collect();
        assert_eq!(found["claude-code"], true);
        assert_eq!(found["pi"], true);
        assert_eq!(found["reasonix"], false);
        assert_eq!(found["opencode"], false);
    }

    #[test]
    fn embedded_set_covers_on_disk_adapter_files() {
        // Drift guard: every shell/ts file shipped under adapters/<h> for a
        // hook-capable harness must be embedded, else `ironlint init` ships a
        // partial hook. Checks the two JsonHook harnesses' hooks/ dirs.
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters");
        for (harness, subdir) in [("claude-code", "hooks"), ("reasonix", "hooks")] {
            let dir = root.join(harness).join(subdir);
            let spec = match &all_harnesses().into_iter().find(|h| h.name == harness).unwrap().kind {
                HarnessKind::JsonHook(s) => *s,
                _ => unreachable!(),
            };
            for entry in std::fs::read_dir(&dir).unwrap() {
                let name = entry.unwrap().file_name().into_string().unwrap();
                if name.ends_with(".sh") {
                    assert!(spec.files.iter().any(|(f, _)| *f == name),
                        "adapters/{harness}/{subdir}/{name} is not embedded in the registry");
                }
            }
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ironlint-core adapter::registry`
Expected: FAIL — `all_harnesses`, specs, `claude_build_entry`, etc. not defined.

- [ ] **Step 4: Implement the core types in `mod.rs`**

Replace `crates/ironlint-core/src/adapter/mod.rs` with:

```rust
//! Harness onboarding: materialize IronLint's hook into supported coding agents.
mod json_settings;
mod materialize;
mod registry;

pub use json_settings::{remove_from_hook_array, sync_hook_array, PatchResult};
pub use materialize::{
    atomic_write, backup_once, read_sidecar, sha256_hex, sidecar_path, write_sidecar,
    AdapterSidecar,
};
pub use registry::{all_harnesses, JsonHookSpec, PluginSpec};

use std::path::PathBuf;

/// Bump when any embedded adapter artifact changes shape; drives doctor's
/// "outdated, re-run ironlint init" check.
pub const CURRENT_ADAPTER_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Local,
    Global,
}

/// Injectable environment so install/detect are testable without touching the
/// real `$HOME`.
#[derive(Debug, Clone)]
pub struct AdapterEnv {
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub project_root: PathBuf,
}

impl AdapterEnv {
    /// Resolve from the process environment + a project root (cwd or `--dir`).
    pub fn from_process(project_root: PathBuf) -> anyhow::Result<Self> {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| anyhow::anyhow!("cannot resolve $HOME"))?;
        let config_home = crate::trust::config_home()
            .ok_or_else(|| anyhow::anyhow!("cannot resolve config home (set $XDG_CONFIG_HOME or $HOME)"))?;
        Ok(Self { home, config_home, project_root })
    }
}

pub enum HarnessKind {
    JsonHook(JsonHookSpec),
    Plugin(PluginSpec),
}

pub struct Harness {
    pub name: &'static str,
    pub kind: HarnessKind,
    pub restart_hint: &'static str,
}

/// `<config_home>/ironlint/adapters` — sits beside the trust store.
pub fn adapters_dir(env: &AdapterEnv) -> PathBuf {
    env.config_home.join("ironlint").join("adapters")
}

/// `(harness-name, installed-on-this-machine?)` for every supported harness.
pub fn detect(env: &AdapterEnv) -> Vec<(&'static str, bool)> {
    all_harnesses()
        .into_iter()
        .map(|h| (h.name, registry::is_detected(&h, env)))
        .collect()
}
```

`JsonHookSpec` and `PluginSpec` derive `Clone, Copy` (all fields are `Copy`: fn pointers + `&'static`), so the test's `*s` deref works. Add `#[derive(Clone, Copy)]` to both in registry.rs (next step).

- [ ] **Step 5: Implement the registry data in `registry.rs`**

Prepend to `crates/ironlint-core/src/adapter/registry.rs`:

```rust
use crate::adapter::{AdapterEnv, Harness, HarnessKind};
use serde_json::{json, Value};
use std::path::PathBuf;

// --- embedded artifacts (single source of truth = adapters/) -----------------
const CLAUDE_HOOK: &str = include_str!("../../../../adapters/claude-code/hooks/hook.sh");
const CLAUDE_SYNTH: &str = include_str!("../../../../adapters/claude-code/hooks/synthesize_diff.sh");
const REASONIX_HOOK: &str = include_str!("../../../../adapters/reasonix/hooks/hook.sh");
const PI_PLUGIN: &str = include_str!("../../../../adapters/pi/src/index.ts");
const OPENCODE_PLUGIN: &str = include_str!("../../../../adapters/opencode/src/index.ts");

#[derive(Clone, Copy)]
pub struct JsonHookSpec {
    pub settings_local: fn(&AdapterEnv) -> Option<PathBuf>,
    pub settings_global: fn(&AdapterEnv) -> PathBuf,
    pub array_key: &'static str,
    pub entry_arg: &'static str,
    pub primary: &'static str,
    pub files: &'static [(&'static str, &'static str)],
    pub build_entry: fn(&str) -> Value,
}

#[derive(Clone, Copy)]
pub struct PluginSpec {
    pub dir_local: fn(&AdapterEnv) -> Option<PathBuf>,
    pub dir_global: fn(&AdapterEnv) -> Option<PathBuf>,
    pub filename: &'static str,
    pub source: &'static str,
    pub detect: fn(&AdapterEnv) -> bool,
}

// --- per-harness entry builders (also unit-tested directly) ------------------
pub(crate) fn claude_build_entry(command: &str) -> Value {
    json!({"matcher": "Edit|Write",
           "hooks": [{"type": "command", "command": command}]})
}

pub(crate) fn reasonix_build_entry(command: &str) -> Value {
    json!({"command": command,
           "match": "^(write_file|edit_file|multi_edit)$",
           "description": "Block edits that violate ironlint policy before they land on disk",
           "timeout": 30000})
}

// --- registry ----------------------------------------------------------------
const CLAUDE: JsonHookSpec = JsonHookSpec {
    settings_local: |e| Some(e.project_root.join(".claude").join("settings.json")),
    settings_global: |e| e.home.join(".claude").join("settings.json"),
    array_key: "PostToolUse",
    entry_arg: "post-tool-use",
    primary: "hook.sh",
    files: &[("hook.sh", CLAUDE_HOOK), ("synthesize_diff.sh", CLAUDE_SYNTH)],
    build_entry: claude_build_entry,
};

const REASONIX: JsonHookSpec = JsonHookSpec {
    settings_local: |_| None, // reasonix settings are user-global only
    settings_global: |e| e.home.join(".reasonix").join("settings.json"),
    array_key: "PreToolUse",
    entry_arg: "pre-tool-use",
    primary: "hook.sh",
    files: &[("hook.sh", REASONIX_HOOK)],
    build_entry: reasonix_build_entry,
};

const PI: PluginSpec = PluginSpec {
    dir_local: |e| Some(e.project_root.join(".pi").join("extensions")),
    dir_global: |e| Some(e.home.join(".pi").join("agent").join("extensions")),
    filename: "ironlint.ts",
    source: PI_PLUGIN,
    detect: |e| e.home.join(".pi").is_dir(),
};

const OPENCODE: PluginSpec = PluginSpec {
    dir_local: |e| Some(e.project_root.join(".opencode").join("plugins")),
    dir_global: |_| None, // opencode plugins are project-scoped (per adapter README)
    filename: "ironlint.ts",
    source: OPENCODE_PLUGIN,
    detect: |e| {
        e.config_home.join("opencode").is_dir() || e.project_root.join(".opencode").is_dir()
    },
};

pub fn all_harnesses() -> Vec<Harness> {
    vec![
        Harness {
            name: "claude-code",
            kind: HarnessKind::JsonHook(CLAUDE),
            restart_hint: "Reload Claude Code (or restart) — it picks up settings.json hooks.",
        },
        Harness {
            name: "reasonix",
            kind: HarnessKind::JsonHook(REASONIX),
            restart_hint: "Restart Reasonix so it reloads settings.",
        },
        Harness {
            name: "pi",
            kind: HarnessKind::Plugin(PI),
            restart_hint: "Restart pi so it loads the new extension.",
        },
        Harness {
            name: "opencode",
            kind: HarnessKind::Plugin(OPENCODE),
            restart_hint: "Restart opencode so it loads the new plugin.",
        },
    ]
}

/// Whether `harness` looks installed on this machine.
pub(crate) fn is_detected(harness: &Harness, env: &AdapterEnv) -> bool {
    match &harness.kind {
        HarnessKind::JsonHook(s) => match s.array_key {
            "PostToolUse" => env.home.join(".claude").is_dir(), // claude-code
            _ => env.home.join(".reasonix").is_dir(),           // reasonix
        },
        HarnessKind::Plugin(p) => (p.detect)(env),
    }
}
```

Note on the `is_detected` JsonHook arm: it keys off `array_key` because the two JsonHook harnesses have distinct homes; add a dedicated `detect: fn` field to `JsonHookSpec` instead if a third JsonHook harness ever shares a key. (YAGNI for now — two harnesses, two keys.)

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p ironlint-core adapter::registry`
Expected: PASS (6 tests). If `embedded_set_covers_on_disk_adapter_files` fails, the `CARGO_MANIFEST_DIR/../../adapters` path is wrong — confirm with `ls crates/ironlint-core/../../adapters`.

- [ ] **Step 7: Lint + commit**

Run: `cargo fmt && cargo clippy -p ironlint-core --all-targets -- -D warnings`
```bash
git add crates/ironlint-core/src/adapter/ crates/ironlint-core/src/trust.rs
git commit -m "feat(adapter): harness registry, embedded artifacts, detection"
```

---

### Task 4: install / uninstall / status orchestration

**Files:**
- Create: `crates/ironlint-core/src/adapter/ops.rs`
- Modify: `crates/ironlint-core/src/adapter/mod.rs` (add `mod ops;` + re-export `install`, `uninstall`, `status`, `InstallOutcome`, `InstallResult`, `HarnessStatus`)

**Interfaces:**
- Consumes: `materialize::*`, `json_settings::*`, `registry::{all_harnesses, JsonHookSpec, PluginSpec}`, `AdapterEnv`, `Scope`, `Harness`, `HarnessKind`, `adapters_dir`, `CURRENT_ADAPTER_VERSION`.
- Produces: `install`, `uninstall`, `status`, `InstallResult`, `InstallOutcome`, `HarnessStatus`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ironlint-core/src/adapter/ops.rs` ending with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{all_harnesses, AdapterEnv, Scope};
    use std::path::PathBuf;

    fn harness(name: &str) -> crate::adapter::Harness {
        all_harnesses().into_iter().find(|h| h.name == name).unwrap()
    }
    fn env(tmp: &std::path::Path) -> AdapterEnv {
        AdapterEnv {
            home: tmp.to_path_buf(),
            config_home: tmp.join(".config"),
            project_root: tmp.join("proj"),
        }
    }

    #[test]
    fn install_reasonix_writes_artifact_sidecar_and_patches_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let out = install(&harness("reasonix"), &e, Scope::Global, false).unwrap();
        assert!(matches!(out.result, InstallResult::Installed));
        let hook = e.config_home.join("ironlint/adapters/reasonix/hook.sh");
        assert!(hook.exists());
        assert!(crate::adapter::read_sidecar(hook.parent().unwrap()).unwrap().is_some());
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(tmp.path().join(".reasonix/settings.json")).unwrap()).unwrap();
        let cmd = settings["hooks"]["PreToolUse"][0]["command"].as_str().unwrap();
        assert!(cmd.contains("adapters/reasonix/hook.sh"));
        assert!(cmd.ends_with("pre-tool-use"));
    }

    #[test]
    fn install_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install(&harness("reasonix"), &e, Scope::Global, false).unwrap();
        let again = install(&harness("reasonix"), &e, Scope::Global, false).unwrap();
        assert!(matches!(again.result, InstallResult::AlreadyPresent));
    }

    #[test]
    fn dry_run_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let out = install(&harness("reasonix"), &e, Scope::Global, true).unwrap();
        assert!(matches!(out.result, InstallResult::DryRun(_)));
        assert!(!tmp.path().join(".reasonix/settings.json").exists());
        assert!(!e.config_home.join("ironlint/adapters/reasonix/hook.sh").exists());
    }

    #[test]
    fn install_plugin_drops_file_in_project_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let out = install(&harness("opencode"), &e, Scope::Local, false).unwrap();
        assert!(matches!(out.result, InstallResult::Installed));
        assert!(e.project_root.join(".opencode/plugins/ironlint.ts").exists());
    }

    #[test]
    fn uninstall_removes_artifact_and_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        install(&harness("reasonix"), &e, Scope::Global, false).unwrap();
        let out = uninstall(&harness("reasonix"), &e, Scope::Global, false).unwrap();
        assert!(matches!(out.result, InstallResult::Installed)); // "removed" reuses Installed-style ok
        assert!(!e.config_home.join("ironlint/adapters/reasonix/hook.sh").exists());
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(tmp.path().join(".reasonix/settings.json")).unwrap()).unwrap();
        assert_eq!(settings["hooks"]["PreToolUse"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn status_reports_installed_and_intact_after_install() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        std::fs::create_dir_all(tmp.path().join(".reasonix")).unwrap();
        install(&harness("reasonix"), &e, Scope::Global, false).unwrap();
        let st = status(&harness("reasonix"), &e, Scope::Global).unwrap();
        assert!(st.detected && st.installed && st.registered);
        assert_eq!(st.intact, Some(true));
        assert_eq!(st.current, Some(true));
    }
}
```

The `uninstall` "removed ok" reuses `InstallResult::Installed` as the success marker; if you prefer a distinct `Removed` variant, add it to the enum and update the test — either is fine, keep it consistent.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ironlint-core adapter::ops`
Expected: FAIL — `install`/`uninstall`/`status` not defined.

- [ ] **Step 3: Implement `ops.rs`**

Prepend to `crates/ironlint-core/src/adapter/ops.rs`:

```rust
use crate::adapter::materialize::{
    atomic_write, backup_once, read_sidecar, sha256_hex, write_sidecar, AdapterSidecar,
};
use crate::adapter::registry::{JsonHookSpec, PluginSpec};
use crate::adapter::{
    adapters_dir, remove_from_hook_array, sync_hook_array, AdapterEnv, Harness, HarnessKind,
    PatchResult, Scope, CURRENT_ADAPTER_VERSION,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum InstallResult {
    Installed,
    AlreadyPresent,
    Updated,
    Skipped(String),
    Failed(String),
    DryRun(Vec<String>),
}

pub struct InstallOutcome {
    pub harness: &'static str,
    pub result: InstallResult,
    pub hint: &'static str,
}

pub struct HarnessStatus {
    pub harness: &'static str,
    pub detected: bool,
    pub installed: bool,
    pub registered: bool,
    pub intact: Option<bool>,
    pub current: Option<bool>,
}

/// Read a JSON settings file, defaulting to `{}` when absent.
fn load_settings(path: &Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).with_context(|| format!("parsing {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Default::default())),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn write_settings(path: &Path, value: &Value) -> Result<()> {
    backup_once(path)?;
    let json = serde_json::to_string_pretty(value)?;
    atomic_write(path, json.as_bytes())
}

fn settings_path(spec: &JsonHookSpec, env: &AdapterEnv, scope: Scope) -> PathBuf {
    match scope {
        Scope::Local => (spec.settings_local)(env).unwrap_or_else(|| (spec.settings_global)(env)),
        Scope::Global => (spec.settings_global)(env),
    }
}

fn plugin_dir(spec: &PluginSpec, env: &AdapterEnv, scope: Scope) -> PathBuf {
    let (primary, fallback) = match scope {
        Scope::Local => ((spec.dir_local)(env), (spec.dir_global)(env)),
        Scope::Global => ((spec.dir_global)(env), (spec.dir_local)(env)),
    };
    primary.or(fallback).expect("every plugin harness has at least one dir")
}

// --- install -----------------------------------------------------------------

pub fn install(h: &Harness, env: &AdapterEnv, scope: Scope, dry_run: bool) -> Result<InstallOutcome> {
    let result = match &h.kind {
        HarnessKind::JsonHook(spec) => install_jsonhook(h.name, spec, env, scope, dry_run)?,
        HarnessKind::Plugin(spec) => install_plugin(spec, env, scope, dry_run)?,
    };
    Ok(InstallOutcome { harness: h.name, result, hint: h.restart_hint })
}

fn install_jsonhook(
    name: &str,
    spec: &JsonHookSpec,
    env: &AdapterEnv,
    scope: Scope,
    dry_run: bool,
) -> Result<InstallResult> {
    let dir = adapters_dir(env).join(name);
    let primary_path = dir.join(spec.primary);
    let command = format!("\"{}\" {}", primary_path.display(), spec.entry_arg);
    let marker = format!("{}", dir.display());
    let settings = settings_path(spec, env, scope);

    if dry_run {
        let mut plan: Vec<String> = spec
            .files
            .iter()
            .map(|(f, _)| format!("write {}", dir.join(f).display()))
            .collect();
        plan.push(format!("patch {} [{}]", settings.display(), spec.array_key));
        return Ok(InstallResult::DryRun(plan));
    }

    let mut files = BTreeMap::new();
    for (fname, bytes) in spec.files {
        let p = dir.join(fname);
        atomic_write(&p, bytes.as_bytes())?;
        set_executable(&p)?;
        files.insert((*fname).to_string(), sha256_hex(bytes.as_bytes()));
    }
    write_sidecar(&dir, &AdapterSidecar { version: CURRENT_ADAPTER_VERSION, files })?;

    let mut value = load_settings(&settings)?;
    let entry = (spec.build_entry)(&command);
    let patch = sync_hook_array(&mut value, spec.array_key, entry, &marker);
    write_settings(&settings, &value)?;
    Ok(match patch {
        PatchResult::AlreadyPresent => InstallResult::AlreadyPresent,
        PatchResult::Added => InstallResult::Installed,
    })
}

fn install_plugin(spec: &PluginSpec, env: &AdapterEnv, scope: Scope, dry_run: bool) -> Result<InstallResult> {
    let dir = plugin_dir(spec, env, scope);
    let file = dir.join(spec.filename);
    if dry_run {
        return Ok(InstallResult::DryRun(vec![format!("write {}", file.display())]));
    }
    let existed = file.exists();
    atomic_write(&file, spec.source.as_bytes())?;
    let mut files = BTreeMap::new();
    files.insert(spec.filename.to_string(), sha256_hex(spec.source.as_bytes()));
    write_sidecar(&dir, &AdapterSidecar { version: CURRENT_ADAPTER_VERSION, files })?;
    Ok(if existed { InstallResult::Updated } else { InstallResult::Installed })
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}
#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

// --- uninstall ---------------------------------------------------------------

pub fn uninstall(h: &Harness, env: &AdapterEnv, scope: Scope, dry_run: bool) -> Result<InstallOutcome> {
    let result = match &h.kind {
        HarnessKind::JsonHook(spec) => {
            let dir = adapters_dir(env).join(h.name);
            let settings = settings_path(spec, env, scope);
            if dry_run {
                return Ok(InstallOutcome {
                    harness: h.name,
                    result: InstallResult::DryRun(vec![
                        format!("remove {}", dir.display()),
                        format!("unpatch {} [{}]", settings.display(), spec.array_key),
                    ]),
                    hint: h.restart_hint,
                });
            }
            if settings.exists() {
                let mut value = load_settings(&settings)?;
                if remove_from_hook_array(&mut value, spec.array_key, &format!("{}", dir.display())) {
                    write_settings(&settings, &value)?;
                }
            }
            remove_dir_if_present(&dir)?;
            InstallResult::Installed
        }
        HarnessKind::Plugin(spec) => {
            let dir = plugin_dir(spec, env, scope);
            let file = dir.join(spec.filename);
            if dry_run {
                return Ok(InstallOutcome {
                    harness: h.name,
                    result: InstallResult::DryRun(vec![format!("remove {}", file.display())]),
                    hint: h.restart_hint,
                });
            }
            let _ = std::fs::remove_file(&file);
            let _ = std::fs::remove_file(crate::adapter::sidecar_path(&dir));
            InstallResult::Installed
        }
    };
    Ok(InstallOutcome { harness: h.name, result, hint: h.restart_hint })
}

fn remove_dir_if_present(dir: &Path) -> Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing {}", dir.display())),
    }
}

// --- status ------------------------------------------------------------------

pub fn status(h: &Harness, env: &AdapterEnv, scope: Scope) -> Result<HarnessStatus> {
    let detected = crate::adapter::registry::is_detected(h, env);
    let (dir, registered, sidecar_dir, files): (PathBuf, bool, PathBuf, Vec<(String, String)>) =
        match &h.kind {
            HarnessKind::JsonHook(spec) => {
                let dir = adapters_dir(env).join(h.name);
                let settings = settings_path(spec, env, scope);
                let registered = settings_has_marker(&settings, spec.array_key, &format!("{}", dir.display()))?;
                let files = spec.files.iter().map(|(f, b)| ((*f).to_string(), sha256_hex(b.as_bytes()))).collect();
                (dir.clone(), registered, dir, files)
            }
            HarnessKind::Plugin(spec) => {
                let dir = plugin_dir(spec, env, scope);
                let file = dir.join(spec.filename);
                let files = vec![(spec.filename.to_string(), sha256_hex(spec.source.as_bytes()))];
                (file.clone(), file.exists(), dir, files)
            }
        };
    let installed = dir.exists();
    let (intact, current) = match read_sidecar(&sidecar_dir)? {
        Some(sc) => {
            let intact = files.iter().all(|(name, hash)| sc.files.get(name) == Some(hash));
            (Some(intact), Some(sc.version == CURRENT_ADAPTER_VERSION))
        }
        None => (None, None),
    };
    Ok(HarnessStatus { harness: h.name, detected, installed, registered, intact, current })
}

fn settings_has_marker(path: &Path, key: &str, marker: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let value = load_settings(path)?;
    Ok(value
        .get("hooks")
        .and_then(|h| h.get(key))
        .map(|arr| serde_json::to_string(arr).unwrap_or_default().contains(marker))
        .unwrap_or(false))
}
```

Add the re-exports to `mod.rs`:

```rust
mod ops;
pub use ops::{install, status, uninstall, HarnessStatus, InstallOutcome, InstallResult};
```

Also make `registry::is_detected` reachable from `ops`: it is `pub(crate)`, so `crate::adapter::registry::is_detected` resolves. Confirm `mod registry;` exposes the path (it's a private module but `pub(crate)` fn is reachable crate-wide via the module path — keep `mod registry;` and reference `crate::adapter::registry::is_detected`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ironlint-core adapter::ops`
Expected: PASS (6 tests).

- [ ] **Step 5: Full core suite + lint + commit**

Run: `cargo test -p ironlint-core && cargo clippy -p ironlint-core --all-targets -- -D warnings`
Expected: PASS, no warnings. If any function trips cognitive-complexity, split it (e.g. `uninstall` → `uninstall_jsonhook` / `uninstall_plugin`).
```bash
git add crates/ironlint-core/src/adapter/
git commit -m "feat(adapter): install/uninstall/status orchestration"
```

---

### Task 5: CLI flags + dispatch + non-fatal existing config

**Files:**
- Modify: `crates/ironlint-cli/src/cli.rs:71-74` (extend `Init` variant)
- Modify: `crates/ironlint-cli/src/main.rs:36` (pass new fields)
- Modify: `crates/ironlint-cli/src/commands/init/mod.rs:17-43` (`Options`, new `run` signature, non-fatal existing config, call hook phase)

**Interfaces:**
- Consumes: `ironlint_core::adapter::*`.
- Produces: `commands::init::Options`, extended `commands::init::run`.

- [ ] **Step 1: Write the failing test (existing-config is non-fatal)**

Add to the `tests` module in `crates/ironlint-cli/src/commands/init/mod.rs`:

```rust
#[test]
fn run_with_existing_config_and_no_hook_is_ok_not_error() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join(".ironlint.yml"), "gates: {}\n").unwrap();
    let opts = Options { harnesses: vec![], global: false, yes: false,
                         no_hook: true, hook_only: false, uninstall: false, dry_run: false };
    let code = run(tmp.path(), opts).unwrap();
    assert_eq!(code, 0); // previously this path returned Err
}
```

(Requires `tempfile` as a dev-dependency of `ironlint-cli` — it already is, per `crates/ironlint-cli/Cargo.toml:24`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ironlint-cli init::tests::run_with_existing_config`
Expected: FAIL — `run` takes `(&Path)` not `(&Path, Options)`, and currently errors on existing config.

- [ ] **Step 3: Extend the clap `Init` variant**

Replace `crates/ironlint-cli/src/cli.rs:71-74` (`Init { dir }`) with:

```rust
    /// Detect stack and scaffold a starter .ironlint.yml, then wire ironlint's hook
    /// into your coding agents.
    Init {
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// Harness(es) to wire up (repeatable); `all` selects every supported
        /// harness. Omit to auto-detect and confirm.
        #[arg(long = "harness", value_name = "NAME")]
        harnesses: Vec<String>,
        /// Patch user-level settings instead of project-local.
        #[arg(long)]
        global: bool,
        /// Skip the install confirmation prompt.
        #[arg(long)]
        yes: bool,
        /// Scaffold the config but install no hooks (legacy behavior).
        #[arg(long)]
        no_hook: bool,
        /// Skip config scaffolding; only wire hooks.
        #[arg(long)]
        hook_only: bool,
        /// Remove ironlint hooks and materialized artifacts.
        #[arg(long)]
        uninstall: bool,
        /// Print intended changes without writing.
        #[arg(long)]
        dry_run: bool,
    },
```

- [ ] **Step 4: Pass the fields through `main.rs`**

Replace `crates/ironlint-cli/src/main.rs:36`:

```rust
        Command::Init {
            dir,
            harnesses,
            global,
            yes,
            no_hook,
            hook_only,
            uninstall,
            dry_run,
        } => commands::init::run(
            &dir,
            commands::init::Options { harnesses, global, yes, no_hook, hook_only, uninstall, dry_run },
        )?,
```

- [ ] **Step 5: Extend `init::run`**

In `crates/ironlint-cli/src/commands/init/mod.rs`, add at the top of the module:

```rust
mod onboard;

pub struct Options {
    pub harnesses: Vec<String>,
    pub global: bool,
    pub yes: bool,
    pub no_hook: bool,
    pub hook_only: bool,
    pub uninstall: bool,
    pub dry_run: bool,
}
```

Replace the body of `pub fn run` (lines 17-43) with:

```rust
pub fn run(dir: &Path, opts: Options) -> Result<i32> {
    if opts.no_hook && opts.hook_only {
        return Err(anyhow!("--no-hook and --hook-only are mutually exclusive"));
    }

    if !opts.hook_only && !opts.uninstall {
        scaffold_config(dir)?;
    }

    if opts.no_hook {
        return Ok(0);
    }

    let env = ironlint_core::adapter::AdapterEnv::from_process(dir.to_path_buf())?;
    onboard::run_hook_phase(&env, &opts)
}

/// Scaffold + bless `.ironlint.yml`, treating an existing config as a no-op
/// (previously a hard error).
fn scaffold_config(dir: &Path) -> Result<()> {
    let cfg_path = dir.join(".ironlint.yml");
    if cfg_path.exists() {
        println!("config: {} already present (skipped)", cfg_path.display());
        return Ok(());
    }
    let stack = detect_stack(dir);
    let workspace = detect_workspace(dir);
    let linters = detect_linters(dir);
    let runner = detect_js_runner(dir);
    let body = build_config(stack, workspace.as_ref(), linters, runner);
    std::fs::write(&cfg_path, body)?;
    ironlint_core::trust::bless(&cfg_path)
        .map_err(|e| anyhow!("scaffolded {} but could not trust it: {e:#}", cfg_path.display()))?;
    println!("scaffolded and trusted: {}", cfg_path.display());
    Ok(())
}
```

(The old `run` printed a `ironlint check ...` hint; move that hint into the summary printed by `onboard::run_hook_phase` so output stays cohesive. The detect/`build_config` helpers are unchanged.)

- [ ] **Step 6: Stub `onboard` so the crate compiles**

Create `crates/ironlint-cli/src/commands/init/onboard.rs` with a temporary stub (replaced in Task 6):

```rust
use super::Options;
use anyhow::Result;
use ironlint_core::adapter::AdapterEnv;

pub fn run_hook_phase(_env: &AdapterEnv, _opts: &Options) -> Result<i32> {
    Ok(0)
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p ironlint-cli init`
Expected: PASS (existing init tests + the new non-fatal test). The pre-existing `run` tests that called `run(&dir)` (if any) must be updated to the new signature — search and fix: `rg "init::run\(" crates/ironlint-cli`.

- [ ] **Step 8: Lint + commit**

Run: `cargo fmt && cargo clippy -p ironlint-cli --all-targets -- -D warnings`
```bash
git add crates/ironlint-cli/src/cli.rs crates/ironlint-cli/src/main.rs crates/ironlint-cli/src/commands/init/
git commit -m "feat(init): onboarding flags, Options, non-fatal existing config"
```

---

### Task 6: Onboarding flow (detect → confirm → install → summary) + uninstall

**Files:**
- Modify: `crates/ironlint-cli/src/commands/init/onboard.rs` (replace stub)
- Create: `crates/ironlint-cli/tests/cli_init_onboarding.rs` (integration tests)

**Interfaces:**
- Consumes: `ironlint_core::adapter::{all_harnesses, detect, install, uninstall, AdapterEnv, Scope, InstallResult, Harness}`.
- Produces: `run_hook_phase`.

- [ ] **Step 1: Write the unit test for confirm-parsing**

End `crates/ironlint-cli/src/commands/init/onboard.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_confirm_defaults_yes_on_empty() {
        assert!(parse_confirm(""));
        assert!(parse_confirm("\n"));
        assert!(parse_confirm("y"));
        assert!(parse_confirm("YES"));
    }
    #[test]
    fn parse_confirm_no() {
        assert!(!parse_confirm("n"));
        assert!(!parse_confirm("no"));
        assert!(!parse_confirm("x"));
    }

    #[test]
    fn select_explicit_all_returns_every_harness() {
        let names = select_harness_names(&["all".to_string()]).unwrap();
        assert_eq!(names, vec!["claude-code", "reasonix", "pi", "opencode"]);
    }
    #[test]
    fn select_explicit_unknown_errors() {
        assert!(select_harness_names(&["bogus".to_string()]).is_err());
    }
    #[test]
    fn select_explicit_dedup_and_order() {
        let names = select_harness_names(&["pi".to_string(), "pi".to_string()]).unwrap();
        assert_eq!(names, vec!["pi"]);
    }
}
```

- [ ] **Step 2: Write the integration tests**

Create `crates/ironlint-cli/tests/cli_init_onboarding.rs`:

```rust
use assert_cmd::Command;
use std::path::Path;

fn ironlint(home: &Path, project: &Path) -> Command {
    let mut c = Command::cargo_bin("ironlint").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .current_dir(project);
    c
}

#[test]
fn init_installs_reasonix_hook_with_yes() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(home.join(".reasonix")).unwrap();

    ironlint(&home, &project)
        .args(["init", "--harness", "reasonix", "--yes"])
        .assert()
        .success();

    let hook = home.join(".config/ironlint/adapters/reasonix/hook.sh");
    assert!(hook.exists(), "hook artifact materialized");
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join(".reasonix/settings.json")).unwrap()).unwrap();
    assert!(settings["hooks"]["PreToolUse"][0]["command"]
        .as_str().unwrap().contains("adapters/reasonix/hook.sh"));
}

#[test]
fn reinstall_reports_already_present() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    let run = || {
        ironlint(&home, &project)
            .args(["init", "--hook-only", "--harness", "reasonix", "--yes"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    run();
    let out = String::from_utf8(run()).unwrap();
    assert!(out.contains("already present"), "second run idempotent: {out}");
}

#[test]
fn dry_run_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    ironlint(&home, &project)
        .args(["init", "--hook-only", "--harness", "reasonix", "--yes", "--dry-run"])
        .assert()
        .success();
    assert!(!home.join(".reasonix/settings.json").exists());
    assert!(!home.join(".config/ironlint/adapters/reasonix/hook.sh").exists());
}

#[test]
fn uninstall_removes_hook() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    ironlint(&home, &project).args(["init", "--hook-only", "--harness", "reasonix", "--yes"]).assert().success();
    ironlint(&home, &project).args(["init", "--uninstall", "--harness", "reasonix"]).assert().success();
    assert!(!home.join(".config/ironlint/adapters/reasonix/hook.sh").exists());
}

#[test]
fn no_tty_without_yes_or_harness_skips_hooks() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(home.join(".reasonix")).unwrap();

    // assert_cmd pipes stdin (non-TTY); bare init must not install.
    ironlint(&home, &project).args(["init", "--hook-only"]).assert().success();
    assert!(!home.join(".reasonix/settings.json").exists());
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ironlint-cli --test cli_init_onboarding`
Expected: FAIL — stub `run_hook_phase` does nothing; artifacts never appear; `parse_confirm`/`select_harness_names` undefined.

- [ ] **Step 4: Implement the flow**

Replace the non-test part of `crates/ironlint-cli/src/commands/init/onboard.rs` with:

```rust
use super::Options;
use anyhow::{anyhow, Result};
use ironlint_core::adapter::{
    all_harnesses, detect, install, uninstall, AdapterEnv, Harness, InstallResult, Scope,
};
use std::io::{IsTerminal, Write};

pub fn run_hook_phase(env: &AdapterEnv, opts: &Options) -> Result<i32> {
    let scope = if opts.global { Scope::Global } else { Scope::Local };
    let names = choose_harnesses(env, opts)?;
    if names.is_empty() {
        return Ok(0);
    }
    let registry = all_harnesses();
    let selected: Vec<&Harness> = names
        .iter()
        .filter_map(|n| registry.iter().find(|h| &h.name == n))
        .collect();

    let mut any_ok = false;
    let mut any_fail = false;
    for h in selected {
        let outcome = if opts.uninstall {
            uninstall(h, env, scope, opts.dry_run)
        } else {
            install(h, env, scope, opts.dry_run)
        };
        match outcome {
            Ok(o) => {
                any_ok = true;
                print_outcome(&o.harness, &o.result, o.hint, opts.uninstall);
            }
            Err(e) => {
                any_fail = true;
                println!("  {:<12} failed: {e:#}", h.name);
            }
        }
    }
    Ok(if any_fail && !any_ok { 3 } else { 0 })
}

/// Resolve the harness set: explicit `--harness`, else detect+confirm.
fn choose_harnesses(env: &AdapterEnv, opts: &Options) -> Result<Vec<String>> {
    if !opts.harnesses.is_empty() {
        return select_harness_names(&opts.harnesses);
    }
    let detected: Vec<String> = detect(env)
        .into_iter()
        .filter(|(_, found)| *found)
        .map(|(n, _)| n.to_string())
        .collect();
    if detected.is_empty() {
        println!("no supported harnesses detected; run `ironlint init --harness all` to wire all four");
        return Ok(vec![]);
    }
    if opts.yes {
        return Ok(detected);
    }
    if !std::io::stdin().is_terminal() {
        println!(
            "detected: {} — re-run with `--yes` or `--harness <name>` to install",
            detected.join(", ")
        );
        return Ok(vec![]);
    }
    print!("Install ironlint hooks into {}? [Y/n] ", detected.join(", "));
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(if parse_confirm(&line) { detected } else { vec![] })
}

/// Validate explicit `--harness` names; `all` expands to the full registry.
fn select_harness_names(requested: &[String]) -> Result<Vec<String>> {
    let known: Vec<&'static str> = all_harnesses().iter().map(|h| h.name).collect();
    let mut out: Vec<String> = Vec::new();
    for r in requested {
        if r == "all" {
            return Ok(known.iter().map(|s| s.to_string()).collect());
        }
        if !known.contains(&r.as_str()) {
            return Err(anyhow!(
                "unknown harness `{r}` (supported: {})",
                known.join(", ")
            ));
        }
        if !out.contains(r) {
            out.push(r.clone());
        }
    }
    Ok(out)
}

fn parse_confirm(line: &str) -> bool {
    let a = line.trim().to_lowercase();
    a.is_empty() || a == "y" || a == "yes"
}

fn print_outcome(harness: &str, result: &InstallResult, hint: &str, uninstalling: bool) {
    match result {
        InstallResult::Installed if uninstalling => println!("  {harness:<12} removed"),
        InstallResult::Installed => println!("  {harness:<12} installed — {hint}"),
        InstallResult::Updated => println!("  {harness:<12} updated — {hint}"),
        InstallResult::AlreadyPresent => println!("  {harness:<12} already present"),
        InstallResult::Skipped(why) => println!("  {harness:<12} skipped: {why}"),
        InstallResult::Failed(why) => println!("  {harness:<12} failed: {why}"),
        InstallResult::DryRun(plan) => {
            println!("  {harness:<12} dry-run:");
            for line in plan {
                println!("      {line}");
            }
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ironlint-cli --test cli_init_onboarding && cargo test -p ironlint-cli init::`
Expected: PASS (5 integration + 5 unit).

- [ ] **Step 6: Lint + commit**

Run: `cargo fmt && cargo clippy -p ironlint-cli --all-targets -- -D warnings`
(If `run_hook_phase` trips complexity, extract `choose_harnesses`/`print_outcome` further — they already are; the loop is flat.)
```bash
git add crates/ironlint-cli/src/commands/init/onboard.rs crates/ironlint-cli/tests/cli_init_onboarding.rs
git commit -m "feat(init): detect/confirm/install/uninstall onboarding flow"
```

---

### Task 7: `ironlint doctor` adapters section

**Files:**
- Modify: `crates/ironlint-cli/src/commands/doctor.rs` (add an adapters section to both `human` and `json` output)

**Interfaces:**
- Consumes: `ironlint_core::adapter::{all_harnesses, status, AdapterEnv, Scope, HarnessStatus}`.
- Produces: doctor output additions (no new public fns required beyond a local `adapter_report`).

- [ ] **Step 1: Read the current doctor shape**

Run: `sed -n '1,60p' crates/ironlint-cli/src/commands/doctor.rs` to see how it assembles the human checklist and the JSON report (it has a `--format` switch). Match that structure — append an `adapters` array to the JSON report and an "Adapters" block to the human checklist. Do not change existing fields (the JSON schema is documented in `docs/operating/diagnostics.md`; bump that doc's schema note in Task 8).

- [ ] **Step 2: Write the failing test**

Add to the `tests` module in `crates/ironlint-cli/src/commands/doctor.rs` (use the same temp-env pattern the file already uses; if it shells via assert_cmd, add this to `tests/` instead — follow the file's existing convention):

```rust
#[test]
fn doctor_reports_installed_adapter() {
    let tmp = tempfile::tempdir().unwrap();
    let env = ironlint_core::adapter::AdapterEnv {
        home: tmp.path().to_path_buf(),
        config_home: tmp.path().join(".config"),
        project_root: tmp.path().join("proj"),
    };
    let h = ironlint_core::adapter::all_harnesses().into_iter().find(|h| h.name == "reasonix").unwrap();
    ironlint_core::adapter::install(&h, &env, ironlint_core::adapter::Scope::Global, false).unwrap();
    let report = adapter_report(&env);
    let reasonix = report.iter().find(|s| s.harness == "reasonix").unwrap();
    assert!(reasonix.installed && reasonix.registered);
    assert_eq!(reasonix.intact, Some(true));
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p ironlint-cli doctor_reports_installed_adapter`
Expected: FAIL — `adapter_report` not defined.

- [ ] **Step 4: Implement**

Add to `crates/ironlint-cli/src/commands/doctor.rs`:

```rust
use ironlint_core::adapter::{all_harnesses, status, AdapterEnv, HarnessStatus, Scope};

/// Status of every supported harness for the doctor report. Global scope is
/// used for status (the broadest view); project-local artifacts still surface
/// because detection and plugin dirs are checked under the project root.
fn adapter_report(env: &AdapterEnv) -> Vec<HarnessStatus> {
    all_harnesses()
        .iter()
        .map(|h| status(h, env, Scope::Global).unwrap_or(HarnessStatus {
            harness: h.name,
            detected: false,
            installed: false,
            registered: false,
            intact: None,
            current: None,
        }))
        .collect()
}

/// One human-readable line per harness.
fn adapter_lines(report: &[HarnessStatus]) -> Vec<String> {
    report
        .iter()
        .map(|s| {
            let state = if !s.installed {
                if s.detected { "detected, not installed" } else { "not installed" }
            } else if !s.registered {
                "installed but not registered (broken)"
            } else if s.intact == Some(false) {
                "installed, MODIFIED (re-run ironlint init)"
            } else if s.current == Some(false) {
                "installed, outdated (re-run ironlint init)"
            } else {
                "installed, ok"
            };
            format!("  {:<12} {state}", s.harness)
        })
        .collect()
}
```

Then, in doctor's `run`: build `let report = adapter_report(&AdapterEnv::from_process(dir.to_path_buf())?);`. For `human` format, print an `Adapters:` header followed by `adapter_lines(&report)`. For `json` format, serialize each `HarnessStatus` into the existing report object under an `"adapters"` key — derive `Serialize` on `HarnessStatus` in `ironlint-core` (add `#[derive(serde::Serialize)]` to the struct in `ops.rs`). An "installed but not registered (broken)" harness sets the doctor exit code to 1 (a real wiring failure); drift/outdated are warnings (exit 0), matching doctor's existing fail-on-failure-only contract.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ironlint-cli doctor && cargo test -p ironlint-core` (the latter for the new `Serialize` derive)
Expected: PASS.

- [ ] **Step 6: Lint + commit**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings`
```bash
git add crates/ironlint-cli/src/commands/doctor.rs crates/ironlint-core/src/adapter/ops.rs
git commit -m "feat(doctor): adapters section reporting install/registration/integrity"
```

---

### Task 8: Docs, migration, full verification

**Files:**
- Modify: `adapters/claude-code/README.md`, `adapters/reasonix/README.md`, `adapters/pi/README.md`, `adapters/opencode/README.md` (lead with `ironlint init`)
- Modify: `adapters/reasonix/install.sh` (deprecation banner)
- Modify: `docs/operating/diagnostics.md` (document the new `adapters` JSON key)
- Modify: `CLAUDE.md` (note `init` now onboards; `doctor` gains an adapters section)
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Rewrite each adapter README's install section**

For each `adapters/<harness>/README.md`, replace the manual install steps with the one-command form, keeping the old steps under a "Manual fallback" heading. Example for reasonix — its primary install becomes:

```markdown
## Install

```bash
ironlint init --harness reasonix      # or bare `ironlint init` to detect + confirm
```

This materializes the hook to `~/.config/ironlint/adapters/reasonix/hook.sh` and
registers a PreToolUse entry in `~/.reasonix/settings.json` (atomic, backed up
to `settings.json.bak`). Verify with `ironlint doctor`. Remove with
`ironlint init --uninstall --harness reasonix`.
```

Use the corresponding default path/scope per harness (claude-code: project `.claude/settings.json`, `--global` for user; pi: `.pi/extensions/ironlint.ts`; opencode: `.opencode/plugins/ironlint.ts`).

- [ ] **Step 2: Deprecate the reasonix shell installer**

Add to the top of `adapters/reasonix/install.sh` (after the shebang/`set -euo pipefail`):

```bash
echo "NOTE: this installer is superseded by \`ironlint init --harness reasonix\`." >&2
echo "      It remains as a fallback for environments without the ironlint binary." >&2
```

- [ ] **Step 3: Update diagnostics doc + CLAUDE.md + CHANGELOG**

- In `docs/operating/diagnostics.md`, add the `adapters` array to the documented JSON schema: each element `{ "harness", "detected", "installed", "registered", "intact", "current" }`.
- In `CLAUDE.md`, update the CLI line so `init` reads "scaffolds config **and onboards hooks into detected harnesses**", and note `doctor` now reports adapter status. Keep it one or two lines.
- In `CHANGELOG.md`, add an Unreleased entry: `ironlint init now installs ironlint's hook into detected coding agents (claude-code, reasonix, pi, opencode) with detect-then-confirm UX, --dry-run, and --uninstall; ironlint doctor reports adapter status.`

- [ ] **Step 4: Full workspace verification**

Run each and confirm:
```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
Expected: all green.

- [ ] **Step 5: Coverage gate on touched files**

Run: `bash scripts/ci-coverage.sh`
Expected: every touched `crates/*/src/` file ≥80% region coverage. If a file is short, add focused tests for the uncovered arms (e.g. the `--no-hook --hook-only` error, plugin `Updated` path, `choose_harnesses` no-detected branch). Per the repo's note, `ci-coverage.sh` may not run locally without `llvm-tools-preview` — if so, state that it's CI-verified and lean on the per-task unit/integration tests for local confidence.

- [ ] **Step 6: Manual smoke test (real binary, throwaway HOME)**

```bash
cargo build --release
HOME=$(mktemp -d) XDG_CONFIG_HOME=$(mktemp -d) ./target/release/ironlint init --hook-only --harness all --yes --dry-run
```
Expected: a dry-run plan listing artifact writes + settings patches for all four harnesses; nothing written.

- [ ] **Step 7: Clean up build artifacts + commit**

Per the repo's cleanup rule, drop the release binary built only for the smoke test:
```bash
cargo clean -p ironlint-cli
git add adapters/ docs/operating/diagnostics.md CLAUDE.md CHANGELOG.md
git commit -m "docs(init): onboard via ironlint init; deprecate per-adapter installers"
```

---

## Self-Review

**Spec coverage:**
- §3 command surface → Task 5 (flags) ✓
- §4 run flow (config/detect/select/confirm/install/summary) → Tasks 5–6 ✓
- §5 architecture (registry, JsonHook/Plugin mechanisms, thin CLI) → Tasks 3–6 ✓
- §6 detection & install targets table → Task 3 registry literals ✓
- §7 install mechanics (atomic, backup, sidecar, idempotency, version) → Tasks 1, 4 ✓
- §8 doctor adapters section → Task 7 ✓
- §9 uninstall → Tasks 4, 6 ✓
- §10 error handling (per-harness, exit only if all fail; usage error) → Tasks 5–6 ✓
- §11 testing (unit + assert_cmd + coverage/complexity) → every task + Task 8 ✓
- §12 out-of-scope respected (no binary-hook port, no new harnesses, no daemon) ✓
- §13 migration notes → Task 8 ✓

**Placeholder scan:** One intentional residue to delete — the `json_detect_unused: ()` field listed in the inventory's `PluginSpec` is a leftover; the real `PluginSpec` in Task 3 does **not** include it (it has `detect: fn(&AdapterEnv) -> bool`). Implement Task 3's version. No other TBD/TODO/"handle edge cases" left; every code step ships real code.

**Type consistency:** `InstallResult`/`InstallOutcome`/`HarnessStatus` defined in Task 4 (`ops.rs`) and consumed unchanged in Tasks 6–7. `AdapterEnv` fields (`home`, `config_home`, `project_root`) consistent across Tasks 3–7. `sync_hook_array`/`remove_from_hook_array` signatures consistent Tasks 2→4. `select_harness_names`/`parse_confirm`/`run_hook_phase` consistent Tasks 5→6. Registry `build_entry` free fns (`claude_build_entry`, `reasonix_build_entry`) named identically in their test (Task 3) and use (Task 3 registry). One naming note carried into Task 4: `uninstall` returns `InstallResult::Installed` as its success marker (documented in the test); if a `Removed` variant is preferred, add it in Task 4 and thread it through `print_outcome` (Task 6) — pick one and keep both files in sync.
