//! Docker shell-outs.

use crate::result::RunResult;

/// Build the base image and the per-adapter image. Idempotent: re-running
/// without changes hits the layer cache.
pub fn build_image(_adapter: &str) -> anyhow::Result<()> {
    anyhow::bail!("not yet implemented")
}

/// Run one case inside the per-adapter container and capture forensics.
pub fn run_case(_adapter: &str, _case: &str) -> anyhow::Result<RunResult> {
    anyhow::bail!("not yet implemented")
}
