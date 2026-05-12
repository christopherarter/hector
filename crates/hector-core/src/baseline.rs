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

    /// Persist the baseline atomically.
    ///
    /// P2-5: the previous implementation used `std::fs::write` which
    /// `open(O_TRUNC) → write → close`s the target. A crash between
    /// truncate and the final write left a half-written or empty file
    /// that future loads couldn't parse. We now serialize to a sibling
    /// temp file under the same parent directory, `fsync` the bytes, and
    /// then `rename` onto the target — POSIX guarantees the rename is
    /// atomic on the same filesystem, so readers either see the full old
    /// file or the full new file, never a torn one.
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;

        // Place the temp alongside the target so `rename` stays on the
        // same filesystem (cross-fs rename is not atomic). Include the
        // PID to keep concurrent `save` invocations from clobbering each
        // other's temp files.
        let tmp_name = match path.file_name() {
            Some(n) => format!("{}.tmp.{}", n.to_string_lossy(), std::process::id()),
            None => format!("baseline.tmp.{}", std::process::id()),
        };
        let tmp_path = parent.join(tmp_name);

        let payload = serde_json::to_string_pretty(self)?;
        {
            let mut f = std::fs::File::create(&tmp_path)?;
            f.write_all(payload.as_bytes())?;
            // sync_all flushes data + metadata so the rename below
            // promotes only fully-durable bytes onto the target.
            f.sync_all()?;
        }
        // If rename fails, do best-effort cleanup of the temp so we
        // don't litter the parent directory.
        if let Err(e) = std::fs::rename(&tmp_path, path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e.into());
        }
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
