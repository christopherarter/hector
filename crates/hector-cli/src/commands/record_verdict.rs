//! H2: `hector record-verdict` — append a single `SemanticVerdict`
//! record to `.hector/log.jsonl`. Consumed by the Claude Code
//! interpreter skill after a subagent evaluates a deferred semantic
//! rule.

use anyhow::Result;
use clap::ValueEnum;
use std::path::Path;

/// Two-arm enum enforcing `--verdict pass | violation` at clap-parse
/// time. Anything else is a parse error from clap — the runtime body
/// of [`run`] cannot see an invalid value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum VerdictValue {
    Pass,
    Violation,
}

impl VerdictValue {
    #[allow(dead_code)] // Phase 2 wires this into the telemetry append.
    fn as_wire_str(self) -> &'static str {
        // The on-disk wire format mirrors bully's `pass` / `violation`
        // (lowercase). The `LogEntry::SemanticVerdict.verdict: String`
        // field is intentionally stringly-typed at the telemetry layer
        // so future extensions don't require a schema bump.
        match self {
            Self::Pass => "pass",
            Self::Violation => "violation",
        }
    }
}

pub fn run(rule: String, verdict: VerdictValue, file: Option<String>, dir: &Path) -> Result<i32> {
    // Phase 1 stub — Phase 2 fills in the actual append.
    let _ = (rule, verdict, file, dir);
    Ok(0)
}
