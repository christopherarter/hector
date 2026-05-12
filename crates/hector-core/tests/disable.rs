use hector_core::disable::DisableMap;

const SOURCE: &str = "\
let x = 1;
eval(expr); // hector-disable: no-eval reason: sandboxed input
console.log('hi'); /* hector-disable: no-console-log reason: debug only */
let y = 2;
";

#[test]
fn detects_line_comment_disable() {
    let map = DisableMap::from_source(SOURCE);
    assert!(map.is_disabled(2, "no-eval"));
    assert!(!map.is_disabled(2, "no-console-log"));
}

#[test]
fn detects_block_comment_disable() {
    let map = DisableMap::from_source(SOURCE);
    assert!(map.is_disabled(3, "no-console-log"));
}

#[test]
fn returns_false_when_no_disable() {
    let map = DisableMap::from_source(SOURCE);
    assert!(!map.is_disabled(1, "no-eval"));
    assert!(!map.is_disabled(4, "no-console-log"));
}

#[test]
fn parses_comma_separated_rule_list() {
    let src = "let x = 1; // hector-disable: a, b reason: x\n";
    let map = DisableMap::from_source(src);
    assert!(map.is_disabled(1, "a"));
    assert!(map.is_disabled(1, "b"));
    assert!(!map.is_disabled(1, "reason"));
    assert!(!map.is_disabled(1, "reason:"));
    assert!(!map.is_disabled(1, "x"));
}

#[test]
fn trims_trailing_comma_from_rule_id() {
    let src = "let x = 1; // hector-disable: a, reason: x\n";
    let map = DisableMap::from_source(src);
    assert!(map.is_disabled(1, "a"));
    assert!(!map.is_disabled(1, "a,"));
    assert!(!map.is_disabled(1, "reason"));
}

#[test]
fn existing_single_rule_unchanged() {
    let src = "eval(expr); // hector-disable: no-eval reason: x\n";
    let map = DisableMap::from_source(src);
    assert!(map.is_disabled(1, "no-eval"));
    assert!(!map.is_disabled(1, "reason"));
    assert!(!map.is_disabled(1, "x"));
}

#[test]
fn file_level_disable_silences_script_violation_without_line() {
    let src = "// hector-disable: noisy-script\nfn main() {}\n";
    let map = DisableMap::from_source(src);
    assert!(map.is_disabled_file_wide("noisy-script"));
    assert!(!map.is_disabled_file_wide("other-rule"));
}

// Regression: P2-4 — namespaced rule IDs commonly contain `/`
// (e.g. `python/no-print`). The directive parser used to treat `/` as an
// unconditional terminator (intended to handle CSS/JS block-comment
// closers), which silently truncated the id to `python` and dropped the
// rest. `/` is now a terminator only when followed by `/` or `*` (the
// actual line/block-comment patterns).
#[test]
fn allows_slash_inside_rule_id() {
    let src = "foo(); // hector-disable: python/no-print reason: legacy script\n";
    let map = DisableMap::from_source(src);
    assert!(
        map.is_disabled(1, "python/no-print"),
        "namespaced id with `/` must round-trip intact"
    );
    assert!(!map.is_disabled(1, "python"));
    assert!(!map.is_disabled(1, "no-print"));
}

// P2-4: a trailing block-comment closer (` */`) still terminates the
// directive — the slash there really is the start of `*/`. Verify both
// the comma list AND the namespaced id survive.
#[test]
fn block_comment_closer_terminates_namespaced_ids() {
    let src = "x(); /* hector-disable: ns/a, ns/b reason: x */\n";
    let map = DisableMap::from_source(src);
    assert!(map.is_disabled(1, "ns/a"));
    assert!(map.is_disabled(1, "ns/b"));
    assert!(!map.is_disabled(1, "ns/a*"));
}

// P2-4: line-comment opener inside the rest of the line. Once we hit a
// `//` we stop scanning rule ids. (Realistically the `hector-disable:`
// itself is *after* the leading `//`, so this case primarily protects
// against a second `//` later in the same line.)
#[test]
fn line_comment_opener_terminates_directive() {
    let src = "x(); /* hector-disable: ns/a //trailing chatter */\n";
    let map = DisableMap::from_source(src);
    assert!(map.is_disabled(1, "ns/a"));
    assert!(!map.is_disabled(1, "//trailing"));
}
