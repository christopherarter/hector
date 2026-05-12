use anyhow::{anyhow, Context, Result};
use serde_yaml::Value;
use sha2::{Digest, Sha256};

/// Strip the `trust:` block and serialize keys in canonical (sorted) order.
pub fn canonicalize_for_fingerprint(input: &str) -> Result<String> {
    let mut value: Value = serde_yaml::from_str(input).context("parse yaml")?;
    if let Value::Mapping(ref mut map) = value {
        map.remove(Value::String("trust".into()));
    }
    let canonical = canonical_serialize(&value);
    Ok(canonical)
}

fn canonical_serialize(value: &Value) -> String {
    let sorted = sort_keys(value.clone());
    serde_yaml::to_string(&sorted).expect("serialize sorted yaml")
}

fn sort_keys(value: Value) -> Value {
    match value {
        Value::Mapping(m) => {
            let mut pairs: Vec<(Value, Value)> =
                m.into_iter().map(|(k, v)| (k, sort_keys(v))).collect();
            pairs.sort_by(|a, b| {
                serde_yaml::to_string(&a.0)
                    .unwrap_or_default()
                    .cmp(&serde_yaml::to_string(&b.0).unwrap_or_default())
            });
            let mut out = serde_yaml::Mapping::new();
            for (k, v) in pairs {
                out.insert(k, v);
            }
            Value::Mapping(out)
        }
        Value::Sequence(s) => Value::Sequence(s.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

/// Compute the sha256 fingerprint of a config, prefixed with `sha256:`.
pub fn fingerprint(input: &str) -> Result<String> {
    let canonical = canonicalize_for_fingerprint(input)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    Ok(format!("sha256:{:x}", digest))
}

/// Verify that the `trust.fingerprint` field of the config equals the recomputed fingerprint.
pub fn verify(input: &str) -> Result<()> {
    let value: Value = serde_yaml::from_str(input).context("parse yaml")?;
    let recorded = value
        .get("trust")
        .and_then(|t| t.get("fingerprint"))
        .and_then(|f| f.as_str())
        .ok_or_else(|| anyhow!("trust block missing or empty; run `hector trust`"))?
        .to_string();
    let expected = fingerprint(input)?;
    if recorded == expected {
        Ok(())
    } else {
        Err(anyhow!(
            "config changed since last trust — review changes and run `hector trust` to acknowledge"
        ))
    }
}

/// Update or insert the `trust:` block in the YAML source with a fresh fingerprint.
///
/// Performs a string-level edit so comments, key order, and scalar style in the
/// rest of the file are preserved verbatim (P2-7). The fingerprint itself is
/// computed via [`fingerprint`], which canonicalizes the YAML semantically and
/// is unaffected by comments or formatting.
///
/// If a top-level `trust:` block already exists, it is replaced in place. The
/// block is identified as a line starting with `trust:` at column 0 and ending
/// at the next top-level key (or EOF). Otherwise, a fresh block is appended.
pub fn write_trust_block(input: &str) -> Result<String> {
    let fp = fingerprint(input)?;
    let new_block = format!("trust:\n  fingerprint: {fp}\n");

    let lines: Vec<&str> = input.lines().collect();
    let trust_start = lines.iter().position(|l| {
        // Top-level `trust:` key (column 0, no leading whitespace). The trust
        // block is always written `trust:\n` with the fingerprint on the
        // following line, so an exact match on `trust:` (after stripping
        // trailing whitespace and inline comments) is sufficient and
        // unambiguously avoids matching `trusted:`, `trust_chain:`, etc.
        let no_comment = match l.find('#') {
            Some(i) => &l[..i],
            None => l,
        };
        no_comment.trim_end() == "trust:"
    });

    if let Some(start) = trust_start {
        // End-of-block: first subsequent non-empty line whose first byte is
        // not whitespace (i.e. another top-level key) — or EOF.
        let end = (start + 1..lines.len())
            .find(|i| {
                let l = lines[*i];
                !l.is_empty() && !l.starts_with(' ') && !l.starts_with('\t')
            })
            .unwrap_or(lines.len());

        let mut out = String::with_capacity(input.len() + new_block.len());
        if start > 0 {
            for l in &lines[..start] {
                out.push_str(l);
                out.push('\n');
            }
        }
        out.push_str(&new_block);
        if end < lines.len() {
            for l in &lines[end..] {
                out.push_str(l);
                out.push('\n');
            }
            // Preserve trailing-newline shape of the original.
            if !input.ends_with('\n') {
                out.pop();
            }
        }
        return Ok(out);
    }

    // No existing trust block — append at EOF.
    let mut out = String::with_capacity(input.len() + new_block.len() + 1);
    out.push_str(input);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&new_block);
    Ok(out)
}

