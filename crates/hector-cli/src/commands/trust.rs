use anyhow::Result;
use std::path::Path;

pub fn run(_config: &Path) -> Result<i32> {
    eprintln!(
        "trust is not enforced in hector 0.3 (the out-of-repo trust store returns in a later release)"
    );
    Ok(0)
}
