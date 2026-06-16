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
mod tests {
    use super::*;
    use crate::{
        AssetScope, InMemoryAssetRegistry, LocalAssetStore, ProviderGenerationStatus,
        ProviderVideoJob, VideoContentItem, VideoExecutionConfig, VideoGenerationConfig,
        VideoTaskListQuery,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockVideoProvider {
        video_url: Option<String>,
        last_frame_url: Option<String>,
        create_status: ProviderGenerationStatus,
        terminal_status: ProviderGenerationStatus,
        polls_until_terminal: usize,
        polls: AtomicUsize,
    }

    #[async_trait]
    impl VideoProvider for MockVideoProvider {
        fn provider_name(&self) -> &str {
            "mock"
        }

        fn model_name(&self) -> &str {
            "mock-video"
        }

        async fn create_video_generation(
            &self,
            _request: &VideoGenerationRequest,
        ) -> Result<ProviderVideoJob, AigcError> {
            Ok(ProviderVideoJob {
                id: "job-1".into(),
                status: self.create_status.clone(),
                raw_status: None,
                video_url: None,
                last_frame_url: None,
                error: None,
                metadata: serde_json::json!({}),
            })
        }

        async fn get_video_generation(&self, _job_id: &str) -> Result<ProviderVideoJob, AigcError> {
            let n = self.polls.fetch_add(1, Ordering::SeqCst) + 1;
            let status = if n >= self.polls_until_terminal {
                self.terminal_status.clone()
            } else {
                ProviderGenerationStatus::Running
            };
            let is_terminal_success = status == ProviderGenerationStatus::Completed;
            Ok(ProviderVideoJob {
                id: "job-1".into(),
                status,
                raw_status: None,
                video_url: if is_terminal_success {
                    self.video_url.clone()
                } else {
                    None
                },
                last_frame_url: if is_terminal_success {
                    self.last_frame_url.clone()
                } else {
                    None
                },
                error: None,
                metadata: serde_json::json!({}),
            })
        }

        async fn cancel_video_generation(&self, _job_id: &str) -> Result<(), AigcError> {
            Ok(())
        }

        async fn list_video_generations(
            &self,
            _query: &VideoTaskListQuery,
        ) -> Result<(Vec<ProviderVideoJob>, u64), AigcError> {
            Ok((vec![], 0))
        }
    }

    fn video_request() -> VideoGenerationRequest {
        VideoGenerationRequest {
            content: vec![VideoContentItem::Text {
                text: "a cat playing piano".into(),
            }],
            generation_config: VideoGenerationConfig::default(),
            execution_config: VideoExecutionConfig {
                poll_interval: Some(Duration::from_millis(1)),
                timeout: Some(Duration::from_secs(5)),
            },
            provider_options: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn video_gateway_persists_provider_url_and_returns_controlled_url() {
        let dir = tempfile::tempdir().unwrap();
        let provider_video = dir.path().join("provider-video.mp4");
        std::fs::write(&provider_video, b"fake-mp4-bytes").unwrap();
        let provider_video_url = format!("file://{}", provider_video.display());

        let store_dir = tempfile::tempdir().unwrap();
        let gateway = VideoGateway::new(
            Arc::new(MockVideoProvider {
                video_url: Some(provider_video_url.clone()),
                last_frame_url: None,
                create_status: ProviderGenerationStatus::Running,
                terminal_status: ProviderGenerationStatus::Completed,
                polls_until_terminal: 2,
                polls: AtomicUsize::new(0),
            }),
            Arc::new(LocalAssetStore::new(
                store_dir.path(),
                Some("http://localhost/assets".into()),
            )),
            Arc::new(InMemoryAssetRegistry::default()),
            VideoGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
            },
        );

        let response = gateway.generate(video_request()).await.unwrap();

        assert_eq!(response.status, GenerationStatus::Completed);
        let video = response.video.expect("expected a video asset");
        assert!(!video.asset_id.is_empty());
        assert!(
            video.url.starts_with("http://localhost/assets/"),
            "expected our own controlled URL, got: {}",
            video.url
        );
        assert_ne!(video.url, provider_video_url);
    }

    #[tokio::test]
    async fn video_gateway_persists_last_frame_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let provider_video = dir.path().join("provider-video.mp4");
        let provider_frame = dir.path().join("provider-frame.png");
        std::fs::write(&provider_video, b"fake-mp4-bytes").unwrap();
        std::fs::write(&provider_frame, b"fake-png-bytes").unwrap();

        let store_dir = tempfile::tempdir().unwrap();
        let gateway = VideoGateway::new(
            Arc::new(MockVideoProvider {
                video_url: Some(format!("file://{}", provider_video.display())),
                last_frame_url: Some(format!("file://{}", provider_frame.display())),
                create_status: ProviderGenerationStatus::Running,
                terminal_status: ProviderGenerationStatus::Completed,
                polls_until_terminal: 1,
                polls: AtomicUsize::new(0),
            }),
            Arc::new(LocalAssetStore::new(store_dir.path(), None)),
            Arc::new(InMemoryAssetRegistry::default()),
            VideoGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
            },
        );

        let response = gateway.generate(video_request()).await.unwrap();

        assert!(response.video.is_some());
        assert!(response.last_frame.is_some());
    }

    #[tokio::test]
    async fn video_gateway_propagates_provider_failure() {
        let dir = tempfile::tempdir().unwrap();
        let gateway = VideoGateway::new(
            Arc::new(MockVideoProvider {
                video_url: None,
                last_frame_url: None,
                create_status: ProviderGenerationStatus::Failed,
                terminal_status: ProviderGenerationStatus::Failed,
                polls_until_terminal: 0,
                polls: AtomicUsize::new(0),
            }),
            Arc::new(LocalAssetStore::new(dir.path(), None)),
            Arc::new(InMemoryAssetRegistry::default()),
            VideoGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
            },
        );

        let err = gateway.generate(video_request()).await.unwrap_err();

        assert_eq!(err.code, "provider_generation_failed");
    }

    #[tokio::test]
    async fn video_gateway_times_out_when_never_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let gateway = VideoGateway::new(
            Arc::new(MockVideoProvider {
                video_url: Some("https://provider.example/video.mp4".into()),
                last_frame_url: None,
                create_status: ProviderGenerationStatus::Running,
                terminal_status: ProviderGenerationStatus::Running,
                polls_until_terminal: usize::MAX,
                polls: AtomicUsize::new(0),
            }),
            Arc::new(LocalAssetStore::new(dir.path(), None)),
            Arc::new(InMemoryAssetRegistry::default()),
            VideoGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
            },
        );
        let mut request = video_request();
        request.execution_config.timeout = Some(Duration::from_millis(10));
        request.execution_config.poll_interval = Some(Duration::from_millis(1));

        let err = gateway.generate(request).await.unwrap_err();

        assert_eq!(err.code, "provider_generation_timed_out");
    }
}
