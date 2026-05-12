use anyhow::Result;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub kind: String,
    pub file: String,
    pub rule_id: Option<String>,
    pub status: String,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub fn append(path: &Path, entry: &LogEntry) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut opts = OpenOptions::new();
    opts.append(true).create(true);
    #[cfg(unix)]
    {
        // Telemetry entries echo back file paths from the user's project, so
        // create owner-only by default rather than inheriting umask (typically
        // 0644). P2-16.
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;

    // Build the line as a single buffer so the actual write is a single
    // write_all syscall. Two separate write_all calls (line then '\n') leave
    // a window where a concurrent writer can interleave bytes between them.
    let mut line = serde_json::to_string(entry)?;
    line.push('\n');

    // For entries larger than PIPE_BUF (4 KiB on Linux, much smaller on macOS)
    // the kernel's atomic-append guarantee for O_APPEND no longer applies, and
    // concurrent writers can interleave even a single write_all. Serialize
    // writers with an advisory exclusive flock. The cost vs corruption risk
    // is negligible; we hold the lock only for the single write. P1-10.
    #[cfg(unix)]
    {
        use fs4::fs_std::FileExt;
        FileExt::lock_exclusive(&file)?;
        let result = file.write_all(line.as_bytes());
        // Release explicitly to keep the critical section tight; the lock
        // would also be released when `file` is dropped.
        FileExt::unlock(&file)?;
        result?;
    }
    #[cfg(not(unix))]
    file.write_all(line.as_bytes())?;

    Ok(())
}
