use crate::verdict::Violation;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Baseline {
    pub fingerprints: HashSet<String>,
}

impl Baseline {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn fingerprint(v: &Violation) -> String {
        format!("{}::{}::{}", v.rule_id, v.file, v.line.unwrap_or(0))
    }

    pub fn add(&mut self, v: &Violation) {
        self.fingerprints.insert(Self::fingerprint(v));
    }

    pub fn contains(&self, v: &Violation) -> bool {
        self.fingerprints.contains(&Self::fingerprint(v))
    }
}
