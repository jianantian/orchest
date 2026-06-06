//! ModelAdapter trait: async LLM completion interface.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::error::ModelError;
use crate::options::{ModelCapabilities, RequestOptions};
use crate::response::ModelResponse;
use crate::stream::StreamEvent;
use crate::types::{Message, ToolDef};

/// Provider-agnostic interface to a chat model. Implementations map a unified
/// request (messages, tools, options) to a provider API and stream
/// `StreamEvent`s. Anthropic/OpenAI/DeepSeek/OpenRouter adapters live in
/// `agent-runtime-providers`.
#[async_trait]
pub trait ModelAdapter: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capabilities(&self) -> ModelCapabilities;

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError>;
}
