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

    /// Stable identity of a violation for baseline membership.
    ///
    /// P1-4: the previous `"{rule_id}::{file}::{line}"` format collided when
    /// `::` appeared in either the rule_id or the file path
    /// (e.g. `rule_id="a::b" file="c"` vs `rule_id="a" file="b::c"`). JSON
    /// encoding of the tuple is unambiguous for every input and also
    /// preserves the `Option<u32>` discriminant on `line`, so `line: None`
    /// and `line: Some(0)` no longer collapse to the same fingerprint.
    pub fn fingerprint(v: &Violation) -> String {
        // Serializing a 3-tuple of primitives cannot fail; an `Err` here
        // would indicate a serde_json bug. Fall back to the legacy format
        // as a defensive last resort rather than panicking the runner.
        serde_json::to_string(&(&v.rule_id, &v.file, &v.line))
            .unwrap_or_else(|_| format!("{}::{}::{}", v.rule_id, v.file, v.line.unwrap_or(0)))
    }

    pub fn add(&mut self, v: &Violation) {
        self.fingerprints.insert(Self::fingerprint(v));
    }

    pub fn contains(&self, v: &Violation) -> bool {
        self.fingerprints.contains(&Self::fingerprint(v))
    }
}
