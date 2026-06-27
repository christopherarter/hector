use super::Options;
use anyhow::{anyhow, Result};
use hector_core::adapter::{
    all_harnesses, detect, install, uninstall, AdapterEnv, Harness, InstallResult, Scope,
};
use std::io::{IsTerminal, Write};

pub fn run_hook_phase(env: &AdapterEnv, opts: &Options) -> Result<i32> {
    let scope = if opts.global {
        Scope::Global
    } else {
        Scope::Local
    };
    let names = choose_harnesses(env, opts)?;
    if names.is_empty() {
        return Ok(0);
    }
    let registry = all_harnesses();
    let selected: Vec<&Harness> = names
        .iter()
        .filter_map(|n| registry.iter().find(|h| h.name == n))
        .collect();

    let mut any_ok = false;
    let mut any_fail = false;
    for h in selected {
        let outcome = if opts.uninstall {
            uninstall(h, env, scope, opts.dry_run)
        } else {
            install(h, env, scope, opts.dry_run)
        };
        match outcome {
            Ok(o) => {
                any_ok = true;
                print_outcome(o.harness, &o.result, o.hint, opts.uninstall);
            }
            Err(e) => {
                any_fail = true;
                println!("  {:<12} failed: {e:#}", h.name);
            }
        }
    }
    Ok(if any_fail && !any_ok { 3 } else { 0 })
}

/// Resolve the harness set: explicit `--harness`, else detect+confirm.
fn choose_harnesses(env: &AdapterEnv, opts: &Options) -> Result<Vec<String>> {
    if !opts.harnesses.is_empty() {
        return select_harness_names(&opts.harnesses);
    }
    let detected: Vec<String> = detect(env)
        .into_iter()
        .filter(|(_, found)| *found)
        .map(|(n, _)| n.to_string())
        .collect();
    if detected.is_empty() {
        println!(
            "no supported harnesses detected; run `hector init --harness all` to wire all four"
        );
        return Ok(vec![]);
    }
    if opts.yes {
        return Ok(detected);
    }
    if !std::io::stdin().is_terminal() {
        println!(
            "detected: {} — re-run with `--yes` or `--harness <name>` to install",
            detected.join(", ")
        );
        return Ok(vec![]);
    }
    print!("Install hector hooks into {}? [Y/n] ", detected.join(", "));
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(if parse_confirm(&line) {
        detected
    } else {
        vec![]
    })
}

/// Validate explicit `--harness` names; `all` expands to the full registry.
fn select_harness_names(requested: &[String]) -> Result<Vec<String>> {
    let known: Vec<&'static str> = all_harnesses().iter().map(|h| h.name).collect();
    let mut out: Vec<String> = Vec::new();
    for r in requested {
        if r == "all" {
            return Ok(known.iter().map(|s| s.to_string()).collect());
        }
        if !known.contains(&r.as_str()) {
            return Err(anyhow!(
                "unknown harness `{r}` (supported: {})",
                known.join(", ")
            ));
        }
        if !out.contains(r) {
            out.push(r.clone());
        }
    }
    Ok(out)
}

fn parse_confirm(line: &str) -> bool {
    let a = line.trim().to_lowercase();
    a.is_empty() || a == "y" || a == "yes"
}

fn print_outcome(harness: &str, result: &InstallResult, hint: &str, uninstalling: bool) {
    match result {
        InstallResult::Installed if uninstalling => println!("  {harness:<12} removed"),
        InstallResult::Installed => println!("  {harness:<12} installed — {hint}"),
        InstallResult::Updated => println!("  {harness:<12} updated — {hint}"),
        InstallResult::AlreadyPresent => println!("  {harness:<12} already present"),
        InstallResult::Skipped(why) => println!("  {harness:<12} skipped: {why}"),
        InstallResult::Failed(why) => println!("  {harness:<12} failed: {why}"),
        InstallResult::DryRun(plan) => {
            println!("  {harness:<12} dry-run:");
            for line in plan {
                println!("      {line}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_confirm_defaults_yes_on_empty() {
        assert!(parse_confirm(""));
        assert!(parse_confirm("\n"));
        assert!(parse_confirm("y"));
        assert!(parse_confirm("YES"));
    }
    #[test]
    fn parse_confirm_no() {
        assert!(!parse_confirm("n"));
        assert!(!parse_confirm("no"));
        assert!(!parse_confirm("x"));
    }

    #[test]
    fn select_explicit_all_returns_every_harness() {
        let names = select_harness_names(&["all".to_string()]).unwrap();
        assert_eq!(names, vec!["claude-code", "reasonix", "pi", "opencode"]);
    }
    #[test]
    fn select_explicit_unknown_errors() {
        assert!(select_harness_names(&["bogus".to_string()]).is_err());
    }
    #[test]
    fn select_explicit_dedup_and_order() {
        let names = select_harness_names(&["pi".to_string(), "pi".to_string()]).unwrap();
        assert_eq!(names, vec!["pi"]);
    }
}
