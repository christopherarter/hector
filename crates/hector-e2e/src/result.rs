//! Forensics captured from one container run.

#[derive(Debug, Default)]
pub struct RunResult {
    pub exit_code: i32,
    pub verdict: Option<serde_json::Value>,
    pub log_entries: Vec<serde_json::Value>,
    pub target_after: Option<String>,
    pub harness_log: String,
    pub drive_log: String,
}
