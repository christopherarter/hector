use hector_core::verdict::{Engine, Severity, Status, Verdict, Violation};

#[test]
fn verdict_status_internal_error_when_engine_fails() {
    let v = Verdict::from_violations(
        vec![Violation {
            rule_id: "r__internal".to_string(),
            severity: Severity::Error,
            engine: Engine::Internal,
            file: "f".into(),
            line: None,
            column: None,
            message: "ANTHROPIC_API_KEY missing".into(),
            suggestion: None,
            context: None,
        }],
        vec![],
        0,
    );
    assert_eq!(v.status, Status::InternalError);
}

#[test]
fn verdict_internal_error_takes_precedence_over_policy_block() {
    // A mix of Internal and real policy errors still resolves to
    // InternalError so the adapter sees "the gate is broken" first.
    let v = Verdict::from_violations(
        vec![
            Violation {
                rule_id: "r1__internal".to_string(),
                severity: Severity::Error,
                engine: Engine::Internal,
                file: "a".into(),
                line: None,
                column: None,
                message: "x".into(),
                suggestion: None,
                context: None,
            },
            Violation {
                rule_id: "r2".to_string(),
                severity: Severity::Error,
                engine: Engine::Script,
                file: "b".into(),
                line: Some(1),
                column: None,
                message: "policy".into(),
                suggestion: None,
                context: None,
            },
        ],
        vec![],
        0,
    );
    assert_eq!(v.status, Status::InternalError);
}
