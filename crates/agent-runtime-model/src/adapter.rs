use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::error::ModelError;
use crate::options::{ModelCapabilities, RequestOptions};
use crate::response::ModelResponse;
use crate::stream::StreamEvent;
use crate::types::{Message, ToolDef};

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
