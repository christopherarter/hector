use serde::{Deserialize, Serialize};

/// Verdict JSON schema version.
///
/// History:
/// - v1: initial 0.1 shape with five `Engine` variants (`Script`, `Ast`,
///   `Semantic`, `Session`, `Trust`).
/// - v2 (P1-1): split overloaded `Engine::Trust` into `Engine::Trust`
///   (true trust-gate failures) and `Engine::Internal` (engine runtime
///   errors). Wire format for the new variant is `"internal"`.
pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub schema_version: u32,
    pub hector_version: String,
    pub status: Status,
    pub violations: Vec<Violation>,
    pub passed_checks: Vec<String>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Warn,
    Block,
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
        }
    }

    pub fn from_violations(
        violations: Vec<Violation>,
        passed: Vec<String>,
        elapsed_ms: u64,
    ) -> Self {
        let status = if violations.iter().any(|v| v.severity == Severity::Error) {
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
        }
    }
}
