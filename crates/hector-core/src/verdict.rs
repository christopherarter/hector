use serde::{Deserialize, Serialize};

/// Verdict JSON schema version.
///
/// **Policy (C6, 2026-05-25):**
/// `SCHEMA_VERSION` bumps ONLY on:
/// - field removals or type changes,
/// - enum variant removals,
/// - semantic re-interpretations of existing fields.
///
/// Additive changes (new optional field with `skip_serializing_if`,
/// new enum variant marked `#[non_exhaustive]`) do NOT bump.
/// Consumers wanting backward compatibility should read
/// `MIN_REQUIRED_SCHEMA_VERSION` and accept anything `>=`.
///
/// History:
/// - v1: initial 0.1 shape with five `Engine` variants (`Script`, `Ast`,
///   `Semantic`, `Session`, `Trust`).
/// - v2 (P1-1): split overloaded `Engine::Trust` into `Engine::Trust`
///   (true trust-gate failures) and `Engine::Internal` (engine runtime
///   errors). Wire format for the new variant is `"internal"`.
/// - v2 retained (R6, 2026-05-23): added optional `deferred_rules:
///   [{rule_id, severity, reason}]` field with
///   `skip_serializing_if = "Vec::is_empty"`. Additive — verdicts
///   without deferred rules are byte-compatible with the original v2
///   shape. SCHEMA_VERSION was briefly bumped to 3 and reverted here
///   (C6, 2026-05-25) because strict consumers rejected every new
///   verdict despite zero wire-format change.
pub const SCHEMA_VERSION: u32 = 2;

/// Floor schema version that all current verdicts satisfy.
///
/// Consumers should assert `schema_version >= MIN_REQUIRED_SCHEMA_VERSION`
/// rather than `schema_version == <constant>`, so they remain compatible
/// with future additive bumps.
pub const MIN_REQUIRED_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub schema_version: u32,
    pub hector_version: String,
    pub status: Status,
    pub violations: Vec<Violation>,
    pub passed_checks: Vec<String>,
    pub elapsed_ms: u64,
    /// R6 (2026-05-23): semantic/session rules whose evaluation was
    /// suppressed because a deterministic rule fired with `severity:
    /// error` on the same edit. Empty in all non-deferred-mode flows
    /// (and serialized as omitted via `skip_serializing_if`), so
    /// existing v2 consumers see no wire-format change.
    ///
    /// Populated only when `CheckOptions::emit_semantic_payload` is true
    /// AND the resulting `Verdict::status` is `Block`. The full deferred
    /// envelope is suppressed in that case — these rule refs are the
    /// surfacing mechanism so the user (and the Claude Code skill) know
    /// their semantic rules are alive even when not evaluated this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deferred_rules: Vec<DeferredRuleRef>,
}

/// R6 (2026-05-23): minimal reference to a deferred rule whose
/// evaluation was suppressed by a deterministic block.
///
/// Lives on `Verdict` (not `DeferredVerdict`) because it surfaces in the
/// deterministic-block path where the full deferred envelope is dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeferredRuleRef {
    pub rule_id: String,
    pub severity: Severity,
    /// Human-readable string explaining why the rule was not evaluated.
    /// Stable enough for adapter skills to surface verbatim; not enum'd
    /// because the only consumer today is the Claude Code interpreter
    /// skill, which renders it as plain text.
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Status {
    Pass,
    Warn,
    Block,
    /// B7 (2026-05-25): at least one rule failed to evaluate due to an
    /// engine-internal error (LLM unavailable, AST refused diff, script
    /// spawn failure). Surfaces in `Violation::engine = Internal` rows.
    /// CLI maps to exit code 3 so adapters can distinguish "config
    /// wrong" from "policy violated" (exit 2).
    #[serde(rename = "internal_error")]
    InternalError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    pub rule_id: String,
    pub severity: Severity,
    pub engine: Engine,
    pub file: String,
    pub line: Option<u32>,
    /// 1-based column of the violation's start position.
    ///
    /// P2-19 / P1-3: only the AST engine populates this — it reads the
    /// column from the matched node's start byte. The `script`,
    /// `semantic`, and `session` engines have no positional information
    /// from a regex/LLM hit and always leave this `None`.
    pub column: Option<u32>,
    pub message: String,
    pub suggestion: Option<String>,
    /// Snippet of source surrounding the violation.
    ///
    /// P2-19 / P1-3: AST populates this with the matched node's line
    /// ±3 lines for editor display. Script, semantic, and session
    /// engines leave it `None`.
    pub context: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Script,
    Ast,
    Semantic,
    Session,
    /// True trust-gate failure: config fingerprint mismatch.
    ///
    /// In practice trust failures halt at `HectorEngine::load`, so this
    /// variant is rarely seen in a `Violation`. Reserved for the case
    /// where a downstream caller wants to surface a trust-rejection as a
    /// structured verdict instead of a load error.
    Trust,
    /// Engine-internal runtime error (LLM unavailable, AST refused diff,
    /// script spawn failure, etc.). The rule's `rule_id` is suffixed with
    /// `__internal` by the runner so consumers can distinguish runtime
    /// errors from rule violations.
    Internal,
}

impl Verdict {
    pub fn pass() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            hector_version: env!("CARGO_PKG_VERSION").to_string(),
            status: Status::Pass,
            violations: vec![],
            passed_checks: vec![],
            elapsed_ms: 0,
            deferred_rules: vec![],
        }
    }

    pub fn from_violations(
        violations: Vec<Violation>,
        passed: Vec<String>,
        elapsed_ms: u64,
    ) -> Self {
        let has_internal = violations.iter().any(|v| v.engine == Engine::Internal);
        let has_error = violations
            .iter()
            .any(|v| v.engine != Engine::Internal && v.severity == Severity::Error);
        let status = if has_internal {
            Status::InternalError
        } else if has_error {
            Status::Block
        } else if violations.is_empty() {
            Status::Pass
        } else {
            Status::Warn
        };
        Self {
            schema_version: SCHEMA_VERSION,
            hector_version: env!("CARGO_PKG_VERSION").to_string(),
            status,
            violations,
            passed_checks: passed,
            elapsed_ms,
            deferred_rules: vec![],
        }
    }
}
