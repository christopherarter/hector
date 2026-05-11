use hector_core::config::{EngineKind, Rule, Severity};
use hector_core::llm::{LlmClient, RuleStatus};
use hector_core::llm::anthropic::AnthropicClient;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_semantic_rule() -> Rule {
    Rule {
        description: "useEffect should not derive state from props".into(),
        engine: EngineKind::Semantic,
        scope: vec!["*.tsx".into()],
        severity: Severity::Warning,
        script: None,
        pattern: None,
        language: None,
        context: None,
        capabilities: None,
        fix_hint: None,
    }
}

#[tokio::test]
async fn anthropic_evaluate_returns_pass_for_clean_diff() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{
                "type": "text",
                "text": "[{\"rule_id\":\"r1\",\"status\":\"pass\"}]"
            }]
        })))
        .mount(&server)
        .await;
    let base_url = server.uri();
    let rule = make_semantic_rule();
    let result = tokio::task::spawn_blocking(move || {
        let client = AnthropicClient::new("test-key", "claude-sonnet-4-6", Some(base_url));
        client.evaluate(&[("r1", &rule)], "diff text", None)
    }).await.unwrap();
    let verdicts = result.expect("evaluate");
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].status, RuleStatus::Pass);
}

#[tokio::test]
async fn anthropic_evaluate_returns_violation_with_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{
                "type": "text",
                "text": "[{\"rule_id\":\"r1\",\"status\":\"violation\",\"message\":\"useEffect derives state from props\",\"line\":12}]"
            }]
        })))
        .mount(&server)
        .await;
    let base_url = server.uri();
    let rule = make_semantic_rule();
    let result = tokio::task::spawn_blocking(move || {
        let client = AnthropicClient::new("test-key", "claude-sonnet-4-6", Some(base_url));
        client.evaluate(&[("r1", &rule)], "diff", None)
    }).await.unwrap();
    let verdicts = result.unwrap();
    match &verdicts[0].status {
        RuleStatus::Violation { message, line } => {
            assert!(message.contains("derives state from props"));
            assert_eq!(*line, Some(12));
        }
        _ => panic!("expected violation"),
    }
}
