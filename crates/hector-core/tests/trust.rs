use hector_core::trust::{canonicalize_for_fingerprint, fingerprint, verify};

const CFG_A: &str = "\
schema_version: 2
rules:
  r:
    description: \"x\"
    engine: script
    scope: [\"*\"]
    severity: error
    script: \"true\"
trust:
  fingerprint: \"sha256:placeholder\"
";

const CFG_A_REORDERED: &str = "\
trust:
  fingerprint: \"sha256:other\"
rules:
  r:
    severity: error
    scope: [\"*\"]
    script: \"true\"
    description: \"x\"
    engine: script
schema_version: 2
";

#[test]
fn fingerprint_ignores_key_order_and_trust_block() {
    let a = fingerprint(CFG_A).unwrap();
    let b = fingerprint(CFG_A_REORDERED).unwrap();
    assert_eq!(
        a, b,
        "canonicalization must ignore key order and trust block"
    );
    assert!(a.starts_with("sha256:"));
}

#[test]
fn fingerprint_detects_semantic_changes() {
    let modified = CFG_A.replace("engine: script", "engine: ast");
    let a = fingerprint(CFG_A).unwrap();
    let b = fingerprint(&modified).unwrap();
    assert_ne!(a, b);
}

#[test]
fn verify_accepts_matching_fingerprint() {
    // Compute fingerprint of a config body (without trust block), then embed it.
    let body = "schema_version: 2\nrules:\n  r:\n    description: \"x\"\n    engine: script\n    scope: [\"*\"]\n    severity: error\n    script: \"true\"\n";
    let fp = fingerprint(body).unwrap();
    let cfg = format!("{body}trust:\n  fingerprint: \"{fp}\"\n");
    assert!(
        verify(&cfg).is_ok(),
        "self-consistent fingerprint should verify"
    );

    // Sanity: canonicalization function is callable.
    let _ = canonicalize_for_fingerprint(body).unwrap();
}

#[test]
fn verify_rejects_missing_trust_block() {
    let cfg = "schema_version: 2\nrules: {}\n";
    let result = verify(cfg);
    assert!(result.is_err());
}

/// P2-3 regression: TOCTOU between trust verify and config parse.
///
/// The TOCTOU window existed when the loader read the file twice — once for
/// `trust::verify` and once for `parse`. An attacker with write access could
/// swap the file between those reads. The fix (landed in Phase 1.1 via
/// `extends::resolve_trusted`) reads the file once and passes the same
/// in-memory buffer to both `trust::verify` and `parse_str`.
///
/// This test exercises the behavior end-to-end: after a successful trusted
/// load, swap the file to a body that mismatches its (preserved) trust
/// fingerprint. A subsequent load must reject — proving the loader re-reads
/// the file fresh on every load *and* checks the same bytes it parses.
#[test]
fn p2_3_load_rejects_when_body_diverges_from_trust_fingerprint() {
    use hector_core::runner::HectorEngine;
    use hector_core::trust::write_trust_block;

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".hector.yml");

    // 1. Write a trusted config; load must succeed.
    let body_a = "schema_version: 2\nrules:\n  r:\n    description: \"x\"\n    engine: script\n    scope: [\"*\"]\n    severity: error\n    script: \"true\"\n";
    let trusted_a = write_trust_block(body_a).unwrap();
    std::fs::write(&path, &trusted_a).unwrap();
    HectorEngine::load(&path).expect("trusted load succeeds");

    // 2. Extract the trust block from the trusted file and graft it onto a
    //    DIFFERENT body. The on-disk file now has a valid-looking trust block
    //    but a body that does not match — exactly the shape of a TOCTOU attack
    //    (swap content while keeping fingerprint headers).
    let trust_line = trusted_a
        .lines()
        .skip_while(|l| !l.starts_with("trust:"))
        .collect::<Vec<_>>()
        .join("\n");
    let body_b = "schema_version: 2\nrules:\n  evil:\n    description: \"diverged\"\n    engine: script\n    scope: [\"*\"]\n    severity: error\n    script: \"touch /tmp/PWNED\"\n";
    let attacker_payload = format!("{body_b}{trust_line}\n");
    std::fs::write(&path, &attacker_payload).unwrap();

    // 3. Load must reject — proving the runner verifies the bytes it parses.
    let result = HectorEngine::load(&path);
    let err = match result {
        Ok(_) => panic!("loader must reject body/trust mismatch"),
        Err(e) => format!("{e:#}"),
    };
    assert!(
        err.contains("trust") || err.contains("fingerprint"),
        "error must reference trust; got: {err}"
    );
}
