# hector-core Resilience & Correctness Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the highest-value correctness/resilience defects in `crates/hector-core` surfaced by the 2026-05-28 review — a `clone(2)` deadlock hazard, silent script-output truncation, stale-content evaluation, brittle LLM parsing, missing retry, and duplicated glob logic — each as an independent, test-first change.

**Architecture:** Six self-contained tasks, ordered to group edits by file (capability.rs ×2, then the semantic/context path, then the LLM layer ×2, then the config/runner glob dedup). Every task is a bug-fix or hardening change driven by a failing test first, per the repo's "bug fixes start with a failing test" rule. No task changes the locked verdict JSON shape or the `EngineKind` dispatch contract.

**Tech Stack:** Rust (edition from workspace), `anyhow` errors, `nix` (Linux clone/exec syscalls), `rayon` (parallel dispatch), `globset`, `reqwest::blocking`, `wiremock` + `tokio` (HTTP tests), `tempfile`, `insta`. Cargo workspace; crate name `hector-core`, binary `hector`.

---

## Definition of done (applies to EVERY task)

Before marking a task complete, all of these must hold — run them in order and confirm each exits 0:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings      # cognitive complexity per fn capped at 15
cargo test -p hector-core                       # full core suite green
bash scripts/ci-coverage.sh                     # per-file ≥80% region coverage gate (matches CI)
```

Repo rules that bind this plan (from `CLAUDE.md`):
- **Bug fixes start with a failing test.** Each task below leads with the failing test; that test becomes regression coverage.
- **Cognitive complexity ≤ 15 per function** (clippy, enforced). Keep extracted helpers small; do not reach for `#[allow(clippy::cognitive_complexity)]`.
- **≥80% region coverage per touched file** (`scripts/ci-coverage.sh`). Code added without bringing the file to the gate breaks the build — add tests for new branches.
- **After completing a coding task, request code review from a separate agent.**
- **Clean up build artifacts you produced** (`cargo clean -p <crate>`, scratch files). Note: `target/llvm-cov-target` / `target/llvm-cov` left by `ci-coverage.sh` must be removed before finishing (`rm -rf target/llvm-cov-target target/llvm-cov`) — a stop hook enforces this.
- `Cargo.lock` is gitignored — do not commit it.
- Commit messages end with the `Co-Authored-By` trailer shown in each task.

Run a single new test file with `cargo test -p hector-core --test <file_stem>`; filter by test-fn name with `cargo test -p hector-core <name>`.

---

## File Structure

Files created or modified, grouped by responsibility:

- `crates/hector-core/src/engine/capability.rs` — **Tasks 1 & 2.** Script-subprocess runner. Task 1 makes stdout/stderr draining concurrent with the wait (fixes the pipe-buffer deadlock). Task 2 makes the cloned child async-signal-safe (pre-builds CStrings/envp in the parent, execs via `execve`+`chdir`). Same file — Task 2 re-reads it after Task 1 lands.
- `crates/hector-core/tests/capability.rs` — **Tasks 1 & 2.** Integration tests for the runner (existing file; append new tests).
- `crates/hector-core/src/engine/context.rs` — **Task 3.** `expand_context` gains an authoritative-content parameter so `context: file`/`repo` rules honor proposed `--content` instead of re-reading disk.
- `crates/hector-core/src/engine/semantic.rs` — **Task 3.** Passes `ctx.content` into `expand_context`.
- `crates/hector-core/src/runner.rs` — **Tasks 3 & 6.** Task 3 threads authoritative content through the deferred/preview call sites of `expand_context`. Task 6 deletes the two hand-rolled glob-identification helpers and calls the matchers' new `matched_pattern`.
- `crates/hector-core/tests/context_expansion.rs` — **Task 3.** Existing file; append content-precedence tests.
- `crates/hector-core/src/llm/mod.rs` — **Tasks 4 & 5.** Task 4 replaces the fragile first-`[`/last-`]` slice in `parse_verdicts` with a balanced-array scan. Task 5 adds a generic `retry_with_backoff` helper + `is_retryable_status`/`backoff_delay`.
- `crates/hector-core/src/llm/anthropic.rs` — **Task 5.** Wraps the request send in `retry_with_backoff`.
- `crates/hector-core/src/llm/openai_compat.rs` — **Task 5.** Same.
- `crates/hector-core/tests/anthropic.rs` — **Task 5.** Existing file; append a 429→200 recovery test.
- `crates/hector-core/src/config/scope.rs` — **Task 6.** `ScopeMatcher` tracks raw patterns and exposes `matched_pattern`.
- `crates/hector-core/src/config/skip.rs` — **Task 6.** `SkipMatcher` does the same.

---

## Task 1: Drain script stdout/stderr concurrently with the wait

**Why:** `spawn_with_timeout` and `wait_for_child` both wait for the child to *exit* before reading its pipes. A script that writes more than the OS pipe buffer (~64 KiB) before exiting blocks on `write(2)`, never exits, trips the 5s timeout, and is killed with its output lost — so the documented `MAX_OUTPUT = 1 MiB` cap is unreachable and a verbose linter (e.g. `output: parsed` with 100 KB of findings) looks like a hang. Fix: drain both streams on dedicated threads while waiting, capped at `MAX_OUTPUT`.

**Files:**
- Modify: `crates/hector-core/src/engine/capability.rs` (`spawn_with_timeout` ~lines 422-470; `wait_for_child` ~lines 296-338; remove `read_pipes_bounded` ~lines 343-357; add `spawn_reader`/`join_reader` helpers)
- Test: `crates/hector-core/tests/capability.rs` (append)

- [ ] **Step 1: Write the failing test**

Append to `crates/hector-core/tests/capability.rs`:

```rust
#[test]
fn captures_large_output_without_deadlocking() {
    // A script that writes more than the OS pipe buffer (~64 KiB on Linux)
    // before exiting must not deadlock. Before the concurrent-drain fix the
    // child blocks on write(2), never exits, and trips the 5s timeout with
    // empty stdout and exit code 124. `network: true` keeps Linux on the
    // shared spawn_with_timeout fast path (no clone), exercising the path
    // every platform shares.
    let caps = Capabilities {
        network: true,
        writes: WritesPolicy::Unrestricted,
    };
    let start = std::time::Instant::now();
    // `yes x` emits "x\n" forever; `head -n 200000` caps the pipeline at
    // ~400 KiB and exits 0 (sh's status is the last pipeline stage).
    let out = run_with_capabilities("yes x | head -n 200000", std::path::Path::new("/tmp"), &caps)
        .expect("runner returns Ok");
    assert_eq!(
        out.exit_code, 0,
        "must exit cleanly, not time out (124); stderr was: {:?}",
        out.stderr
    );
    assert!(
        out.stdout.len() > 64 * 1024,
        "must capture more than one pipe buffer of stdout; got {} bytes",
        out.stdout.len()
    );
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "must not hit the 5s timeout; took {:?}",
        start.elapsed()
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p hector-core --test capability captures_large_output_without_deadlocking -- --nocapture`
Expected: FAIL — either `exit_code` is `124` (timeout) with empty stdout, or it takes ~5s.

- [ ] **Step 3: Add the reader helpers**

In `crates/hector-core/src/engine/capability.rs`, add these two helpers (place them just above `spawn_with_timeout`, with no `#[cfg]` gate — both targets use them):

```rust
/// Drain a child stream on its own thread, capped at `MAX_OUTPUT`, returning
/// the captured text. Reading concurrently with the wait prevents the child
/// from blocking on `write(2)` once it fills the OS pipe buffer.
fn spawn_reader<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = reader.take(MAX_OUTPUT as u64).read_to_string(&mut buf);
        buf
    })
}

/// Join a reader thread, treating a panicked reader as empty output (the
/// stream is best-effort diagnostic data, never load-bearing).
fn join_reader(handle: std::thread::JoinHandle<String>) -> String {
    handle.join().unwrap_or_default()
}
```

- [ ] **Step 4: Rewrite `spawn_with_timeout` to drain concurrently**

Replace the body of `spawn_with_timeout` (everything after the `command.spawn()?` line through the final `Ok(ExecOutcome { ... })`) with:

```rust
    let mut child = command.spawn().context("spawning script subprocess")?;

    // Drain both streams on dedicated threads BEFORE waiting, so a child that
    // writes past the pipe buffer never blocks on write(2). Each reader is
    // capped at MAX_OUTPUT and ends when the child's write-end closes.
    let stdout_reader = child.stdout.take().map(spawn_reader);
    let stderr_reader = child.stderr.take().map(spawn_reader);

    let status = child
        .wait_timeout(TIMEOUT)
        .context("waiting for subprocess")?;

    let Some(status) = status else {
        // Timeout fired. Kill and reap; the readers then hit EOF and finish.
        let _ = child.kill();
        let _ = child.wait();
        if let Some(h) = stdout_reader {
            let _ = h.join();
        }
        if let Some(h) = stderr_reader {
            let _ = h.join();
        }
        return Ok(ExecOutcome {
            stdout: String::new(),
            stderr: format!("hector: script killed after {TIMEOUT:?} timeout"),
            exit_code: TIMEOUT_EXIT_CODE,
        });
    };

    let stdout = stdout_reader.map(join_reader).unwrap_or_default();
    let stderr = stderr_reader.map(join_reader).unwrap_or_default();

    Ok(ExecOutcome {
        stdout,
        stderr,
        exit_code: status.code().unwrap_or(-1),
    })
```

- [ ] **Step 5: Rewrite the Linux `wait_for_child` to drain concurrently and delete `read_pipes_bounded`**

Replace the entire `wait_for_child` function body with the version below, and **delete** the now-unused `read_pipes_bounded` function:

```rust
#[cfg(target_os = "linux")]
fn wait_for_child(
    pid: nix::unistd::Pid,
    stdout_r: std::os::fd::OwnedFd,
    stderr_r: std::os::fd::OwnedFd,
) -> Result<ExecOutcome> {
    use nix::sys::signal::{kill, Signal};
    use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};

    // Drain the pipes on dedicated threads up front so the child never blocks
    // on write(2) after filling the kernel pipe buffer.
    let stdout_reader = spawn_reader(std::fs::File::from(stdout_r));
    let stderr_reader = spawn_reader(std::fs::File::from(stderr_r));

    let deadline = std::time::Instant::now() + TIMEOUT;
    let poll_interval = std::time::Duration::from_millis(10);

    let exit_status = loop {
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)).context("waitpid on cloned child")? {
            WaitStatus::StillAlive => {
                if std::time::Instant::now() >= deadline {
                    let _ = kill(pid, Signal::SIGKILL);
                    let _ = waitpid(pid, None);
                    let _ = join_reader(stdout_reader);
                    let _ = join_reader(stderr_reader);
                    return Ok(ExecOutcome {
                        stdout: String::new(),
                        stderr: format!("hector: script killed after {TIMEOUT:?} timeout"),
                        exit_code: TIMEOUT_EXIT_CODE,
                    });
                }
                std::thread::sleep(poll_interval);
            }
            other => break other,
        }
    };

    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    Ok(ExecOutcome {
        stdout,
        stderr,
        exit_code: exit_status_to_code(exit_status),
    })
}
```

Also update the `wait_for_child` doc comment: the "If a child writes more than the kernel pipe buffer ... it will block on `write(2)` and trip the timeout" note is now obsolete — replace it with "stdout/stderr are drained on reader threads, so large output no longer trips the timeout."

- [ ] **Step 6: Run the new test and the full capability suite**

Run: `cargo test -p hector-core --test capability`
Expected: PASS — including the existing `capability_run_kills_runaway_command` (timeout path still works) and `linux_network_disabled_blocks_network_attempts`.

- [ ] **Step 7: Verify the Definition of done, then commit**

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -p hector-core
rm -rf target/llvm-cov-target target/llvm-cov
git add crates/hector-core/src/engine/capability.rs crates/hector-core/tests/capability.rs
git commit -m "fix(capability): drain script output concurrently to honor MAX_OUTPUT

Reader threads drain stdout/stderr while waiting, so a child that writes
past the pipe buffer no longer blocks on write(2), exits cleanly, and its
output is captured up to MAX_OUTPUT instead of being lost to the timeout.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Make the cloned script child async-signal-safe

**Why:** On Linux, `network: false` script rules run via `nix::sched::clone`, called from `evaluate_one_rule` inside rayon's `par_iter` — so the parent is multithreaded at clone time. The child (`child_main`) then calls `std::env::set_var` (takes std's global ENV lock), `std::env::set_current_dir`, and `CString::new` (allocates) — none async-signal-safe. Raw `clone(2)` (unlike `fork()`) does not run libc's `pthread_atfork` handlers, so if a sibling rayon thread holds the malloc arena lock or the ENV lock at the clone instant, the single-threaded child deadlocks. Fix: pre-build every `CString` and an explicit `envp` in the parent, and in the child call only `dup2` + `chdir` + `execve` (raw syscalls). `execve` replaces the environment wholesale, so forward the parent's full env plus the injected overrides.

> The deadlock is timing-dependent and cannot be reproduced deterministically in a test. The regression guard is the env/cwd/exec contract: the new test asserts an injected env var reaches the child **and** an inherited var (`PATH`, needed for `sh` to find commands) survives the `execve` switch, plus the existing `capability.rs` tests (network isolation, runaway-kill) must stay green.

**Files:**
- Modify: `crates/hector-core/src/engine/capability.rs` (`spawn_clone` ~lines 148-216; rename/rewrite `child_main` → `child_exec` ~lines 226-282)
- Test: `crates/hector-core/tests/capability.rs` (append)

- [ ] **Step 1: Write the failing test**

Append to `crates/hector-core/tests/capability.rs` (the import line at the top of the file must include `run_with_capabilities_env` — change `use hector_core::engine::capability::run_with_capabilities;` to `use hector_core::engine::capability::{run_with_capabilities, run_with_capabilities_env};`):

```rust
#[cfg(target_os = "linux")]
#[test]
fn clone_child_receives_injected_env_and_inherits_path() {
    // network:false routes through the clone path on Linux. The injected var
    // must reach the child, AND the parent's PATH must survive — execve
    // replaces the environment wholesale, so the runner must forward the full
    // parent env plus the overrides. (On unprivileged CI, clone(2) returns
    // EPERM and falls back to spawn_with_timeout, which also forwards env, so
    // this holds on both paths.)
    let caps = Capabilities {
        network: false,
        writes: WritesPolicy::None,
    };
    let out = run_with_capabilities_env(
        "printf '%s\\n' \"$HECTOR_TEST_VAR\"; command -v sh >/dev/null && echo PATH_OK",
        std::path::Path::new("/tmp"),
        &caps,
        &[("HECTOR_TEST_VAR", "injected-value")],
    )
    .expect("run");
    assert!(
        out.stdout.contains("injected-value"),
        "injected env var must reach the child; stdout: {:?}",
        out.stdout
    );
    assert!(
        out.stdout.contains("PATH_OK"),
        "inherited PATH must survive execve; stdout: {:?}",
        out.stdout
    );
}
```

- [ ] **Step 2: Run the test to verify it passes today but for the wrong reason, then confirm the refactor keeps it green**

Run: `cargo test -p hector-core --test capability clone_child_receives_injected_env_and_inherits_path`
Expected on Linux: PASS today (the current `set_var`+`execv` path forwards env and inherits PATH). This test is the **behavioral contract** the refactor must preserve. On non-Linux it is skipped (`#[cfg(target_os = "linux")]`).

> If you are working on macOS, you cannot exercise the clone path locally. Implement the change, confirm the non-Linux build still compiles (`cargo build -p hector-core`), and rely on Linux CI for this test. State this explicitly when requesting review.

- [ ] **Step 3: Rewrite `spawn_clone` to pre-build CStrings and envp in the parent**

In `spawn_clone`, replace the block that captures the child inputs and builds `child_fn` (from `let cmd_string = cmd.to_string();` down to the `let child_fn: nix::sched::CloneCb<'_> = Box::new(...)` assignment) with:

```rust
    use std::ffi::CString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    // Pre-build EVERYTHING the child needs as CStrings in the PARENT. After
    // clone(2) the child shares a COW copy of this memory and must call only
    // async-signal-safe functions: no malloc, no std ENV lock. Raw clone (unlike
    // fork) does not run libc's atfork handlers, so a sibling rayon thread
    // holding the malloc/ENV lock at clone time would otherwise deadlock the
    // child on its first allocation or set_var.
    let argv: Vec<CString> = vec![
        CString::new("sh").expect("static arg has no interior NUL"),
        CString::new("-c").expect("static arg has no interior NUL"),
        CString::new(cmd).context("script command contains an interior NUL byte")?,
    ];
    let sh_path = CString::new("/bin/sh").expect("static path has no interior NUL");
    let cwd_c =
        CString::new(cwd.as_os_str().as_bytes()).context("cwd contains an interior NUL byte")?;

    // execve replaces the environment, so forward the parent's full env plus
    // the injected overrides — otherwise the child loses PATH and `sh` cannot
    // resolve the linter. BTreeMap dedups (override wins) and is deterministic.
    let mut env_map: std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString> =
        std::env::vars_os().collect();
    for (k, v) in env {
        env_map.insert(std::ffi::OsString::from(*k), std::ffi::OsString::from(*v));
    }
    let envp: Vec<CString> = env_map
        .into_iter()
        .filter_map(|(k, v)| {
            let mut kv = k.into_vec();
            kv.push(b'=');
            kv.extend_from_slice(&v.into_vec());
            CString::new(kv).ok() // drop any pair with an interior NUL
        })
        .collect();

    let stdout_w_raw = stdout_w.as_raw_fd();
    let stderr_w_raw = stderr_w.as_raw_fd();

    let child_fn: nix::sched::CloneCb<'_> = Box::new(move || -> isize {
        child_exec(stdout_w_raw, stderr_w_raw, &cwd_c, &sh_path, &argv, &envp)
    });
```

> Keep the existing `let stdout_w_raw = stdout_w.as_raw_fd();` / `let stderr_w_raw = ...` only once — the snippet above includes them, so remove the originals if they now appear twice. Leave the `use std::os::fd::AsRawFd;` import and the pipe/stack setup above this block unchanged, and leave the `unsafe { nix::sched::clone(...) }` call, `Box::leak(stack)`, and `drop(stdout_w); drop(stderr_w);` below it unchanged.

- [ ] **Step 4: Replace `child_main` with an async-signal-safe `child_exec`**

Delete the entire `child_main` function (lines ~226-282) and replace it with:

```rust
/// Body of the cloned child. Runs in the child's COW address space until
/// `execve` replaces it. Calls ONLY async-signal-safe syscalls (`dup2`,
/// `chdir`, `execve`) on data the parent pre-built — no allocation, no locks.
///
/// Conventions:
/// - exit 126: `dup2` or `chdir` failed before exec
/// - exit 127: `execve` failed (command not found / not executable) —
///   matches POSIX shell convention for "command not found"
#[cfg(target_os = "linux")]
fn child_exec(
    stdout_w_raw: std::os::fd::RawFd,
    stderr_w_raw: std::os::fd::RawFd,
    cwd: &std::ffi::CStr,
    sh_path: &std::ffi::CStr,
    argv: &[std::ffi::CString],
    envp: &[std::ffi::CString],
) -> isize {
    use nix::unistd::{chdir, dup2, execve};

    // Redirect stdout/stderr to the pipe write-ends.
    if dup2(stdout_w_raw, 1).is_err() || dup2(stderr_w_raw, 2).is_err() {
        return 126;
    }
    // `chdir(&CStr)` is a raw chdir(2) with no allocation (CStr: NixPath).
    if chdir(cwd).is_err() {
        return 126;
    }
    // execve passes argv + envp directly; does not return on success.
    let _ = execve(sh_path, argv, envp);
    127
}
```

- [ ] **Step 5: Update the `spawn_clone` SAFETY comment**

The big `// SAFETY:` comment above `nix::sched::clone` currently says the closure "performs syscalls (`dup2`, `chdir`, `setenv`, `execv`)". Update it to read: "the closure calls only `child_exec`, which performs `dup2`/`chdir`/`execve` on parent-pre-built CStrings — async-signal-safe, with no allocation or lock acquisition, so a multithreaded parent at clone time cannot deadlock the child."

- [ ] **Step 6: Build and run the capability suite**

Run: `cargo build -p hector-core` then `cargo test -p hector-core --test capability`
Expected: compiles; on Linux all capability tests PASS (including the new env test, network isolation, and runaway-kill). On macOS the Linux-gated tests are skipped.

- [ ] **Step 7: Verify the Definition of done, then commit**

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -p hector-core
rm -rf target/llvm-cov-target target/llvm-cov
git add crates/hector-core/src/engine/capability.rs crates/hector-core/tests/capability.rs
git commit -m "fix(capability): make cloned child async-signal-safe

Pre-build argv/envp/cwd CStrings in the parent and exec via execve+chdir
instead of set_var+set_current_dir+execv in the child. Raw clone(2) skips
libc atfork handlers, so the prior child could deadlock on malloc/ENV-lock
contention with sibling rayon threads. The child now touches no allocator
or lock. envp forwards the parent env plus injected overrides so PATH
survives the execve switch.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Honor authoritative `--content` in `context: file`/`repo` expansion

**Why:** `expand_context` re-reads the file from disk for `File`/`Repo` scope, ignoring the authoritative `content` the runner already holds. The AST engine uses `ctx.content` correctly, but the semantic File/Repo path does not — so in PreToolUse `--content` mode (proposed edit not yet on disk) a `context: file` semantic rule evaluates the *old* file (or errors on a new file). It's also a redundant second disk read. Fix: add a `content` parameter to `expand_context`, prefer it when present, and thread it from the semantic engine and the deferred/runner paths so direct-API and subagent routes stay byte-identical.

**Files:**
- Modify: `crates/hector-core/src/engine/context.rs` (`expand_context` signature + `File`/`Repo` arms)
- Modify: `crates/hector-core/src/engine/semantic.rs` (the `expand_context` call ~line 13)
- Modify: `crates/hector-core/src/runner.rs` (`render_semantic_prompts` call ~line 1496; `expand_deferred_contexts` ~lines 1435-1471 + its caller `build_deferred_evaluator_input` ~line 1221 + that caller in `check_inner` ~line 1341)
- Test: `crates/hector-core/tests/context_expansion.rs` (append)

- [ ] **Step 1: Write the failing test**

Append to `crates/hector-core/tests/context_expansion.rs`:

```rust
#[test]
fn file_scope_prefers_authoritative_content_over_disk() {
    // PreToolUse passes proposed content that is not yet on disk. When the
    // caller supplies authoritative content, File scope must use it, not the
    // stale on-disk bytes.
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "OLD DISK CONTENT\n").unwrap();
    let (primary, ctx) = expand_context(
        ContextScope::File,
        None,
        Some(&file),
        Some("PROPOSED NEW CONTENT"),
        dir.path(),
    )
    .unwrap();
    assert!(primary.contains("PROPOSED NEW CONTENT"));
    assert!(!primary.contains("OLD DISK CONTENT"));
    assert!(ctx.is_none());
}

#[test]
fn file_scope_uses_supplied_content_even_when_file_is_absent() {
    // A brand-new file proposed in PreToolUse mode has no disk bytes at all.
    let dir = tempdir().unwrap();
    let missing = dir.path().join("brand-new.txt");
    let (primary, _ctx) = expand_context(
        ContextScope::File,
        None,
        Some(&missing),
        Some("content for a file not yet written"),
        dir.path(),
    )
    .expect("supplied content needs no disk read");
    assert!(primary.contains("content for a file not yet written"));
}

#[test]
fn file_scope_falls_back_to_disk_when_no_content_supplied() {
    // Diff mode and the prompt-preview path pass content: None and must keep
    // reading from disk.
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "the whole file\n").unwrap();
    let (primary, _ctx) =
        expand_context(ContextScope::File, None, Some(&file), None, dir.path()).unwrap();
    assert!(primary.contains("the whole file"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p hector-core --test context_expansion file_scope_prefers_authoritative_content`
Expected: FAIL to compile — `expand_context` takes 4 args, not 5 ("this function takes 4 arguments but 5 arguments were supplied").

- [ ] **Step 3: Add the `content` parameter to `expand_context`**

Replace the `expand_context` function in `crates/hector-core/src/engine/context.rs` with:

```rust
/// Returns (primary, secondary) text for the LLM.
/// - `Diff`: primary = diff, no secondary.
/// - `File`: primary = authoritative `content` when supplied, else the file
///   read from disk; no secondary.
/// - `Repo`: primary = same as `File`; secondary = a deferral note.
///
/// `content` is the authoritative bytes the caller already holds (a PreToolUse
/// `--content` payload or a successful diff-mode disk read). When present it is
/// preferred over a disk read, so `context: file`/`repo` rules evaluate the
/// proposed edit even before it lands on disk. When `None`, File/Repo read the
/// anchor file from disk as before.
pub fn expand_context(
    scope: ContextScope,
    diff: Option<&str>,
    file: Option<&Path>,
    content: Option<&str>,
    _cwd: &Path,
) -> Result<(String, Option<String>)> {
    match scope {
        ContextScope::Diff => {
            let d = diff.ok_or_else(|| anyhow!("context: diff but no diff provided"))?;
            Ok((d.to_string(), None))
        }
        ContextScope::File => {
            let body = file_body(content, file)?;
            Ok((body, None))
        }
        ContextScope::Repo => {
            let body = file_body(content, file)?;
            Ok((
                body,
                Some("(repo-context expansion deferred; using file content only)".to_string()),
            ))
        }
    }
}

/// Resolve File/Repo primary text: prefer authoritative `content`, else read
/// the anchor `file` from disk.
fn file_body(content: Option<&str>, file: Option<&Path>) -> Result<String> {
    if let Some(c) = content {
        return Ok(c.to_string());
    }
    let p = file.ok_or_else(|| anyhow!("context: file but no file or content provided"))?;
    Ok(std::fs::read_to_string(p)?)
}
```

- [ ] **Step 4: Run the context tests to verify they pass**

Run: `cargo test -p hector-core --test context_expansion`
Expected: the three new tests PASS; the existing `file_scope_errors_when_file_is_missing` / `file_scope_surfaces_read_error_for_nonexistent_path` still PASS (they pass `content: None`).

- [ ] **Step 5: Update the semantic engine to pass authoritative content**

In `crates/hector-core/src/engine/semantic.rs`, change the `expand_context` call (line ~13) from:

```rust
        let (primary, context_text) = expand_context(scope, ctx.diff, Some(ctx.file), ctx.cwd)?;
```

to:

```rust
        let (primary, context_text) =
            expand_context(scope, ctx.diff, Some(ctx.file), ctx.content, ctx.cwd)?;
```

- [ ] **Step 6: Update the two runner call sites**

In `crates/hector-core/src/runner.rs`:

(a) `render_semantic_prompts` (~line 1496) — the prompt-preview path has no separate authoritative content, so pass `None` to preserve disk-read behavior. Change:

```rust
            let (primary, context_text) = crate::engine::context::expand_context(
                scope,
                if diff.is_empty() { None } else { Some(&diff) },
                Some(&path),
                &self.config_dir,
            )?;
```

to:

```rust
            let (primary, context_text) = crate::engine::context::expand_context(
                scope,
                if diff.is_empty() { None } else { Some(&diff) },
                Some(&path),
                None,
                &self.config_dir,
            )?;
```

(b) Thread authoritative content into the deferred path so the subagent envelope matches the direct-API evidence. Change `expand_deferred_contexts` (~line 1435) to accept content:

```rust
    fn expand_deferred_contexts<'a>(
        &'a self,
        deferred_rules: &'a [crate::verdict_deferred::DeferredRule],
        path: &Path,
        diff: &str,
        content: Option<&str>,
    ) -> DeferredExpansion<'a> {
```

and inside its loop change the `expand_context` call from:

```rust
            let expansion = crate::engine::context::expand_context(
                scope,
                if diff.is_empty() { None } else { Some(diff) },
                Some(path),
                &self.config_dir,
            );
```

to:

```rust
            let expansion = crate::engine::context::expand_context(
                scope,
                if diff.is_empty() { None } else { Some(diff) },
                Some(path),
                content,
                &self.config_dir,
            );
```

Then change `build_deferred_evaluator_input` (~line 1221) to accept and forward `content`:

```rust
    fn build_deferred_evaluator_input(
        &self,
        deferred_rules: &[crate::verdict_deferred::DeferredRule],
        path: &Path,
        diff: &str,
        content: Option<&str>,
        violations: &mut Vec<Violation>,
    ) -> Option<String> {
        let expansion = self.expand_deferred_contexts(deferred_rules, path, diff, content);
```

Finally, at the call site in `check_inner` (~line 1341), pass the same authoritative-content view the rule loop uses. Change:

```rust
        let evaluator_input =
            self.build_deferred_evaluator_input(&deferred, &path, &diff, &mut dispatch.violations);
```

to:

```rust
        let evaluator_input = self.build_deferred_evaluator_input(
            &deferred,
            &path,
            &diff,
            inputs.content,
            &mut dispatch.violations,
        );
```

> `inputs.content` is the `Option<&str>` already computed in `check_inner` (authoritative-or-`None`), so this reuses the exact same precedence the dispatch path uses — no new logic.

- [ ] **Step 7: Run the full core suite (parity + deferred tests must stay green)**

Run: `cargo test -p hector-core`
Expected: PASS, including `runner_deferred_context_parity`, `runner_deferred_mode`, `semantic_engine`, and `context_expansion`.

- [ ] **Step 8: Verify the Definition of done, then commit**

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -p hector-core
rm -rf target/llvm-cov-target target/llvm-cov
git add crates/hector-core/src/engine/context.rs crates/hector-core/src/engine/semantic.rs crates/hector-core/src/runner.rs crates/hector-core/tests/context_expansion.rs
git commit -m "fix(context): honor authoritative content in file/repo scope

expand_context now prefers caller-supplied content over a disk read, so
context: file/repo semantic rules evaluate the proposed PreToolUse edit
instead of stale on-disk bytes. Threaded through the semantic engine and
the deferred-envelope path to keep direct-API and subagent evidence
byte-identical.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Harden `parse_verdicts` against prose-wrapped JSON

**Why:** `parse_verdicts` slices from the first `[` to the last `]`. If the model wraps its array in prose containing incidental brackets ("I found [2] issues: [{…}] — done"), the slice spans the wrong region and fails to parse → engine error. Fix: scan each `[` as a candidate array start, take its balanced span, and return the first span that deserializes into the verdict shape.

**Files:**
- Modify: `crates/hector-core/src/llm/mod.rs` (`parse_verdicts` ~lines 228-258; add `balanced_array_span` + `extract_wire_verdicts` helpers; add tests to the existing `#[cfg(test)] mod redact_tests` or a new test module)

- [ ] **Step 1: Write the failing test**

In `crates/hector-core/src/llm/mod.rs`, add a test module at the end of the file (after `redact_tests`):

```rust
#[cfg(test)]
mod parse_verdict_tests {
    use super::{parse_verdicts, RuleStatus};

    #[test]
    fn extracts_array_amid_prose_with_incidental_brackets() {
        let text = "I reviewed the changes [see the 2 notes] and conclude: \
                    [{\"rule_id\":\"r1\",\"status\":\"pass\"}] — all done [end]";
        let v = parse_verdicts(text).expect("must skip prose brackets and parse the real array");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule_id, "r1");
        assert_eq!(v[0].status, RuleStatus::Pass);
    }

    #[test]
    fn skips_non_verdict_array_before_the_real_one() {
        let text = "scores: [1, 2, 3]. verdict: \
                    [{\"rule_id\":\"r2\",\"status\":\"violation\",\"message\":\"nope\",\"line\":4}]";
        let v = parse_verdicts(text).expect("must skip [1,2,3] and find the verdict array");
        assert_eq!(v.len(), 1);
        match &v[0].status {
            RuleStatus::Violation { message, line } => {
                assert_eq!(message, "nope");
                assert_eq!(*line, Some(4));
            }
            _ => panic!("expected violation"),
        }
    }

    #[test]
    fn plain_array_still_parses() {
        let v = parse_verdicts("[{\"rule_id\":\"r1\",\"status\":\"pass\"}]").unwrap();
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn no_array_is_an_error() {
        let err = parse_verdicts("the model refused to answer").expect_err("no array");
        assert!(format!("{err:#}").to_lowercase().contains("json"));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p hector-core extracts_array_amid_prose_with_incidental_brackets`
Expected: FAIL — the current first-`[`/last-`]` slice produces invalid JSON (`[see the 2 notes] and conclude: [{...}] — all done [end]`) and `parse_verdicts` returns an error.

- [ ] **Step 3: Add the balanced-scan helpers and rewrite `parse_verdicts`**

In `crates/hector-core/src/llm/mod.rs`, replace the `parse_verdicts` function with:

```rust
/// Find the balanced `[...]` span starting at byte index `start` (which must
/// point at a `[`), respecting JSON string literals so brackets inside strings
/// don't affect depth. Returns `None` if the array never closes.
fn balanced_array_span(s: &str, start: usize) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        // start and i both index ASCII delimiters → valid bounds.
                        return Some(&s[start..=i]);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Pull the verdict array out of an LLM response that may wrap it in prose or
/// markdown. Tries each `[` as a candidate start and returns the first balanced
/// span that deserializes into the verdict shape — so incidental brackets like
/// `[see notes]` or `[1, 2, 3]` are skipped.
fn extract_wire_verdicts(text: &str) -> Result<Vec<WireVerdict>> {
    let trimmed = text.trim();
    let mut search_from = 0;
    while let Some(rel) = trimmed[search_from..].find('[') {
        let start = search_from + rel;
        if let Some(span) = balanced_array_span(trimmed, start) {
            if let Ok(wire) = serde_json::from_str::<Vec<WireVerdict>>(span) {
                return Ok(wire);
            }
        }
        search_from = start + 1;
    }
    Err(anyhow!("no JSON verdict array found in response: {trimmed}"))
}

/// Parse the LLM's JSON-array response into structured verdicts.
///
/// Tolerates prose or markdown fences around the array, and incidental
/// brackets before it, by scanning for the first balanced `[...]` that matches
/// the verdict shape.
pub fn parse_verdicts(text: &str) -> Result<Vec<RuleVerdict>> {
    let wire = extract_wire_verdicts(text)?;
    let mut out = Vec::with_capacity(wire.len());
    for w in wire {
        let status = match w.status.to_ascii_lowercase().as_str() {
            "pass" => RuleStatus::Pass,
            "violation" => RuleStatus::Violation {
                message: w.message.unwrap_or_default(),
                line: w.line,
            },
            other => bail!(
                "unknown LLM status `{other}` for rule `{}`; expected `pass` or `violation`",
                w.rule_id
            ),
        };
        out.push(RuleVerdict {
            rule_id: w.rule_id,
            status,
        });
    }
    Ok(out)
}
```

> The conversion loop (wire → `RuleVerdict`) is unchanged from the original — only the extraction is replaced. Confirm `anyhow::{anyhow, bail}` are already imported at the top of `llm/mod.rs` (they are: `use anyhow::{anyhow, bail, Context, Result};`).

- [ ] **Step 4: Run the new tests and the LLM suite**

Run: `cargo test -p hector-core --test anthropic && cargo test -p hector-core parse_verdict_tests`
Expected: PASS — including the existing `anthropic_returns_err_on_malformed_text_json` ("not a json array" has no `[` → error mentions "json").

- [ ] **Step 5: Verify the Definition of done, then commit**

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -p hector-core
rm -rf target/llvm-cov-target target/llvm-cov
git add crates/hector-core/src/llm/mod.rs
git commit -m "fix(llm): extract verdict array via balanced scan, not first/last bracket

parse_verdicts now scans each '[' for a balanced span that deserializes to
the verdict shape, so prose or markdown with incidental brackets around the
array no longer breaks parsing.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Bounded retry with backoff on LLM 429/5xx

**Why:** Both LLM clients send once; a transient 429 (rate limit) or 5xx becomes an engine error. Parallel rule dispatch fires multiple semantic calls concurrently, making 429s *more* likely under exactly the load that triggers them. Fix: a small generic `retry_with_backoff` helper (unit-testable, no real network) wired into both clients, retrying on 429/500/502/503/504.

**Files:**
- Modify: `crates/hector-core/src/llm/mod.rs` (add `retry_with_backoff`, `is_retryable_status`, `backoff_delay`, `MAX_LLM_RETRIES`; add unit tests)
- Modify: `crates/hector-core/src/llm/anthropic.rs` (wrap the send ~lines 84-91)
- Modify: `crates/hector-core/src/llm/openai_compat.rs` (wrap the send ~lines 84-88)
- Test: `crates/hector-core/tests/anthropic.rs` (append a 429→200 recovery test)

- [ ] **Step 1: Write the failing unit tests for the retry helper**

In `crates/hector-core/src/llm/mod.rs`, add a test module at the end of the file:

```rust
#[cfg(test)]
mod retry_tests {
    use super::{is_retryable_status, retry_with_backoff};
    use std::cell::Cell;

    #[test]
    fn retries_until_non_retryable_then_returns_success() {
        let calls = Cell::new(0);
        let out: Result<u16, ()> = retry_with_backoff(
            2,
            |r| matches!(r, Ok(429)),
            || {
                calls.set(calls.get() + 1);
                if calls.get() < 3 {
                    Ok(429)
                } else {
                    Ok(200)
                }
            },
            |_attempt| {},
        );
        assert_eq!(out, Ok(200));
        assert_eq!(calls.get(), 3, "1 initial + 2 retries");
    }

    #[test]
    fn gives_up_after_max_retries_returning_last_result() {
        let calls = Cell::new(0);
        let out: Result<u16, ()> = retry_with_backoff(
            2,
            |r| matches!(r, Ok(429)),
            || {
                calls.set(calls.get() + 1);
                Ok(429)
            },
            |_| {},
        );
        assert_eq!(out, Ok(429));
        assert_eq!(calls.get(), 3, "no attempts beyond max_retries");
    }

    #[test]
    fn does_not_retry_on_first_success() {
        let calls = Cell::new(0);
        let out: Result<u16, ()> = retry_with_backoff(
            2,
            |r| matches!(r, Ok(429)),
            || {
                calls.set(calls.get() + 1);
                Ok(200)
            },
            |_| {},
        );
        assert_eq!(out, Ok(200));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn retryable_status_set() {
        for code in [429, 500, 502, 503, 504] {
            assert!(is_retryable_status(code), "{code} should retry");
        }
        for code in [200, 400, 401, 403, 404, 422] {
            assert!(!is_retryable_status(code), "{code} should not retry");
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p hector-core retry_tests`
Expected: FAIL to compile — `retry_with_backoff` and `is_retryable_status` are not defined.

- [ ] **Step 3: Implement the retry helper, status predicate, and backoff**

In `crates/hector-core/src/llm/mod.rs`, add (place near the top, after the imports / `RuleVerdict` definitions):

```rust
/// Maximum retries after the initial attempt (total attempts = 1 + this).
pub(crate) const MAX_LLM_RETRIES: u32 = 2;

/// HTTP status codes worth retrying: rate limits and transient upstream errors.
pub(crate) fn is_retryable_status(code: u16) -> bool {
    matches!(code, 429 | 500 | 502 | 503 | 504)
}

/// Exponential backoff: 250ms, 500ms, … for `attempt` 1, 2, … (no jitter —
/// single process, low concurrency).
pub(crate) fn backoff_delay(attempt: u32) -> std::time::Duration {
    let factor = 2u64.saturating_pow(attempt.saturating_sub(1));
    std::time::Duration::from_millis(250u64.saturating_mul(factor))
}

/// Run `send`, retrying up to `max_retries` times while `is_retryable` holds,
/// invoking `on_retry(attempt)` (e.g. to sleep) between attempts. Generic over
/// the result type so the loop is unit-testable without a real network call.
pub(crate) fn retry_with_backoff<T, E>(
    max_retries: u32,
    is_retryable: impl Fn(&std::result::Result<T, E>) -> bool,
    mut send: impl FnMut() -> std::result::Result<T, E>,
    mut on_retry: impl FnMut(u32),
) -> std::result::Result<T, E> {
    let mut attempt = 0u32;
    loop {
        let result = send();
        if attempt < max_retries && is_retryable(&result) {
            attempt += 1;
            on_retry(attempt);
            continue;
        }
        return result;
    }
}
```

- [ ] **Step 4: Run the unit tests to verify they pass**

Run: `cargo test -p hector-core retry_tests`
Expected: PASS.

- [ ] **Step 5: Wire the retry into the Anthropic client**

In `crates/hector-core/src/llm/anthropic.rs`, replace the request-send block:

```rust
        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .context("anthropic request")?;
```

with:

```rust
        let response = super::retry_with_backoff(
            super::MAX_LLM_RETRIES,
            |r: &reqwest::Result<reqwest::blocking::Response>| {
                matches!(r, Ok(resp) if super::is_retryable_status(resp.status().as_u16()))
            },
            || {
                self.client
                    .post(&url)
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&body)
                    .send()
            },
            |attempt| std::thread::sleep(super::backoff_delay(attempt)),
        )
        .context("anthropic request")?;
```

- [ ] **Step 6: Wire the retry into the OpenAI-compat client**

In `crates/hector-core/src/llm/openai_compat.rs`, replace:

```rust
        let mut req = self.client.post(&url).json(&body);
        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }
        let response = req.send().context("openai-compat request")?;
```

with:

```rust
        let response = super::retry_with_backoff(
            super::MAX_LLM_RETRIES,
            |r: &reqwest::Result<reqwest::blocking::Response>| {
                matches!(r, Ok(resp) if super::is_retryable_status(resp.status().as_u16()))
            },
            || {
                let mut req = self.client.post(&url).json(&body);
                if !self.api_key.is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", self.api_key));
                }
                req.send()
            },
            |attempt| std::thread::sleep(super::backoff_delay(attempt)),
        )
        .context("openai-compat request")?;
```

- [ ] **Step 7: Write the integration test (429 then 200 recovery)**

Append to `crates/hector-core/tests/anthropic.rs`:

```rust
#[tokio::test]
async fn anthropic_retries_once_on_429_then_succeeds() {
    // First call rate-limited, second succeeds. The client must retry and
    // return the verdict rather than surfacing the 429 as an error.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "[{\"rule_id\":\"r1\",\"status\":\"pass\"}]" }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let base_url = server.uri();
    let rule = make_semantic_rule();
    let result = tokio::task::spawn_blocking(move || {
        let client = AnthropicClient::new("test-key", "claude-sonnet-4-6", Some(base_url));
        client.evaluate(&[("r1", &rule)], "diff text", None)
    })
    .await
    .unwrap();
    let verdicts = result.expect("retry should recover from the 429");
    assert_eq!(verdicts.len(), 1);
    // Both mocks' `.expect(1)` are verified when `server` drops at scope end.
}
```

> wiremock serves the `up_to_n_times(1)` 429 mock once, then the 200 mock matches. If the executor finds the matching order ambiguous on the installed wiremock version, the unit tests in Step 1 already prove the retry loop; adjust the integration test to a single stateful `Respond` impl returning 429-then-200. The real backoff sleeps ~250ms once, so this test adds well under a second.

- [ ] **Step 8: Run the LLM suite**

Run: `cargo test -p hector-core --test anthropic && cargo test -p hector-core --test openai_compat`
Expected: PASS, including the new retry test and the existing `anthropic_returns_err_on_http_500` (500 is retryable, so it now sends 3 times before surfacing the error — the mock in that test responds 500 to every call, so the final result is still an Err mentioning 500; confirm that test still passes, and if it asserts a request count, update its `.expect(...)` to 3).

- [ ] **Step 9: Verify the Definition of done, then commit**

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -p hector-core
rm -rf target/llvm-cov-target target/llvm-cov
git add crates/hector-core/src/llm/mod.rs crates/hector-core/src/llm/anthropic.rs crates/hector-core/src/llm/openai_compat.rs crates/hector-core/tests/anthropic.rs
git commit -m "feat(llm): retry transient 429/5xx with bounded backoff

Both clients now retry up to MAX_LLM_RETRIES on 429/500/502/503/504 with
exponential backoff, so semantic rules stop flaking under the parallel
dispatch that triggers rate limits. Retry loop is a generic, unit-tested
helper independent of reqwest.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Single source of truth for "which glob matched"

**Why:** The bare-pattern glob rule (a slashless `*.py` also registers `**/*.py`) — explicitly load-bearing per `CLAUDE.md` — lives in `ScopeMatcher`/`SkipMatcher` *and* is re-implemented in `runner.rs`'s `first_matching_scope_glob`/`first_matching_skip_glob` (used by `explain`/`guide` to report which glob matched). The duplicates can drift. Fix: have the matchers expose `matched_pattern`, backed by `GlobSet::matches`, and delete the runner duplicates.

**Files:**
- Modify: `crates/hector-core/src/config/scope.rs` (`ScopeMatcher`: track raw patterns + `matched_pattern`; add tests)
- Modify: `crates/hector-core/src/config/skip.rs` (`SkipMatcher`: track raw patterns + `matched_pattern`; add tests)
- Modify: `crates/hector-core/src/runner.rs` (`scope_outcomes` ~lines 536-568 uses the cached matchers; delete `first_matching_skip_glob` ~lines 371-403 and `first_matching_scope_glob` ~lines 408-426)

- [ ] **Step 1: Write the failing tests for `matched_pattern`**

In `crates/hector-core/src/config/scope.rs`, add a test module at the end:

```rust
#[cfg(test)]
mod matched_pattern_tests {
    use super::ScopeMatcher;
    use std::path::Path;

    #[test]
    fn reports_the_author_pattern_for_a_bare_glob_at_depth() {
        let m = ScopeMatcher::new(&["*.py".to_string()]).unwrap();
        // Bare *.py also matches at depth via the **/ form, but the reported
        // pattern must be the author's "*.py", not the synthesized "**/*.py".
        assert_eq!(m.matched_pattern(Path::new("src/app/main.py")), Some("*.py"));
        assert_eq!(m.matched_pattern(Path::new("main.py")), Some("*.py"));
    }

    #[test]
    fn returns_first_matching_pattern_in_declaration_order() {
        let m = ScopeMatcher::new(&["src/**".to_string(), "*.rs".to_string()]).unwrap();
        // src/lib.rs matches both; declaration order wins.
        assert_eq!(m.matched_pattern(Path::new("src/lib.rs")), Some("src/**"));
    }

    #[test]
    fn returns_none_when_nothing_matches() {
        let m = ScopeMatcher::new(&["*.py".to_string()]).unwrap();
        assert_eq!(m.matched_pattern(Path::new("README.md")), None);
    }
}
```

And in `crates/hector-core/src/config/skip.rs`, add to the existing `mod tests`:

```rust
    #[test]
    fn matched_pattern_reports_author_glob() {
        let m = SkipMatcher::with_built_ins(&["fixtures/**".into()]).unwrap();
        assert_eq!(m.matched_pattern(Path::new("Cargo.lock")), Some("Cargo.lock"));
        assert_eq!(
            m.matched_pattern(Path::new("crates/x/Cargo.lock")),
            Some("Cargo.lock")
        );
        assert_eq!(
            m.matched_pattern(Path::new("fixtures/large.json")),
            Some("fixtures/**")
        );
        assert_eq!(m.matched_pattern(Path::new("src/main.rs")), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p hector-core matched_pattern`
Expected: FAIL to compile — `matched_pattern` does not exist on `ScopeMatcher`/`SkipMatcher`.

- [ ] **Step 3: Add pattern tracking + `matched_pattern` to `ScopeMatcher`**

Replace `crates/hector-core/src/config/scope.rs`'s `ScopeMatcher` struct and `impl` with:

```rust
#[derive(Clone)]
pub struct ScopeMatcher {
    set: GlobSet,
    /// Raw author pattern for each glob added to `set`, in insertion order. A
    /// bare pattern adds two entries (itself + `**/<pattern>`) that both map to
    /// the same author string, so `matched_pattern` reports what the author
    /// wrote, not the synthesized form.
    patterns: Vec<String>,
}

impl ScopeMatcher {
    pub fn new(globs: &[String]) -> Result<Self> {
        let mut b = GlobSetBuilder::new();
        let mut patterns = Vec::new();
        for g in globs {
            // Bully's matcher is right-anchored: bare `*.py` should match at any depth.
            // globset treats `*.py` as matching `.py` at any depth iff the input
            // is the filename only. We pre-compute by also adding `**/<pattern>`
            // when the pattern has no slash.
            let glob = Glob::new(g).with_context(|| format!("invalid glob: {g}"))?;
            b.add(glob);
            patterns.push(g.clone());
            if !g.contains('/') {
                let prefixed = format!("**/{}", g);
                let glob =
                    Glob::new(&prefixed).with_context(|| format!("invalid glob: {prefixed}"))?;
                b.add(glob);
                patterns.push(g.clone());
            }
        }
        Ok(Self {
            set: b.build()?,
            patterns,
        })
    }

    pub fn matches<P: AsRef<Path>>(&self, path: P) -> bool {
        self.set.is_match(path.as_ref())
    }

    /// The first author-authored pattern (in declaration order) that matches
    /// `path`, or `None`. Single source of truth for "which glob matched",
    /// replacing the runner's hand-rolled re-implementation.
    pub fn matched_pattern<P: AsRef<Path>>(&self, path: P) -> Option<&str> {
        self.set
            .matches(path.as_ref())
            .into_iter()
            .min()
            .map(|i| self.patterns[i].as_str())
    }
}
```

- [ ] **Step 4: Add pattern tracking + `matched_pattern` to `SkipMatcher`**

In `crates/hector-core/src/config/skip.rs`, change the struct to carry patterns and have `add_glob` record them. Replace the `SkipMatcher` struct, its `impl`, and `add_glob` with:

```rust
pub struct SkipMatcher {
    set: GlobSet,
    /// Raw pattern for each glob added, in insertion order (see ScopeMatcher).
    patterns: Vec<String>,
}

impl SkipMatcher {
    /// Build a matcher from the built-in patterns plus any extras the caller
    /// provides (project `skip:` list, `~/.hector-ignore` entries, etc.).
    pub fn with_built_ins(extras: &[String]) -> Result<Self> {
        let mut b = GlobSetBuilder::new();
        let mut patterns = Vec::new();
        for g in built_in_skip_globs() {
            add_glob(&mut b, g, &mut patterns)?;
        }
        for g in extras {
            add_glob(&mut b, g, &mut patterns)?;
        }
        Ok(Self {
            set: b.build()?,
            patterns,
        })
    }

    pub fn matches<P: AsRef<Path>>(&self, path: P) -> bool {
        self.set.is_match(path.as_ref())
    }

    /// The first skip pattern (in construction order: built-ins then extras)
    /// that matches `path`, or `None`. Single source of truth for "which skip
    /// glob matched".
    pub fn matched_pattern<P: AsRef<Path>>(&self, path: P) -> Option<&str> {
        self.set
            .matches(path.as_ref())
            .into_iter()
            .min()
            .map(|i| self.patterns[i].as_str())
    }
}

fn add_glob(b: &mut GlobSetBuilder, raw: &str, patterns: &mut Vec<String>) -> Result<()> {
    let glob = Glob::new(raw).with_context(|| format!("invalid skip glob: {raw}"))?;
    b.add(glob);
    patterns.push(raw.to_string());
    if !raw.contains('/') {
        let prefixed = format!("**/{raw}");
        let glob =
            Glob::new(&prefixed).with_context(|| format!("invalid skip glob: {prefixed}"))?;
        b.add(glob);
        patterns.push(raw.to_string());
    } else if let Some(prefix) = raw.strip_suffix("/**") {
        // `node_modules/**` should also match `packages/web/node_modules/bar.js`.
        // Mirrors bully's path-component check for `/**`-suffixed patterns.
        if !prefix.is_empty() && !prefix.contains('*') {
            let any_depth = format!("**/{prefix}/**");
            let glob =
                Glob::new(&any_depth).with_context(|| format!("invalid skip glob: {any_depth}"))?;
            b.add(glob);
            patterns.push(raw.to_string());
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Run the matcher tests**

Run: `cargo test -p hector-core matched_pattern && cargo test -p hector-core --test scope`
Expected: PASS.

- [ ] **Step 6: Rewrite `scope_outcomes` to use the matchers and delete the runner duplicates**

In `crates/hector-core/src/runner.rs`, replace the body of `scope_outcomes` (the function ~lines 536-568) with the version below. It uses the load-time `self.skip` matcher (built with the same project+user-global extras) and the cached per-rule `scope_matchers`, dropping the per-call home-dir read and the hand-rolled glob walks:

```rust
    pub fn scope_outcomes(&self, file: &std::path::Path) -> ScopeOutcomes {
        let match_path = relativize(file, &self.config_dir);

        // The load-time skip matcher already unions built-ins + project skip +
        // user-global ignore, so it is the single source of truth here too.
        let skip = self
            .skip
            .matched_pattern(&match_path)
            .map(|pattern| SkipHit {
                pattern: pattern.to_string(),
            });

        let mut rules: Vec<RuleScopeEntry> = Vec::with_capacity(self.config.rules.len());
        for (rule_id, rule) in &self.config.rules {
            let matched = self
                .scope_matchers
                .get(rule_id)
                .and_then(|m| m.matched_pattern(&match_path).map(str::to_string));
            let scope_match = match matched {
                Some(glob) => ScopeMatch::Match { glob },
                None => ScopeMatch::NoMatch {
                    scopes: rule.scope.clone(),
                },
            };
            rules.push(RuleScopeEntry {
                rule_id: rule_id.clone(),
                engine: rule.engine,
                severity: rule.severity,
                description: rule.description.clone(),
                scope_match,
            });
        }
        ScopeOutcomes { skip, rules }
    }
```

Then **delete** the now-unused free functions `first_matching_skip_glob` (~lines 371-403) and `first_matching_scope_glob` (~lines 408-426). Remove any now-unused imports they introduced (e.g. the `use crate::config::skip::{parse_user_global_ignore, ...}` items used only by the deleted skip walk — let `cargo clippy -D warnings` flag unused imports and remove exactly those it reports; keep `SkipMatcher` and `USER_GLOBAL_IGNORE_FILENAME` if still used by `load_with`).

- [ ] **Step 7: Run the scope-outcomes integration tests**

Run: `cargo test -p hector-core --test runner_scope_outcomes && cargo test -p hector-core --test runner_skip`
Expected: PASS — `explain`/`guide` report the same matched globs as before.

- [ ] **Step 8: Verify the Definition of done, then commit**

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -p hector-core
rm -rf target/llvm-cov-target target/llvm-cov
git add crates/hector-core/src/config/scope.rs crates/hector-core/src/config/skip.rs crates/hector-core/src/runner.rs
git commit -m "refactor(scope): expose matched_pattern, drop runner's glob duplicates

ScopeMatcher/SkipMatcher now report which author pattern matched, backed by
GlobSet::matches. scope_outcomes uses them and the load-time skip matcher,
deleting the two hand-rolled glob walks in runner.rs that re-encoded the
load-bearing bare-pattern semantics and could drift.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Deferred (out of scope for this plan)

These review findings are intentionally **not** in this plan, with rationale. Recommend a separate plan for the first one.

- **`runner.rs` decomposition (review #5).** The 1,841-LOC orchestrator (health 2.5, 35 dependents) should be split into `runner/paths.rs`, `runner/deferred.rs`, `runner/explain.rs`, and the two session paths unified. This is a large, no-new-behavior structural refactor guarded by the existing suite — it does not fit the TDD "failing test first" shape and would dominate this plan. **Recommend its own plan** (a pure module-extraction task list), executed after Tasks 1-6 land so it rebases onto the smaller `runner.rs` Task 6 produces.
- **Unify the two `extends` DFS walkers (review #7).** `resolve_inner` vs `resolve_inner_with_origin` duplicate the cycle-detect/parse/merge walk. Worth doing, but it's a refactor with no behavior change and lower blast radius than #5 — fold into the runner-decomposition plan or a separate cleanup pass.
- **Performance items (review #9/#10/#11).** Reuse the rayon pool across checks; cache the baseline load; relativize session edit paths once. **No `benches/` exists**, so per the rust-development performance guidance these need a criterion baseline before/after to justify the change — measure first, then plan. Do not implement blind.
- **`comment_markers_for` dead arms + `#[non_exhaustive]` (review #8/#12).** `makefile`/`dockerfile`/`gitignore` listed as extensions never match (`Path::extension()` returns `None` for those filenames); and `Engine`/config structs could take `#[non_exhaustive]`. Both are trivial, low-value hygiene — batch into an opportunistic cleanup commit, not a standalone task.

---

## Self-Review

**1. Spec coverage** — every actionable review finding maps to a task or the deferred list:
- #1 clone deadlock → Task 2. #2 pipe-buffer/MAX_OUTPUT → Task 1. #3 stale content → Task 3. #4 parse_verdicts → Task 4. #13 LLM retry → Task 5. #6 glob duplication → Task 6. #5/#7/#8/#9/#10/#11/#12 → Deferred (with rationale).

**2. Placeholder scan** — no "TBD"/"add error handling"/"similar to Task N"; every code step shows complete code; every command shows expected output.

**3. Type consistency** — verified across tasks:
- `expand_context` signature `(ContextScope, Option<&str> diff, Option<&Path> file, Option<&str> content, &Path cwd)` is used identically in Task 3's test, `semantic.rs`, `render_semantic_prompts`, and `expand_deferred_contexts`.
- `retry_with_backoff(max_retries, is_retryable, send, on_retry)`, `is_retryable_status(u16)`, `backoff_delay(u32)`, `MAX_LLM_RETRIES` — defined in Task 5 Step 3, consumed unchanged in Steps 5/6 and the unit tests.
- `matched_pattern<P: AsRef<Path>>(&self, P) -> Option<&str>` — identical on `ScopeMatcher` and `SkipMatcher`, consumed in `scope_outcomes`.
- `spawn_reader`/`join_reader` — defined once in Task 1, reused by both `spawn_with_timeout` and `wait_for_child`.
- `child_exec(stdout_w_raw, stderr_w_raw, cwd: &CStr, sh_path: &CStr, argv: &[CString], envp: &[CString])` — signature in Task 2 Step 4 matches the closure call in Step 3.

Verified facts the plan depends on: `nix`'s `sched`+`signal` features (both enabled in the workspace) transitively enable `process`, so `execve`/`chdir` need no `Cargo.toml` change; `GlobSet::matches` returns `Vec<usize>` of matched indices; `wiremock`/`tempfile`/`tokio` are dev-deps; `run_with_capabilities` and `run_with_capabilities_env` are both `pub`.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-28-hector-core-resilience-hardening.md`. Two execution options:

**1. Subagent-Driven (recommended)** — dispatch a fresh Opus 4.8 subagent per task, review between tasks, fast iteration. Tasks 1→2 share `capability.rs` (run sequentially; Task 2's subagent re-reads the file after Task 1 commits). Tasks 3, 4+5, and 6 touch disjoint files and could be parallelized across worktrees if desired, but sequential review is simpler.

**2. Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints for review.

**Which approach?**
