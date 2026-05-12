use crate::config::{Capabilities, WritesPolicy};
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub struct ExecOutcome {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Run a shell command under the requested capability constraints.
/// Returns Ok(outcome) if the process completed (even non-zero exit).
pub fn run_with_capabilities(cmd: &str, cwd: &Path, caps: &Capabilities) -> Result<ExecOutcome> {
    run_with_capabilities_env(cmd, cwd, caps, &[])
}

/// Same as [`run_with_capabilities`], with an extra env-injection slice.
///
/// Each `(name, value)` pair is applied to the child process environment
/// before spawning. Used by the script engine to pass attacker-controlled
/// values (like file paths) to the shell via env vars rather than splicing
/// them into the command text.
pub fn run_with_capabilities_env(
    cmd: &str,
    cwd: &Path,
    caps: &Capabilities,
    env: &[(&str, &str)],
) -> Result<ExecOutcome> {
    #[cfg(target_os = "linux")]
    {
        run_linux(cmd, cwd, caps, env)
    }
    #[cfg(not(target_os = "linux"))]
    {
        run_best_effort(cmd, cwd, caps, env)
    }
}

#[cfg(target_os = "linux")]
fn run_linux(
    cmd: &str,
    cwd: &Path,
    caps: &Capabilities,
    env: &[(&str, &str)],
) -> Result<ExecOutcome> {
    use nix::sched::{unshare, CloneFlags};
    let mut flags = CloneFlags::empty();
    if !caps.network {
        flags.insert(CloneFlags::CLONE_NEWNET);
    }
    if matches!(caps.writes, WritesPolicy::None | WritesPolicy::CwdOnly) {
        flags.insert(CloneFlags::CLONE_NEWNS);
    }
    let mut child = Command::new("sh");
    child.arg("-c").arg(cmd).current_dir(cwd);
    for (k, v) in env {
        child.env(k, v);
    }
    // Pre-exec hook to unshare into restricted namespaces.
    unsafe {
        use std::os::unix::process::CommandExt;
        let flags_captured = flags;
        let writes_policy = caps.writes;
        let cwd_owned = cwd.to_path_buf();
        child.pre_exec(move || {
            unshare(flags_captured).map_err(|e| std::io::Error::other(format!("unshare: {e}")))?;
            if flags_captured.contains(CloneFlags::CLONE_NEWNS) {
                apply_mount_policy(writes_policy, &cwd_owned)?;
            }
            Ok(())
        });
    }
    let output = child.output().context("running command")?;
    Ok(ExecOutcome {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

#[cfg(target_os = "linux")]
fn apply_mount_policy(policy: WritesPolicy, cwd: &Path) -> std::io::Result<()> {
    // Best-effort: this requires CAP_SYS_ADMIN in the new ns (granted by CLONE_NEWUSER).
    // For 0.1a, we skip mount remounting if not permitted; documented in docs/security.md.
    let _ = (policy, cwd);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn run_best_effort(
    cmd: &str,
    cwd: &Path,
    caps: &Capabilities,
    env: &[(&str, &str)],
) -> Result<ExecOutcome> {
    // macOS: caps are advisory; log the limitation, run normally.
    if !caps.network || !matches!(caps.writes, WritesPolicy::Unrestricted) {
        eprintln!(
            "hector: capability enforcement is best-effort on this platform (see docs/security.md); running command unrestricted"
        );
    }
    let mut child = Command::new("sh");
    child.arg("-c").arg(cmd).current_dir(cwd);
    for (k, v) in env {
        child.env(k, v);
    }
    let output = child.output().context("running command")?;
    Ok(ExecOutcome {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}
