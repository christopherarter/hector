# Launch-readiness & maintainability audit — 2026-05-29

Scope: is IronLint ready to share on Hacker News, and what threatens long-term
maintenance? Method: 8-dimension fan-out (security, resilience, maintainability,
API stability, docs/DX, adapters, CI/release, test quality), every material
finding adversarially re-verified against the source, then synthesized. Ground
truth at audit time: `cargo clippy --all-targets -D warnings` **clean** and the
full `cargo test` suite **green** (exit 0). 43 findings survived verification
(2 blocker, 10 high, 23 medium, 8 low); refuted/overstated ones were dropped or
downgraded and are not listed.

The engineering is genuinely above the bar for a 0.2 solo project. The problems
below are mostly small and well-scoped — but two of them undercut the core
promise and the first-impression, so they matter before launch.

---

## Launch blockers

### 1. The gate fails **open** on a panic — and bypasses the fail-closed switch
`crates/ironlint-cli/src/main.rs` installs no `std::panic::set_hook`, so any panic
unwinds to process exit **101**. `crates/ironlint-core/src/runner.rs:669` is a
live panic site: `ThreadPoolBuilder::build().expect("rayon pool construction
must not fail")` — `build()` is fallible (OS thread-spawn exhaustion). The
Claude Code adapter's post-tool-use handler
(`adapters/claude-code/hooks/hook.sh:251-255`) routes any unrecognized exit code
through the `*)` arm, which prints to stderr and `exit 1` → Claude Code treats a
hook exit 1 as non-blocking → **the edit is allowed.**

The sharp part: the clean internal-error path (`3)`,
`hook.sh:239-249`) honors `IRONLINT_FAIL_CLOSED_ON_INTERNAL=1`, but the `*)` arm
does **not**. So a user who explicitly opted into strict gating still gets
fail-**open** when the binary panics. For a policy-*enforcement* tool, "crashes
silently disable enforcement, even in fail-closed mode" is the worst failure
mode, and it's exactly the one left unguarded.

Fix (two-pronged, both small):
- Core: `std::panic::set_hook(Box::new(|_| std::process::exit(3)))` in `main`
  (or `catch_unwind` around the dispatch and emit a real `InternalError`
  verdict). Routes panics into the `3)` arm that honors fail-closed. Add a CI
  test asserting panic → exit 3.
- Core: make `runner.rs:669` return `Result` and surface as
  `Status::InternalError` rather than `.expect`.
- Adapter (defense in depth): have the `*)` arm also respect
  `IRONLINT_FAIL_CLOSED_ON_INTERNAL`.

### 2. No way to try it without a 10-minute source build
README (`README.md:19-24`) and `docs/getting-started.md` offer only
`cargo build --release`. CI uploads binaries with **1-day retention**
(`.github/workflows/ci.yml:47-52`) — no GitHub Release despite existing tags, no
`cargo install` (crates aren't publishable: `crates/ironlint-cli/Cargo.toml` and
`crates/ironlint-core/Cargo.toml` lack the mandatory `description` field and
others), no Homebrew/install script. The HN "let me try it right now" impulse
dies at a cold Rust compile. Add a tag-triggered release workflow shipping
prebuilt linux + macOS binaries; add crates.io metadata.

### 3. No LICENSE file
`Cargo.toml:8` declares `license = "Apache-2.0"`, but there is **no LICENSE /
COPYING file** at the repo root. GitHub renders "No license detected"; cautious
users and OSPO teams won't touch it. Drop in the Apache-2.0 text. (effort: S)

---

## Fix before HN (high-value polish)

- **macOS capability enforcement is a silent no-op.**
  `crates/ironlint-core/src/engine/capability.rs:387` —
  `run_best_effort_macos(cmd, cwd, _caps, env)` ignores `_caps` entirely. A
  macOS user who writes `network: false` on a script rule believes the script
  can't phone home; it can. Only `ironlint doctor` warns — `ironlint check` is
  silent. Given how many HN readers are on macOS, this is a credibility risk.
  Emit a one-time `AtomicBool`-gated stderr warning on first capability use
  during `check` (mirror the existing unprivileged-user warning at
  `capability.rs:125`), and state the platform matrix plainly in
  `docs/security/capabilities.md`.
- **README has no hook and no example rule.** `README.md:1-3` opens with
  "Policy-enforcement pipeline…" jargon and zero scenario. A reader can't see
  what a *rule* even looks like without clicking through. Rewrite the opener
  around the concrete DEBUG-marker narrative that already exists in
  `docs/adapters/claude-code.md:37-51`, and embed one 3-line YAML rule.
- **pi & reasonix adapters are fully built + CI-tested but undocumented.**
  `README.md:7,17` advertises only claude-code + opencode as shipped and lists
  the rest as "planned" — yet `adapters/pi/` (353-line TS impl) and
  `adapters/reasonix/` (155-line hook) are complete. Discovering working,
  undocumented adapters reads as weak QC. Either claim them or mark them
  experimental.
- **Hardcoded absolute path in a shipped example.**
  `adapters/reasonix/hooks/settings.example.json:5` points at
  `/Users/chrisarter/Documents/projects/ironlint/...`. Anyone who copies it gets a
  broken hook that silently no-ops. Replace with a `<INSTALL_PATH>` placeholder.
- **Hooks don't validate `jq`.** `adapters/claude-code/hooks/hook.sh:135` and
  `adapters/reasonix/hooks/hook.sh` assume `jq` exists. Missing → exit 127 →
  `*)` arm → silent disablement (same class as blocker #1). Add a
  `command -v jq` preflight that fails loudly.

---

## Long-term maintenance

- **`runner.rs` (1787 lines) — the debt is duplication, not length.**
  `check_inner` (`runner.rs:1211-1308`) tangles 8+ concerns, and rule
  orchestration is duplicated between `check_inner` and `check_session`
  (`1578-1594`). Adding a 5th engine means edits in 2-3 places. Extract a shared
  orchestration helper.
- **Session rules use a split dispatch surface.** `evaluate_one_rule` skips them
  (`runner.rs:741-743`) while `check_session` evaluates separately
  (`1567-1623`) with its own scope filtering + telemetry. Make `Session` a
  first-class `RuleEngine` so dispatch stops being scattered.
- **Latent correctness bug: deferred context expansion diverges.** `check_inner`
  calls `expand_deferred_contexts`, but `check_session_with_options` hardcodes
  `None` for context (`runner.rs:1734`). A session rule with `context: repo`
  silently omits evidence in deferred mode — violating the "byte-identical
  evidence" invariant — and no test catches it. Add a test + share the path.
- **Forward-compat is asserted by comments, not tests, before the 0.3 freeze.**
  `verdict_snapshot.rs` only serializes; nothing deserializes a `Verdict` with
  an unknown future field. `SessionState` (`session_state.rs`) has no
  `schema_version` while `Baseline` and `Telemetry` both do. Add a forward-compat
  deserialization test now.
- **Subprocess orphaning on timeout.** `capability.rs:323-370` kills only the
  immediate `sh` child; backgrounded grandchildren reparent to init and leak
  PIDs/FDs across repeated checks. The timeout test asserts wall-clock return,
  not process cleanup.
- **Test coverage holes in hot paths.** `apply_baseline` has no end-to-end test
  mixing matched + new violations (`runner.rs:1129-1157`); `resolve_check_input`
  adverse branches untested (`945-999`); parallel dispatch has no panic/failure
  injection (`runner_parallel.rs`). Logic looks correct, but regressions would
  be invisible — and the coverage gate can be satisfied by easy files while hot
  logic stays thin.
- **Supply-chain hygiene.** `Cargo.lock` is gitignored (`.gitignore:3`) — fine-ish
  since workspace deps are version-pinned, but for a shipping security binary,
  commit it for reproducible `cargo install --git`. GitHub Actions use floating
  `@v4`/`@stable` tags throughout `ci.yml` — pin to commit SHAs.
- **`lib.rs` exports 12 modules with no stability tiers** (`lib.rs:1-17`).
  Library users already import deep internals (`ironlint_core::engine::ast::…`)
  that will break silently on refactor. Document stable-vs-internal before
  adoption grows.
- **Trust YAML-anchor guard has known false positives** (`trust.rs:35-88`):
  unquoted shell `&`, escaped quotes in double-quoted strings. Safe (conservative
  reject), but it'll frustrate operators with valid configs and no workaround in
  the error text. Improve the message; add tests for the two known cases.

---

## What's genuinely solid (credit where due)

- **Exit-code contract is real, documented, and respected in code**
  (`check.rs:309-316`, with a `#[non_exhaustive]`-aware fail-open default). The
  *only* gap is panics, which sit outside the map (blocker #1).
- **Trust gate is a genuine security primitive** — canonical-YAML sha256, keys
  sorted, `trust:` stripped, conservative anchor/alias guard — and honestly
  scoped (capabilities are accident-protection, not adversarial).
- **Test discipline above solo-project norms** — 106 test files, wiremock-backed
  LLM HTTP tests, insta snapshots locking verdict shape, CI per-file ≥80%
  region-coverage gate, clippy cognitive-complexity cap of 15.
- **Honest documentation of limitations** — pre-edit script-rule semantics,
  macOS advisory capabilities, Windows-unsupported isolation are all documented
  correctly. No overclaiming. (The macOS gap above is a *runtime-silence*
  problem, not a docs-lie problem.)
- **Additive-schema discipline works in practice** — `deferred_rules` added at v2
  via `skip_serializing_if` without a needless `SCHEMA_VERSION` bump; baseline/
  telemetry use versioned variants.

---

## Suggested order

1. Panic → fail-open fix (blocker #1) + CI test. Smallest change, biggest
   correctness win, and it's the one a sharp HN commenter will find.
2. LICENSE file (5 min).
3. Release workflow + crates.io metadata + binary downloads.
4. README rewrite with a hook + example rule.
5. macOS capability warning; jq preflight; reasonix hardcoded path; adapter docs.
6. Long-term items as capacity allows — start with the deferred-context
   correctness bug and the forward-compat tests (both are pre-0.3-freeze).
