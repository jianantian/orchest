//! Deterministic scripted `ModelAdapter` for offline eval tests.
//!
//! Not a product path — only used by the eval runner when injected, or by
//! integration tests. Live eval run constructs the real model from env.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use orchest::model::ModelAdapter;
use orchest_protocol::{
    ContentBlock, Message, ModelCapabilities, ModelError, ModelResponse, RequestOptions,
    StopReason, StreamEvent, TokenUsage, ToolDef,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// One scripted model turn.
#[derive(Debug, Clone)]
pub enum ScriptedTurn {
    /// Emit a tool-use response (stop_reason = ToolUse).
    Tool {
        name: String,
        input: Value,
        usage: TokenUsage,
    },
    /// Emit a final text answer.
    Text { text: String, usage: TokenUsage },
    /// Fail the model call.
    Error { message: String },
}

impl ScriptedTurn {
    pub fn tool(name: impl Into<String>, input: Value) -> Self {
        Self::Tool {
            name: name.into(),
            input,
            usage: TokenUsage {
                input_tokens: 20,
                output_tokens: 10,
                ..Default::default()
            },
        }
    }

    pub fn tool_with_usage(name: impl Into<String>, input: Value, usage: TokenUsage) -> Self {
        Self::Tool {
            name: name.into(),
            input,
            usage,
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            usage: TokenUsage {
                input_tokens: 15,
                output_tokens: 25,
                ..Default::default()
            },
        }
    }

    pub fn text_with_usage(text: impl Into<String>, usage: TokenUsage) -> Self {
        Self::Text {
            text: text.into(),
            usage,
        }
    }
}

/// Queue-driven model used by eval tests.
pub struct ScriptedModel {
    provider: String,
    model: String,
    turns: Mutex<VecDeque<ScriptedTurn>>,
    /// When the queue is empty, respond with this text (or error if None).
    fallback_text: Option<String>,
    call_count: Mutex<u64>,
}

impl ScriptedModel {
    pub fn new(turns: Vec<ScriptedTurn>) -> Self {
        Self {
            provider: "scripted".into(),
            model: "eval-script".into(),
            turns: Mutex::new(turns.into()),
            fallback_text: Some("scripted fallback answer".into()),
            call_count: Mutex::new(0),
        }
    }

    pub fn with_identity(mut self, provider: impl Into<String>, model: impl Into<String>) -> Self {
        self.provider = provider.into();
        self.model = model.into();
        self
    }

    pub fn without_fallback(mut self) -> Self {
        self.fallback_text = None;
        self
    }

    pub fn call_count(&self) -> u64 {
        *self.call_count.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A minimal successful briefing chain: search → read → review → write → text.
    pub fn briefing_happy_path(report_markdown: &str) -> Self {
        let turns = vec![
            ScriptedTurn::tool(
                "search_fixtures",
                json!({"query": "retention investment Q4"}),
            ),
            ScriptedTurn::tool(
                "read_fixture",
                json!({"path": "fixtures/research/001-retention-dashboard-notes.md"}),
            ),
            ScriptedTurn::tool("review_report", json!({"draft": report_markdown})),
            ScriptedTurn::tool("write_report", json!({"content": report_markdown})),
            ScriptedTurn::text(report_markdown),
        ];
        Self::new(turns)
    }

    /// Follow-up path: answer from history without tools.
    pub fn followup_text(answer: &str) -> Self {
        Self::new(vec![ScriptedTurn::text(answer)])
    }
}

#[async_trait]
impl ModelAdapter for ScriptedModel {
    fn provider_name(&self) -> &str {
        &self.provider
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: false,
            ..ModelCapabilities::default()
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        {
            let mut c = self.call_count.lock().unwrap_or_else(|e| e.into_inner());
            *c = c.saturating_add(1);
        }

        let turn = {
            let mut q = self.turns.lock().unwrap_or_else(|e| e.into_inner());
            q.pop_front()
        };

        let response = match turn {
            Some(ScriptedTurn::Tool { name, input, usage }) => {
                let id = format!("call_{}", self.call_count());
                ModelResponse {
                    content: vec![ContentBlock::ToolUse { id, name, input }],
                    usage,
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                }
            }
            Some(ScriptedTurn::Text { text, usage }) => ModelResponse {
                content: vec![ContentBlock::Text(text)],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            },
            Some(ScriptedTurn::Error { message }) => {
                return Err(ModelError {
                    message,
                    code: Some("scripted_error".into()),
                    provider: Some(self.provider.clone()),
                    status: None,
                    retry_after_secs: None,
                    upstream: None,
                });
            }
            None => {
                let has_tool_result = messages.iter().any(|m| {
                    m.content
                        .iter()
                        .any(|c| matches!(c, ContentBlock::ToolResult { .. }))
                });
                let text = if has_tool_result {
                    self.fallback_text.clone().unwrap_or_else(|| "done".into())
                } else if let Some(t) = &self.fallback_text {
                    t.clone()
                } else {
                    return Err(ModelError {
                        message: "scripted model queue empty".into(),
                        code: Some("scripted_exhausted".into()),
                        provider: Some(self.provider.clone()),
                        status: None,
                        retry_after_secs: None,
                        upstream: None,
                    });
                };
                ModelResponse {
                    content: vec![ContentBlock::Text(text)],
                    usage: TokenUsage {
                        input_tokens: 10,
                        output_tokens: 10,
                        ..Default::default()
                    },
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                }
            }
        };

        if let Some(tx) = tx {
            if let ContentBlock::Text(t) = &response.content[0] {
                let _ = tx.send(StreamEvent::Text { delta: t.clone() }).await;
            }
            let _ = tx
                .send(StreamEvent::Done {
                    usage: response.usage.clone(),
                })
                .await;
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scripted_emits_tool_then_text() {
        let model = ScriptedModel::new(vec![
            ScriptedTurn::tool("search_fixtures", json!({"query": "x"})),
            ScriptedTurn::text("done"),
        ]);
        let r1 = model
            .complete(&[], &[], &RequestOptions::default(), None)
            .await
            .unwrap();
        assert!(matches!(r1.content[0], ContentBlock::ToolUse { .. }));
        let r2 = model
            .complete(&[], &[], &RequestOptions::default(), None)
            .await
            .unwrap();
        assert!(matches!(r2.content[0], ContentBlock::Text(_)));
    }
}
