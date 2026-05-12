use hector_core::config::{is_legacy, parse_file, parse_str, peek_schema_version};
use tempfile::tempdir;

const V2: &str = "schema_version: 2\nrules:\n  r:\n    description: \"x\"\n    engine: script\n    scope: [\"*\"]\n    severity: error\n    script: \"true\"\n";

#[test]
fn parse_file_reads_and_parses_a_valid_config() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".hector.yml");
    std::fs::write(&path, V2).unwrap();
    let cfg = parse_file(&path).expect("parse_file");
    assert_eq!(cfg.schema_version, 2);
}

#[test]
fn parse_file_surfaces_a_reading_error_for_missing_path() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("nope.yml");
    let err = parse_file(&missing).expect_err("missing file");
    let s = format!("{err:#}");
    assert!(s.contains("reading"), "context mentions reading: {s}");
}

#[test]
fn is_legacy_distinguishes_v1_from_v2() {
    let v1 = parse_str("schema_version: 1\nrules: {}\n").expect("parse v1");
    let v2 = parse_str(V2).expect("parse v2");
    assert!(is_legacy(&v1));
    assert!(!is_legacy(&v2));
}

#[test]
fn peek_schema_version_returns_the_top_level_integer() {
    assert_eq!(peek_schema_version("schema_version: 2\n"), Some(2));
    assert_eq!(peek_schema_version("schema_version: 1\n"), Some(1));
}

#[test]
fn peek_schema_version_returns_none_for_unparseable_yaml() {
    assert_eq!(peek_schema_version(": : :\n  oops\n"), None);
}

#[test]
fn peek_schema_version_returns_none_when_field_is_absent() {
    assert_eq!(peek_schema_version("rules: {}\n"), None);
}

#[test]
fn peek_schema_version_returns_none_when_field_is_not_an_integer() {
    assert_eq!(peek_schema_version("schema_version: \"two\"\n"), None);
}
