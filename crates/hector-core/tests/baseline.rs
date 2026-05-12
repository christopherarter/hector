use hector_core::baseline::Baseline;
use hector_core::verdict::{Engine, Severity, Violation};
use tempfile::tempdir;

fn make_violation(rule_id: &str, file: &str, line: Option<u32>) -> Violation {
    Violation {
        rule_id: rule_id.to_string(),
        severity: Severity::Error,
        engine: Engine::Script,
        file: file.to_string(),
        line,
        column: None,
        message: "boom".to_string(),
        suggestion: None,
        context: None,
    }
}

#[test]
fn default_baseline_contains_nothing() {
    let b = Baseline::default();
    let v = make_violation("r1", "a.txt", Some(3));
    assert!(!b.contains(&v));
}

#[test]
fn add_then_contains_is_true() {
    let mut b = Baseline::default();
    let v = make_violation("r1", "a.txt", Some(3));
    b.add(&v);
    assert!(b.contains(&v));
}

#[test]
fn fingerprint_is_stable_for_identical_violations() {
    let v1 = make_violation("r1", "a.txt", Some(3));
    let mut v2 = make_violation("r1", "a.txt", Some(3));
    // Differ in fields the fingerprint must ignore.
    v2.message = "different message".to_string();
    v2.severity = Severity::Warning;
    v2.engine = Engine::Ast;
    v2.column = Some(99);
    v2.suggestion = Some("hint".to_string());
    v2.context = Some("ctx".to_string());
    assert_eq!(Baseline::fingerprint(&v1), Baseline::fingerprint(&v2));
}

#[test]
fn load_missing_path_returns_default() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("does_not_exist.json");
    let b = Baseline::load(&path).expect("missing path is OK");
    assert!(b.fingerprints.is_empty());
}

#[test]
fn save_creates_parent_dir() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".hector").join("baseline.json");
    assert!(!path.parent().unwrap().exists());
    let b = Baseline::default();
    b.save(&path).expect("save should create parent dir");
    assert!(path.exists());
}

#[test]
fn save_then_load_round_trip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".hector").join("baseline.json");
    let mut b = Baseline::default();
    let v1 = make_violation("rule-a", "a.txt", Some(1));
    let v2 = make_violation("rule-b", "b.txt", Some(2));
    let v3 = make_violation("rule-c", "c.txt", None);
    b.add(&v1);
    b.add(&v2);
    b.add(&v3);
    b.save(&path).unwrap();
    let loaded = Baseline::load(&path).unwrap();
    assert!(loaded.contains(&v1));
    assert!(loaded.contains(&v2));
    assert!(loaded.contains(&v3));
}

// P1-4 regression: the previous fingerprint formula was
// `{rule_id}::{file}::{line.unwrap_or(0)}`. With `rule_id="a::b" file="c"` and
// `rule_id="a" file="b::c"`, fingerprints collided because `::` is both the
// separator and a legal substring of either field. We now JSON-encode the
// tuple, which removes ambiguity for every input.
#[test]
fn fingerprint_distinguishes_separator_in_id_vs_file() {
    let v1 = make_violation("a::b", "c", Some(0));
    let v2 = make_violation("a", "b::c", Some(0));
    assert_ne!(
        Baseline::fingerprint(&v1),
        Baseline::fingerprint(&v2),
        "rule_id and file boundaries must not collapse"
    );
}

// P1-4: separator embedded in either field round-trips through save/load.
#[test]
fn fingerprint_with_separator_round_trips() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".hector").join("baseline.json");
    let mut b = Baseline::default();
    let v = make_violation("ns::rule", "weird::name.txt", Some(7));
    b.add(&v);
    b.save(&path).unwrap();
    let loaded = Baseline::load(&path).unwrap();
    assert!(loaded.contains(&v));
    // A near-miss with the boundary shifted by one char must NOT collide.
    let v_collide = make_violation("ns", "rule::weird::name.txt", Some(7));
    assert!(!loaded.contains(&v_collide));
}

// Note: line-None now serializes distinctly from line-Some(0) because the
// JSON encoding preserves the Option discriminant. This is a strict
// improvement over the prior collision behavior.
#[test]
fn line_none_distinct_from_line_zero() {
    let v_none = make_violation("r1", "a.txt", None);
    let v_zero = make_violation("r1", "a.txt", Some(0));
    assert_ne!(
        Baseline::fingerprint(&v_none),
        Baseline::fingerprint(&v_zero)
    );
}
