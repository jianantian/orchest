use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{
    resolve_asset_url, AigcError, AssetIngestSource, AssetRegistry, AssetScope, AssetStore,
    GenerationStatus, ImageUrlOutput, ProviderGenerationStatus, PutAssetOptions,
};
use crate::{GeneratedVideoAsset, VideoGenerationRequest, VideoGenerationResponse, VideoProvider};

use super::{persist_provider_asset, AssetStorageContext};

/// Wraps a [`VideoProvider`] the same way [`super::ImageGateway`] wraps an
/// `ImageProvider`: the provider's own video URL is short-lived (Volcengine's
/// expire after 24h), so the gateway downloads it and persists it to our own
/// [`AssetStore`] before handing back a controlled URL. Unlike images, video
/// generation is inherently async on every known provider, so `generate` always
/// polls internally until the task reaches a terminal status.
pub struct VideoGateway {
    provider: Arc<dyn VideoProvider>,
    asset_store: Arc<dyn AssetStore>,
    asset_registry: Arc<dyn AssetRegistry>,
    config: VideoGatewayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGatewayConfig {
    pub scope: AssetScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_url_ttl: Option<Duration>,
}

impl VideoGateway {
    pub fn new(
        provider: Arc<dyn VideoProvider>,
        asset_store: Arc<dyn AssetStore>,
        asset_registry: Arc<dyn AssetRegistry>,
        config: VideoGatewayConfig,
    ) -> Self {
        Self {
            provider,
            asset_store,
            asset_registry,
            config,
        }
    }

    #[allow(clippy::result_large_err)] // justified: AigcError carries diagnostic context needed for user-facing messages
    pub async fn generate(
        &self,
        request: VideoGenerationRequest,
    ) -> Result<VideoGenerationResponse, AigcError> {
        let provider_started_at = std::time::Instant::now();
        let provider_job = self.provider.create_video_generation(&request).await?;
        let job = self.wait_for_completion(provider_job, &request).await?;
        metrics::histogram!(
            crate::telemetry::METRIC_PROVIDER_DURATION,
            "provider" => self.provider.provider_name().to_string()
        )
        .record(provider_started_at.elapsed().as_secs_f64());

        if matches!(job.status, ProviderGenerationStatus::Failed) {
            metrics::counter!(
                crate::telemetry::METRIC_ERROR_COUNT,
                "provider" => self.provider.provider_name().to_string(),
                "code" => "provider_generation_failed"
            )
            .increment(1);
            return Err(AigcError::new(
                "provider_generation_failed",
                job.error.unwrap_or_else(|| {
                    format!(
                        "provider video generation did not complete (status: {:?})",
                        job.raw_status
                    )
                }),
            )
            .provider(self.provider.provider_name()));
        }
        if matches!(job.status, ProviderGenerationStatus::TimedOut) {
            metrics::counter!(
                crate::telemetry::METRIC_ERROR_COUNT,
                "provider" => self.provider.provider_name().to_string(),
                "code" => "provider_generation_timed_out"
            )
            .increment(1);
            return Err(AigcError::new(
                "provider_generation_timed_out",
                "provider video generation timed out",
            )
            .provider(self.provider.provider_name()));
        }

        let video = match &job.video_url {
            Some(url) => Some(self.persist_video_asset(url, "video/mp4", "videos").await?),
            None => None,
        };
        let last_frame = match &job.last_frame_url {
            Some(url) => Some(
                self.persist_video_asset(url, "image/png", "video-frames")
                    .await?,
            ),
            None => None,
        };

        Ok(VideoGenerationResponse {
            job_id: job.id,
            status: GenerationStatus::Completed,
            video,
            last_frame,
            error: None,
        })
    }

    async fn persist_video_asset(
        &self,
        provider_url: &str,
        content_type_hint: &str,
        key_prefix: &str,
    ) -> Result<GeneratedVideoAsset, AigcError> {
        let stored = persist_provider_asset(
            AssetStorageContext {
                asset_store: self.asset_store.as_ref(),
                asset_registry: self.asset_registry.as_ref(),
                scope: &self.config.scope,
                provider_name: self.provider.provider_name(),
            },
            AssetIngestSource::Url(provider_url.to_string()),
            PutAssetOptions {
                content_type_hint: Some(content_type_hint.into()),
                key_prefix: Some(key_prefix.into()),
                ..Default::default()
            },
        )
        .await?;
        let access = self
            .asset_store
            .signed_url(&stored, self.config.signed_url_ttl)
            .await?;
        Ok(GeneratedVideoAsset {
            asset_id: stored.asset_id,
            url: access.url,
            expires_at: access.expires_at,
            mime_type: Some(stored.content_type),
        })
    }

    #[allow(clippy::result_large_err)] // justified: same AigcError used throughout for consistency
    pub async fn resolve_asset_url(
        &self,
        asset_id: &str,
        ttl: Option<Duration>,
    ) -> Result<ImageUrlOutput, AigcError> {
        Ok(resolve_asset_url(
            self.asset_registry.as_ref(),
            self.asset_store.as_ref(),
            &self.config.scope,
            asset_id,
            ttl.or(self.config.signed_url_ttl),
        )
        .await?)
    }

    async fn wait_for_completion(
        &self,
        mut job: crate::ProviderVideoJob,
        request: &VideoGenerationRequest,
    ) -> Result<crate::ProviderVideoJob, AigcError> {
        let started_at = std::time::Instant::now();
        let timeout = request
            .execution_config
            .timeout
            .unwrap_or_else(|| Duration::from_secs(600));
        let poll_interval = request
            .execution_config
            .poll_interval
            .unwrap_or_else(|| Duration::from_secs(5));
        loop {
            match job.status {
                ProviderGenerationStatus::Completed
                | ProviderGenerationStatus::Failed
                | ProviderGenerationStatus::TimedOut => return Ok(job),
                ProviderGenerationStatus::Queued | ProviderGenerationStatus::Running => {
                    if started_at.elapsed() >= timeout {
                        job.status = ProviderGenerationStatus::TimedOut;
                        return Ok(job);
                    }
                    let _span = crate::telemetry::provider_poll_span(
                        self.provider.provider_name(),
                        self.provider.model_name(),
                    )
                    .entered();
                    job = self.provider.get_video_generation(&job.id).await?;
                    if matches!(
                        job.status,
                        ProviderGenerationStatus::Queued | ProviderGenerationStatus::Running
                    ) {
                        tokio::time::sleep(poll_interval).await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
