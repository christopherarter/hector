use hector_core::config::scope::ScopeMatcher;

#[test]
fn single_glob_matches_any_depth() {
    let m = ScopeMatcher::new(&["*.py".to_string()]).unwrap();
    assert!(m.matches("foo.py"));
    assert!(m.matches("src/bar.py"));
    assert!(m.matches("src/nested/baz.py"));
    assert!(!m.matches("foo.rs"));
}

#[test]
fn pathed_glob_anchors() {
    let m = ScopeMatcher::new(&["src/**/*.ts".to_string()]).unwrap();
    assert!(m.matches("src/a.ts"));
    assert!(m.matches("src/nested/b.ts"));
    assert!(!m.matches("tests/a.ts"));
}

#[test]
fn list_of_globs() {
    let m = ScopeMatcher::new(&["*.php".to_string(), "*.blade.php".to_string()]).unwrap();
    assert!(m.matches("a.php"));
    assert!(m.matches("view.blade.php"));
    assert!(!m.matches("a.rs"));
}

#[test]
fn wildcard_matches_anything() {
    let m = ScopeMatcher::new(&["*".to_string()]).unwrap();
    assert!(m.matches("anything"));
    assert!(m.matches("nested/path/file.ext"));
}
