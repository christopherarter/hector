use anyhow::{anyhow, Context, Result};
use std::path::Path;

/// Strip `semantic` and `session` rules and the top-level `llm:` block from
/// the parsed YAML mapping. Returns `(dropped_rules, dropped_llm_block)` where
/// `dropped_rules` is a list of `(rule_id, engine)` pairs.
fn strip_removed_llm_config(map: &mut serde_yaml::Mapping) -> (Vec<(String, String)>, bool) {
    let mut dropped: Vec<(String, String)> = Vec::new();

    if let Some(rules_val) = map.get_mut(serde_yaml::Value::String("rules".into())) {
        if let Some(rules) = rules_val.as_mapping_mut() {
            // Collect the keys to remove first to avoid borrow conflict.
            let mut doomed: Vec<serde_yaml::Value> = Vec::new();
            for (k, v) in rules.iter() {
                if let Some(engine) = v.get("engine").and_then(|e| e.as_str()) {
                    if matches!(engine, "semantic" | "session") {
                        dropped.push((
                            k.as_str().unwrap_or("<non-string id>").to_string(),
                            engine.to_string(),
                        ));
                        doomed.push(k.clone());
                    }
                }
            }
            for k in &doomed {
                rules.remove(k);
            }
        }
    }

    let dropped_llm_block = map
        .remove(serde_yaml::Value::String("llm".into()))
        .is_some();

    (dropped, dropped_llm_block)
}

pub fn run(dir: &Path, clean: bool) -> Result<i32> {
    let bully = dir.join(".bully.yml");
    let hector = dir.join(".hector.yml");

    if !bully.exists() {
        return Err(anyhow!("no .bully.yml found in {}", dir.display()));
    }
    if hector.exists() {
        return Err(anyhow!(
            "{} already exists; refusing to overwrite",
            hector.display()
        ));
    }

    let raw =
        std::fs::read_to_string(&bully).with_context(|| format!("reading {}", bully.display()))?;

    // Parse-then-set instead of a naive string replace, which would also
    // rewrite `schema_version: 1` inside comments and string values. Comments
    // are lost by the serde round-trip; that's an explicit one-shot tradeoff
    // for migration (and we tell the user below).
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&raw)
        .with_context(|| format!("parsing {} as YAML", bully.display()))?;
    let map = doc
        .as_mapping_mut()
        .ok_or_else(|| anyhow!("{} root is not a YAML mapping", bully.display()))?;
    map.insert(
        serde_yaml::Value::String("schema_version".into()),
        serde_yaml::Value::Number(2.into()),
    );

    let (dropped, dropped_llm_block) = strip_removed_llm_config(map);

    let migrated = serde_yaml::to_string(&doc)
        .with_context(|| format!("re-serializing migrated {}", bully.display()))?;
    std::fs::write(&hector, migrated)?;

    let bully_dir = dir.join(".bully");
    let hector_dir = dir.join(".hector");
    if bully_dir.exists() && !hector_dir.exists() {
        std::fs::rename(&bully_dir, &hector_dir).with_context(|| {
            format!("moving {} -> {}", bully_dir.display(), hector_dir.display())
        })?;
    }

    if clean {
        std::fs::remove_file(&bully)?;
    }

    println!("migrated: {} -> {}", bully.display(), hector.display());
    println!(
        "note: migration parsed and re-serialized the YAML; comments and \
         non-essential formatting were not preserved."
    );
    println!("note: run `hector trust` next to sign the migrated config.");
    if !clean {
        println!("note: .bully.yml preserved. Run with --clean to remove.");
    }

    for (id, engine) in &dropped {
        eprintln!(
            "note: dropped rule '{id}' — engine '{engine}' was removed in \
             hector 0.2; rewrite it as a script or ast rule if still needed"
        );
    }
    if dropped_llm_block {
        eprintln!("note: dropped 'llm:' block — LLM evaluation was removed in hector 0.2");
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml_mapping(src: &str) -> serde_yaml::Mapping {
        let v: serde_yaml::Value = serde_yaml::from_str(src).unwrap();
        v.as_mapping().unwrap().clone()
    }

    #[test]
    fn strip_removes_semantic_and_session_keeps_script() {
        let mut map = yaml_mapping(
            "
schema_version: 1
rules:
  keep-me:
    engine: script
  drop-semantic:
    engine: semantic
  drop-session:
    engine: session
",
        );
        let (dropped, dropped_llm) = strip_removed_llm_config(&mut map);
        assert!(!dropped_llm);
        assert_eq!(dropped.len(), 2);
        let ids: Vec<&str> = dropped.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"drop-semantic"));
        assert!(ids.contains(&"drop-session"));
        let rules = map
            .get(serde_yaml::Value::String("rules".into()))
            .and_then(|v| v.as_mapping())
            .unwrap();
        assert!(rules.contains_key(serde_yaml::Value::String("keep-me".into())));
        assert!(!rules.contains_key(serde_yaml::Value::String("drop-semantic".into())));
        assert!(!rules.contains_key(serde_yaml::Value::String("drop-session".into())));
    }

    #[test]
    fn strip_removes_llm_block() {
        let mut map = yaml_mapping(
            "
schema_version: 1
llm:
  provider: anthropic
  model: claude-x
rules:
  r:
    engine: script
",
        );
        let (dropped, dropped_llm) = strip_removed_llm_config(&mut map);
        assert!(dropped_llm);
        assert!(dropped.is_empty());
        assert!(!map.contains_key(serde_yaml::Value::String("llm".into())));
    }

    #[test]
    fn strip_noop_when_nothing_to_remove() {
        let mut map = yaml_mapping("schema_version: 1\nrules:\n  r:\n    engine: script\n");
        let (dropped, dropped_llm) = strip_removed_llm_config(&mut map);
        assert!(!dropped_llm);
        assert!(dropped.is_empty());
    }
}
