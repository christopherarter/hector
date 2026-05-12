//! Local diff analysis to short-circuit expensive semantic dispatch.
//!
//! `can_match_diff` answers a single question: given this diff, this file, and
//! this rule description, is it *possible* for the semantic engine to find a
//! violation? On `No(reason)`, the runner skips the LLM call entirely.
//!
//! This is a cost lever, not a correctness gate. False negatives (we say
//! `Yes` when the LLM would have passed) just mean the LLM runs anyway —
//! same as no filter. False positives (we say `No` when the LLM would have
//! flagged a violation) are silent misses, so each `No` branch errs
//! conservative: unknown extensions, unrecognized "avoid" phrasings, and
//! mixed comment-and-code all dispatch.

use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Empty,
    WhitespaceOnly,
    CommentsOnly,
    PureDeletion,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::WhitespaceOnly => "whitespace_only",
            Self::CommentsOnly => "comments_only",
            Self::PureDeletion => "pure_deletion",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanMatch {
    Yes,
    No(SkipReason),
}

pub fn can_match_diff(diff: &str, file_path: &Path, rule_description: &str) -> CanMatch {
    let lines: Vec<&str> = diff.lines().collect();
    let mut in_hunk = false;
    let mut added: Vec<&str> = Vec::new();
    let mut removed_count: usize = 0;

    for raw in &lines {
        if raw.starts_with("@@ ") || raw.starts_with("@@\t") {
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        if raw.starts_with("+++") || raw.starts_with("---") {
            continue;
        }
        if let Some(content) = raw.strip_prefix('+') {
            added.push(content);
        } else if raw.starts_with('-') {
            removed_count += 1;
        }
    }

    if !in_hunk {
        return CanMatch::No(SkipReason::Empty);
    }

    if added.is_empty() {
        if removed_count > 0 && is_avoid_rule(rule_description) {
            return CanMatch::No(SkipReason::PureDeletion);
        }
        return CanMatch::Yes;
    }

    if added.iter().all(|l| l.trim().is_empty()) {
        return CanMatch::No(SkipReason::WhitespaceOnly);
    }

    if let Some(markers) = comment_markers_for(file_path) {
        let all_comments = added.iter().all(|l| {
            let t = l.trim_start();
            t.is_empty() || markers.iter().any(|m| t.starts_with(m))
        });
        if all_comments && !rule_mentions_comments(rule_description) {
            return CanMatch::No(SkipReason::CommentsOnly);
        }
    }

    CanMatch::Yes
}

fn comment_markers_for(path: &Path) -> Option<&'static [&'static str]> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" | "c" | "h" | "cc" | "cpp" | "hpp" | "java" | "swift" | "kt" | "kts" | "scala"
        | "cs" | "go" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => &["//", "/*", "*/", "*"],
        "php" => &["//", "#", "/*", "*/", "*"],
        "py" | "rb" | "sh" | "bash" | "zsh" | "fish" | "yml" | "yaml" | "toml" | "ini" | "cfg"
        | "conf" | "mk" | "makefile" | "dockerfile" | "gitignore" => &["#"],
        "lua" | "hs" | "sql" | "ada" | "adb" | "ads" => &["--"],
        "lisp" | "lsp" | "el" | "scm" | "clj" | "cljs" | "cljc" => &[";"],
        "html" | "htm" | "xml" | "svg" | "vue" | "svelte" => &["<!--", "-->"],
        _ => return None,
    })
}

fn is_avoid_rule(description: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\b(avoid|don't|do not|no|ban|forbid|prohibit)\b").unwrap()
    });
    re.is_match(description)
}

fn rule_mentions_comments(description: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?i)\bcomments?\b").unwrap());
    re.is_match(description)
}
