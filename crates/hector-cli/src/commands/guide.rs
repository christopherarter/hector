//! C2 phase 2 stub — replaced wholesale in Phase 3.

use crate::cli::OutputFormat;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn run(_file: PathBuf, _format: OutputFormat, _config: &Path) -> Result<i32> {
    eprintln!("ERROR: hector guide is not yet implemented (C2 phase 3)");
    Ok(1)
}
