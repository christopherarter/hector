use hector_core::config::ContextScope;
use hector_core::engine::context::expand_context;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn diff_scope_returns_diff_as_is() {
    let result = expand_context(
        ContextScope::Diff,
        Some("--- a/foo\n+++ b/foo\n@@ -1 +1 @@\n-old\n+new"),
        None,
        Path::new("/tmp"),
    );
    let (primary, ctx) = result.unwrap();
    assert!(primary.contains("+new"));
    assert!(ctx.is_none());
}

#[test]
fn file_scope_returns_file_content() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "the whole file\n").unwrap();
    let result = expand_context(ContextScope::File, None, Some(&file), dir.path());
    let (primary, ctx) = result.unwrap();
    assert!(primary.contains("the whole file"));
    assert!(ctx.is_none());
}

#[test]
fn repo_scope_falls_back_to_file_for_now() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "file content\n").unwrap();
    let result = expand_context(ContextScope::Repo, None, Some(&file), dir.path());
    let (primary, ctx) = result.unwrap();
    assert!(primary.contains("file content"));
    // Repo expansion is degraded in 0.1b — returns file content with a note in ctx.
    assert!(ctx.is_some());
}

#[test]
fn diff_scope_errors_when_diff_is_missing() {
    let err = expand_context(ContextScope::Diff, None, None, Path::new("/tmp"))
        .expect_err("missing diff");
    assert!(format!("{err:#}").contains("diff"));
}

#[test]
fn file_scope_errors_when_file_is_missing() {
    let err = expand_context(ContextScope::File, None, None, Path::new("/tmp"))
        .expect_err("missing file anchor");
    assert!(format!("{err:#}").contains("file"));
}

#[test]
fn repo_scope_errors_when_file_anchor_is_missing() {
    let err = expand_context(ContextScope::Repo, None, None, Path::new("/tmp"))
        .expect_err("missing repo anchor");
    assert!(format!("{err:#}").contains("repo"));
}

#[test]
fn file_scope_surfaces_read_error_for_nonexistent_path() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("nope.txt");
    let result = expand_context(ContextScope::File, None, Some(&missing), dir.path());
    assert!(result.is_err());
}

#[test]
fn repo_scope_surfaces_read_error_for_nonexistent_path() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("nope.txt");
    let result = expand_context(ContextScope::Repo, None, Some(&missing), dir.path());
    assert!(result.is_err());
}
