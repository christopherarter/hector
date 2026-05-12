use hector_core::config::{EngineKind, Rule, Severity};
use hector_core::engine::ast::AstEngine;
use hector_core::engine::{RuleContext, RuleEngine};
use tempfile::tempdir;

fn make_ast_rule(pattern: &str, language: &str) -> Rule {
    Rule {
        description: "test ast rule".into(),
        engine: EngineKind::Ast,
        scope: vec!["*.ts".into()],
        severity: Severity::Error,
        script: None,
        pattern: Some(pattern.into()),
        language: Some(language.into()),
        context: None,
        capabilities: None,
        fix_hint: None,
    }
}

#[test]
fn ast_engine_matches_pattern() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.ts");
    let content = "const x = y as any;\nconst z = 5;\n";
    std::fs::write(&file, content).unwrap();
    let rule = make_ast_rule("$EXPR as any", "TypeScript");
    let ctx = RuleContext {
        rule_id: "no-as-any",
        rule: &rule,
        file: &file,
        content: Some(content),
        diff: None,
        cwd: dir.path(),
        llm: None,
    };
    let engine = AstEngine;
    let outcome = engine.run(&ctx).expect("run");
    let v = outcome.expect("violation expected");
    assert_eq!(v.rule_id, "no-as-any");
    assert_eq!(v.engine, hector_core::verdict::Engine::Ast);
    assert_eq!(v.line, Some(1));
}

#[test]
fn ast_engine_no_match_no_violation() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.ts");
    let content = "const x = 1;\n";
    std::fs::write(&file, content).unwrap();
    let rule = make_ast_rule("$EXPR as any", "TypeScript");
    let ctx = RuleContext {
        rule_id: "no-as-any",
        rule: &rule,
        file: &file,
        content: Some(content),
        diff: None,
        cwd: dir.path(),
        llm: None,
    };
    let engine = AstEngine;
    let outcome = engine.run(&ctx).expect("run");
    assert!(outcome.is_none());
}

#[test]
fn ast_violation_populates_column_and_context() {
    // P1-3: AST violations must populate `column` and `context` (the verdict
    // shape defines both; AST has the data via `start_pos().column()` and we
    // can synthesize a ±N-line window around the match).
    let rule = Rule {
        description: "x".into(),
        engine: EngineKind::Ast,
        scope: vec!["**/*.rs".into()],
        severity: Severity::Warning,
        script: None,
        pattern: Some("$E.unwrap()".into()),
        language: Some("rust".into()),
        context: None,
        capabilities: None,
        fix_hint: None,
    };
    let content = "fn a() {\n    foo();\n    bar.unwrap();\n    baz();\n}\n";
    let ctx = RuleContext {
        rule_id: "no-unwrap",
        rule: &rule,
        file: std::path::Path::new("test.rs"),
        content: Some(content),
        diff: None,
        cwd: std::path::Path::new("."),
        llm: None,
    };
    let v = AstEngine.run(&ctx).expect("run").expect("violation expected");
    assert!(v.column.is_some(), "column must be populated for ast");
    let ctxstr = v.context.expect("context must be populated for ast");
    assert!(
        ctxstr.contains("foo();") && ctxstr.contains("bar.unwrap();") && ctxstr.contains("baz();"),
        "context should include surrounding ±N lines: {ctxstr}"
    );
}
