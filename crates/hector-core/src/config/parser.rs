use super::types::Config;
use anyhow::{Context, Result};

pub fn parse_str(input: &str) -> Result<Config> {
    serde_yaml::from_str::<Config>(input).context("parsing hector config")
}

pub fn parse_file(path: &std::path::Path) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    parse_str(&content)
}
