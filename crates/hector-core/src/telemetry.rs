//! Append-only check log at `.hector/log.jsonl`.
//!
//! Typed records: every line is one `LogEntry`. The discriminator is `type`
//! (snake_case). Payload fields are variant-specific. The legacy flat-record
//! reader was removed at the 0.3 redesign (the deprecation window is over).
//!
//! Wire format documented in [`docs/operating/telemetry.md`](../../docs/operating/telemetry.md).
use crate::verdict::Status;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Telemetry record-set version. Independent of the verdict schema.
///
/// Bumps when this enum's shape changes. Bumped to 5: `LogEntry::Check.file`
/// became `Option<String>` (absent on pre-commit/set invocations) and
/// `set_size: Option<usize>` was added (present on pre-commit to record the
/// number of files in the checked set).
pub const SCHEMA_VERSION: u32 = 5;

/// Per-check outcome line carried inside a [`LogEntry::Check`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerCheckRecord {
    pub check: String,
    /// Step within a multi-step check. `None` in Phase 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    pub status: Status,
    pub elapsed_ms: u64,
    /// Optional reason: a stable `InternalReason` string for crashed checks.
    /// `None` for vanilla pass/block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One line in `.hector/log.jsonl`.
///
/// Discriminator field is `type`; variant payload follows. `Check.checks` is
/// empty when no check matched the file (file was checked, no check ran).
///
/// `file` is present on write-lifecycle invocations (the absolute path of the
/// file being checked) and absent on pre-commit/set invocations where there is
/// no single primary target. `set_size` is the inverse: present on pre-commit
/// with the count of files in the set, absent on per-file write records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogEntry {
    Check {
        ts: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        file: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        set_size: Option<usize>,
        event: String,
        status: Status,
        elapsed_ms: u64,
        checks: Vec<PerCheckRecord>,
    },
}

/// Append one record. Atomic single-write; owner-only mode; advisory `flock`
/// to serialize concurrent writers.
pub fn append(path: &Path, entry: &LogEntry) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut opts = OpenOptions::new();
    opts.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;

    let mut line = serde_json::to_string(entry)?;
    line.push('\n');

    #[cfg(unix)]
    {
        use fs4::fs_std::FileExt;
        FileExt::lock_exclusive(&file)?;
        let result = file.write_all(line.as_bytes());
        FileExt::unlock(&file)?;
        result?;
    }
    #[cfg(not(unix))]
    file.write_all(line.as_bytes())?;

    Ok(())
}

/// Parse raw JSONL text into entries and dropped lines.
///
/// Returns `(valid_entries, dropped)` where each dropped item is
/// `(line_number_1based, error_string)`. Callers decide what to do with
/// the dropped information (log to stderr, silently discard, etc.).
fn parse_entries(raw: &str) -> (Vec<LogEntry>, Vec<(usize, String)>) {
    let mut entries = Vec::new();
    let mut dropped = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<LogEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(e) => dropped.push((i + 1, e.to_string())),
        }
    }
    (entries, dropped)
}

/// Read every record in `path`. Malformed lines are warned to stderr and
/// dropped — a single corrupt line should not fail the whole batch.
pub fn read_all(path: &Path) -> Result<Vec<LogEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let (entries, dropped) = parse_entries(&raw);
    for (line_num, err) in &dropped {
        eprintln!(
            "hector: warning — telemetry log {}:{} dropped (parse error: {err})",
            path.display(),
            line_num
        );
    }
    Ok(entries)
}

/// Like [`read_all`] but never writes to stderr on malformed lines.
///
/// Used by the `hector watch` event loop, which ticks every ~250 ms while
/// an alternate-screen TUI is active. Any `eprintln!` during that window
/// bleeds through raw mode and corrupts the rendered frame. Dropped lines
/// are silently ignored; only the valid entries are returned.
///
/// Missing file → `Ok(Vec::new())`, same as [`read_all`].
pub fn read_all_quiet(path: &Path) -> Result<Vec<LogEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let (entries, _dropped) = parse_entries(&raw);
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write-lifecycle record: `file` is present, `set_size` is absent.
    #[test]
    fn round_trips_a_write_entry() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log.jsonl");
        let entry = LogEntry::Check {
            ts: "2026-06-15T00:00:00Z".into(),
            file: Some("a.rs".into()),
            set_size: None,
            event: "edit".into(),
            status: Status::Block,
            elapsed_ms: 3,
            checks: vec![PerCheckRecord {
                check: "no-todo".into(),
                step: None,
                status: Status::Block,
                elapsed_ms: 3,
                reason: None,
            }],
        };
        append(&log, &entry).unwrap();
        let back = read_all(&log).unwrap();
        assert_eq!(back, vec![entry]);
        // Confirm `file` key is present and `set_size` key is absent in the JSON.
        let raw = std::fs::read_to_string(&log).unwrap();
        assert!(raw.contains("\"file\":"), "write record must include file");
        assert!(
            !raw.contains("\"set_size\":"),
            "write record must not include set_size"
        );
    }

    /// Pre-commit/set-level record: `file` is absent, `set_size` is present.
    #[test]
    fn round_trips_a_pre_commit_entry() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log.jsonl");
        let entry = LogEntry::Check {
            ts: "2026-06-28T00:00:00Z".into(),
            file: None,
            set_size: Some(3),
            event: "pre-commit".into(),
            status: Status::Pass,
            elapsed_ms: 5,
            checks: vec![],
        };
        append(&log, &entry).unwrap();
        let back = read_all(&log).unwrap();
        assert_eq!(back, vec![entry]);
        // Confirm `file` key is absent and `set_size` key is present in the JSON.
        let raw = std::fs::read_to_string(&log).unwrap();
        assert!(
            !raw.contains("\"file\":"),
            "pre-commit record must not include file"
        );
        assert!(
            raw.contains("\"set_size\":3"),
            "pre-commit record must include set_size"
        );
    }

    #[test]
    fn schema_version_is_5() {
        assert_eq!(SCHEMA_VERSION, 5);
    }

    /// Helper: one valid `LogEntry::Check` as a JSONL line.
    fn valid_jsonl_line() -> String {
        let entry = LogEntry::Check {
            ts: "2026-06-29T00:00:00Z".into(),
            file: Some("foo.rs".into()),
            set_size: None,
            event: "write".into(),
            status: Status::Pass,
            elapsed_ms: 1,
            checks: vec![],
        };
        serde_json::to_string(&entry).unwrap()
    }

    /// `read_all` returns the valid entry when a file also contains a malformed
    /// line. The malformed line is dropped (and warned to stderr, but we don't
    /// capture that here — we only care the valid entry survives).
    #[test]
    fn read_all_survives_one_malformed_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log.jsonl");
        let content = format!("{}\nnot-json\n", valid_jsonl_line());
        std::fs::write(&log, content).unwrap();

        let entries = read_all(&log).unwrap();
        assert_eq!(entries.len(), 1, "read_all must return the one valid entry");
        let LogEntry::Check { file, .. } = &entries[0];
        assert_eq!(file.as_deref(), Some("foo.rs"));
    }

    /// `read_all_quiet` returns the valid entry, drops the malformed line
    /// silently, and does not panic.
    #[test]
    fn read_all_quiet_returns_valid_entry_and_drops_malformed_silently() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log.jsonl");
        let content = format!("{}\nbad-json-line\n", valid_jsonl_line());
        std::fs::write(&log, content).unwrap();

        let entries = read_all_quiet(&log).unwrap();
        assert_eq!(
            entries.len(),
            1,
            "read_all_quiet must return exactly the one valid entry"
        );
        let LogEntry::Check { file, .. } = &entries[0];
        assert_eq!(file.as_deref(), Some("foo.rs"));
    }

    /// `read_all_quiet` returns `Ok([])` for a missing file (no panic, no Err).
    #[test]
    fn read_all_quiet_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("nonexistent.jsonl");
        let result = read_all_quiet(&log).unwrap();
        assert!(result.is_empty());
    }
}
