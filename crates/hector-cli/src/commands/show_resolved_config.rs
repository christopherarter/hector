//! C3 — `hector show-resolved-config`. Print the post-extends merged
//! rule set in one of three formats. Read-only.

use crate::cli::ShowFormat;
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The shape that gets serialized by the YAML and JSON formatters.
///
/// Mirrors `Config` minus the two fields that are meaningless after the
/// extends merge:
/// - `trust:` — per-config-file fingerprint; the merged form has no
///   single source file to fingerprint.
/// - `extends:` — already consumed by the merge; leaving it in would
///   imply unresolved inheritance.
///
/// Constructed by [`build_view`] from a `Config` + the rule origin map.
#[derive(Debug, Serialize)]
struct ResolvedView<'a> {
    schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    llm: Option<&'a hector_core::config::LlmConfig>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skip: &'a Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<&'a hector_core::config::ExecutionConfig>,
    /// Sorted-by-id rule list. Each entry carries an `origin` field
    /// alongside the rule body so the JSON shape can attribute every
    /// rule to its source file.
    rules: BTreeMap<&'a str, RuleView<'a>>,
}

#[derive(Debug, Serialize)]
struct RuleView<'a> {
    #[serde(flatten)]
    rule: &'a hector_core::config::Rule,
    origin: String,
}

fn build_view<'a>(
    cfg: &'a hector_core::config::Config,
    origins: &'a BTreeMap<String, PathBuf>,
) -> ResolvedView<'a> {
    let rules: BTreeMap<&'a str, RuleView<'a>> = cfg
        .rules
        .iter()
        .map(|(id, rule)| {
            let origin = origins
                .get(id)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            (id.as_str(), RuleView { rule, origin })
        })
        .collect();
    ResolvedView {
        schema_version: cfg.schema_version,
        llm: cfg.llm.as_ref(),
        skip: &cfg.skip,
        execution: cfg.execution.as_ref(),
        rules,
    }
}

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
    cfg: &hector_core::config::Config,
    origins: &BTreeMap<String, PathBuf>,
) -> Result<String> {
    let view = build_view(cfg, origins);
    let body = serde_yaml::to_string(&view)?;
    Ok(annotate_yaml_with_origins(&body, origins))
}

/// Walk the rendered YAML body and inject a `# origin: <path>` comment
/// line above each rule entry. Detects rule entries by matching lines
/// of the form `^  <id>:$` *inside* the `rules:` block — every rule key
/// in `ResolvedView` is two-space-indented.
fn annotate_yaml_with_origins(
    body: &str,
    origins: &BTreeMap<String, PathBuf>,
) -> String {
    let mut out = String::with_capacity(body.len() + 128);
    let mut in_rules_block = false;
    for line in body.lines() {
        if line.starts_with("rules:") {
            in_rules_block = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_rules_block {
            // A rule-key line is `  <id>:` with exactly two leading
            // spaces and a trailing colon. Anything more deeply
            // indented is a field of the rule body, not a new rule.
            if let Some(stripped) = line.strip_prefix("  ") {
                if !stripped.starts_with(' ')
                    && stripped.ends_with(':')
                    && stripped.len() > 1
                {
                    let id = &stripped[..stripped.len() - 1];
                    if let Some(origin) = origins.get(id) {
                        out.push_str(&format!("  # origin: {}\n", origin.display()));
                    }
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn format_json(
    _cfg: &hector_core::config::Config,
    _origins: &std::collections::BTreeMap<String, std::path::PathBuf>,
) -> Result<String> {
    Ok(String::new())
}
