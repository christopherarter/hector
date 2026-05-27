use hector_core::trust::fingerprint;

/// C1: the same semantic content in block-style and flow-style YAML
/// must hash identically. Pre-fix, serde_yaml's emitter sometimes
/// chose different scalar styles, producing different fingerprints.
#[test]
fn fingerprint_stable_across_yaml_styles() {
    let block = "schema_version: 2\nrules:\n  r:\n    description: \"x\"\n    engine: script\n    scope: [\"*\"]\n    severity: error\n    script: \"true\"\n";
    let flow = "{schema_version: 2, rules: {r: {description: \"x\", engine: script, scope: [\"*\"], severity: error, script: \"true\"}}}";
    let fp_block = fingerprint(block).expect("block");
    let fp_flow = fingerprint(flow).expect("flow");
    assert_eq!(
        fp_block, fp_flow,
        "semantic equality must yield same fingerprint"
    );
}

/// C1: unsupported YAML features (binary scalars, anchor references)
/// must error at fingerprint time with a clear message instead of
/// silently producing a fragile hash.
#[test]
fn fingerprint_rejects_anchor_reference() {
    let with_anchor = "schema_version: 2\nrules:\n  base: &b\n    description: \"x\"\n    engine: script\n    scope: [\"*\"]\n    severity: error\n    script: \"true\"\n  alias: *b\n";
    let result = fingerprint(with_anchor);
    assert!(result.is_err(), "anchors must be rejected; got {result:?}");
}
