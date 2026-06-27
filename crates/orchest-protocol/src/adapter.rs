//! `ChatModel` trait: async chat/turn completion interface.
//!
//! Renamed from `ModelAdapter` in v0.9.12 (provider unification). The signature
//! is **unchanged** — existing providers `impl ModelAdapter for X` keep compiling
//! through the deprecated `ModelAdapter` alias (see `lib.rs`). The only addition
//! is a defaulted, spine-native [`ChatModel::descriptor`]; the push→pull
//! convergence (a pulled `events()` and `ProtocolError`) is owned by Issue 005.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::descriptor::{Capability, CapabilityDescriptor, CapabilityExt, ChatCapabilityExt};
use crate::error::ModelError;
use crate::options::{ModelCapabilities, RequestOptions};
use crate::response::ModelResponse;
use crate::stream::StreamEvent;
use crate::types::{Message, ToolDef};

/// Provider-agnostic interface to a chat model. Implementations map a unified
/// request (messages, tools, options) to a provider API and stream
/// `StreamEvent`s. Anthropic/OpenAI/DeepSeek/OpenRouter adapters live in
/// `agent-runtime-providers` (moving behind the wall in Issue 005).
#[async_trait]
pub trait ChatModel: Send + Sync {
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

    /// Spine-native capability descriptor. The default derives the queryable
    /// core from `capabilities()` (no modality info — `ModelCapabilities` lacks
    /// it); providers override in Issue 005 to fold in the catalog's
    /// input/output modalities. Lets the registry (Issue 004) treat chat models
    /// uniformly with the other capabilities.
    fn descriptor(&self) -> CapabilityDescriptor {
        let caps = self.capabilities();
        CapabilityDescriptor::new(
            self.provider_name().to_string(),
            self.model_name().to_string(),
            Capability::Chat,
        )
        .streaming(caps.streaming)
        .tools(caps.tool_use)
        .thinking(caps.reasoning.supported)
        .with_source(caps.source)
        .with_ext(CapabilityExt::Chat(ChatCapabilityExt {
            parallel_tool_use: caps.parallel_tool_use,
            reasoning: caps.reasoning,
            prompt_cache: caps.prompt_cache,
            max_output_tokens: caps.max_output_tokens,
            context_window_size: caps.context_window_size,
            pricing: caps.pricing,
        }))
    }
}
