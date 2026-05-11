use super::{LlmClient, RuleStatus, RuleVerdict};
use crate::config::Rule;
use crate::llm::prompt::build_prompt;
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

pub struct AnthropicClient {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::blocking::Client,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
            client: reqwest::blocking::Client::new(),
        }
    }

    pub fn from_env(model: &str) -> Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY not set")?;
        Ok(Self::new(key, model, None))
    }
}

#[derive(Debug, Deserialize)]
struct Message {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

impl LlmClient for AnthropicClient {
    fn evaluate(
        &self,
        rules: &[(&str, &Rule)],
        primary: &str,
        context: Option<&str>,
    ) -> Result<Vec<RuleVerdict>> {
        let prompt = build_prompt(rules, primary, context);
        let url = format!("{}/v1/messages", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let response = self.client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .context("anthropic request")?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            return Err(anyhow!("anthropic returned {status}: {text}"));
        }
        let message: Message = response.json().context("parse anthropic response")?;
        let text = message.content.iter()
            .find(|b| b.block_type == "text")
            .and_then(|b| b.text.as_ref())
            .ok_or_else(|| anyhow!("anthropic response missing text content"))?;
        parse_verdicts(text)
    }
}

#[derive(Debug, Deserialize)]
struct WireVerdict {
    rule_id: String,
    status: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    line: Option<u32>,
}

fn parse_verdicts(text: &str) -> Result<Vec<RuleVerdict>> {
    // Extract the first JSON array in the response (LLM may wrap in markdown).
    let trimmed = text.trim();
    let start = trimmed.find('[').ok_or_else(|| anyhow!("no JSON array in response: {trimmed}"))?;
    let end = trimmed.rfind(']').ok_or_else(|| anyhow!("no closing bracket: {trimmed}"))?;
    let json = &trimmed[start..=end];
    let wire: Vec<WireVerdict> = serde_json::from_str(json)
        .with_context(|| format!("parse anthropic verdict JSON: {json}"))?;
    Ok(wire.into_iter().map(|w| RuleVerdict {
        rule_id: w.rule_id,
        status: match w.status.as_str() {
            "pass" => RuleStatus::Pass,
            "violation" => RuleStatus::Violation {
                message: w.message.unwrap_or_default(),
                line: w.line,
            },
            other => RuleStatus::Violation {
                message: format!("unknown status from LLM: {other}"),
                line: None,
            },
        },
    }).collect())
}
