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
