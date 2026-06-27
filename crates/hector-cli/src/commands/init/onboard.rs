use super::Options;
use anyhow::Result;
use hector_core::adapter::AdapterEnv;

pub fn run_hook_phase(_env: &AdapterEnv, _opts: &Options) -> Result<i32> {
    Ok(0)
}
