use async_trait::async_trait;

use crate::{AigcError, ProviderVideoJob, VideoGenerationRequest, VideoTaskListQuery};

/// Video generation is asynchronous on every known provider: `create_video_generation`
/// returns a task immediately (typically `Queued` or `Running`), and callers must poll
/// `get_video_generation` until the job reaches a terminal status.
#[async_trait]
pub trait VideoProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;

    async fn create_video_generation(
        &self,
        request: &VideoGenerationRequest,
    ) -> Result<ProviderVideoJob, AigcError>;

    async fn get_video_generation(&self, job_id: &str) -> Result<ProviderVideoJob, AigcError>;

    async fn cancel_video_generation(&self, job_id: &str) -> Result<(), AigcError>;

    async fn list_video_generations(
        &self,
        query: &VideoTaskListQuery,
    ) -> Result<(Vec<ProviderVideoJob>, u64), AigcError>;
}
