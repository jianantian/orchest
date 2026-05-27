use async_trait::async_trait;

use crate::{
    AigcError, ImageGenerationRequest, ImageModelCapabilities, ProviderImageEvent, ProviderImageJob,
};

#[async_trait]
pub trait ImageProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capabilities(&self) -> ImageModelCapabilities;

    async fn create_image_generation(
        &self,
        request: &ImageGenerationRequest,
    ) -> Result<ProviderImageJob, AigcError>;

    async fn get_image_generation(&self, job_id: &str) -> Result<ProviderImageJob, AigcError>;
}

pub type ImageEventSender = tokio::sync::mpsc::Sender<ProviderImageEvent>;
