use crate::config::Rule;

/// Build the user-side prompt for the LLM. The LLM is instructed to return
/// a JSON array of {rule_id, status, message?, line?} objects.
pub fn build_prompt(rules: &[(&str, &Rule)], primary: &str, context: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("You are evaluating code changes against project policies. For each rule below, decide whether the code violates it.\n\n");
    out.push_str("## Rules\n\n");
    for (id, rule) in rules {
        out.push_str(&format!("- `{id}`: {}\n", rule.description));
    }
    out.push_str("\n## Code\n\n```\n");
    out.push_str(primary);
    out.push_str("\n```\n");
    if let Some(ctx) = context {
        out.push_str("\n## Additional context\n\n```\n");
        out.push_str(ctx);
        out.push_str("\n```\n");
    }
    out.push_str("\n## Instructions\n\n");
    out.push_str("Return ONLY a JSON array. Each element: {\"rule_id\": string, \"status\": \"pass\" | \"violation\", \"message\": string (only if violation), \"line\": number (optional)}.\n");
    out.push_str("No prose, no markdown fences, just the array.\n");
    out
}
