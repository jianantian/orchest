pub mod anthropic;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::tool::ToolDef;

#[async_trait]
pub trait ModelAdapter: Send + Sync {
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<ModelResponse, ModelError>;

    async fn call(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<ModelResponse, ModelError> {
        let (tx, _rx) = mpsc::channel(64);
        self.stream(messages, tools, tx).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelStreamChunk {
    Text { delta: String },
    ThinkingStart,
    Thinking { delta: String },
    ThinkingEnd,
    ToolCallArgsChunk { id: String, delta: String },
    Done { usage: TokenUsage },
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ModelError {
    pub message: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub provider: String,
    pub model: String,
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_url: Option<String>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub content: Vec<ContentBlock>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}
