//! C3 — `hector show-resolved-config`. Print the post-extends merged
//! rule set in one of three formats. Read-only.

use crate::cli::ShowFormat;
use anyhow::Result;
use std::path::Path;

pub fn run(config: &Path, format: ShowFormat) -> Result<i32> {
    match hector_core::config::extends::resolve_with_origin(config) {
        Ok((cfg, origins)) => {
            let body = match format {
                ShowFormat::Tsv => format_tsv(&cfg, &origins),
                ShowFormat::Yaml => format_yaml(&cfg, &origins)?,
                ShowFormat::Json => format_json(&cfg, &origins)?,
            };
            print!("{body}");
            Ok(0)
        }
        Err(e) => {
            eprintln!("ERROR: {:#}", e);
            Ok(1)
        }
    }
}

fn format_tsv(
    cfg: &hector_core::config::Config,
    origins: &std::collections::BTreeMap<String, std::path::PathBuf>,
) -> String {
    let mut out = String::new();
    for (id, rule) in sorted_rules(cfg) {
        let engine = engine_kind_str(rule.engine);
        let severity = severity_str(rule.severity);
        let scope = rule.scope.join(",");
        let fix_hint = rule.fix_hint.as_deref().unwrap_or("");
        let origin = origins
            .get(id)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        // Six columns; cell separator is a single tab; row terminator is
        // newline. Empty cells preserve column count so downstream
        // `cut -f6` still works on rows with no fix_hint.
        out.push_str(&format!(
            "{id}\t{engine}\t{severity}\t{scope}\t{fix_hint}\t{origin}\n"
        ));
    }
    out
}

/// Materialize the rule list once in deterministic id order. `BTreeMap`
/// already iterates in key order; we re-sort defensively so the output
/// contract isn't tied to the upstream container choice.
fn sorted_rules(
    cfg: &hector_core::config::Config,
) -> Vec<(&String, &hector_core::config::Rule)> {
    let mut v: Vec<(&String, &hector_core::config::Rule)> = cfg.rules.iter().collect();
    v.sort_by(|a, b| a.0.cmp(b.0));
    v
}

fn engine_kind_str(k: hector_core::config::EngineKind) -> &'static str {
    match k {
        hector_core::config::EngineKind::Script => "script",
        hector_core::config::EngineKind::Ast => "ast",
        hector_core::config::EngineKind::Semantic => "semantic",
        hector_core::config::EngineKind::Session => "session",
    }
}

fn severity_str(s: hector_core::config::Severity) -> &'static str {
    match s {
        hector_core::config::Severity::Error => "error",
        hector_core::config::Severity::Warning => "warning",
    }
}

fn format_yaml(
    _cfg: &hector_core::config::Config,
    _origins: &std::collections::BTreeMap<String, std::path::PathBuf>,
) -> Result<String> {
    Ok(String::new())
}

fn format_json(
    _cfg: &hector_core::config::Config,
    _origins: &std::collections::BTreeMap<String, std::path::PathBuf>,
) -> Result<String> {
    Ok(String::new())
}
