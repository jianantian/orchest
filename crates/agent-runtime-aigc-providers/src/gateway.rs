use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    resolve_asset_url, AigcError, AssetIngestSource, AssetRef, AssetRegistry, AssetScope,
    AssetStore, CompatibilityPolicy, GenerationStatus, ImageBackground, ImageBase64Output,
    ImageGenerationEvent, ImageGenerationRequest, ImageGenerationResponse, ImageInput,
    ImageOperation, ImageOperationCapability, ImageOutput, ImageOutputDelivery, ImageUrlOutput,
    OptionAdjustment, ProviderAsset, ProviderGenerationStatus, ProviderImageEvent, PutAssetOptions,
    StoredAsset,
};
use crate::{GeneratedImage, ImageProvider};
use crate::{GeneratedVideoAsset, VideoGenerationRequest, VideoGenerationResponse, VideoProvider};

pub struct ImageGateway {
    provider: Arc<dyn ImageProvider>,
    asset_store: Arc<dyn AssetStore>,
    asset_registry: Arc<dyn AssetRegistry>,
    config: ImageGatewayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGatewayConfig {
    pub scope: AssetScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_url_ttl: Option<Duration>,
    #[serde(default)]
    pub max_base64_bytes: Option<u64>,
    #[serde(default)]
    pub emit_partial_images: bool,
}

impl ImageGateway {
    pub fn new(
        provider: Arc<dyn ImageProvider>,
        asset_store: Arc<dyn AssetStore>,
        asset_registry: Arc<dyn AssetRegistry>,
        config: ImageGatewayConfig,
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
        request: ImageGenerationRequest,
    ) -> Result<ImageGenerationResponse, AigcError> {
        self.generate_inner(request, None).await
    }

    #[allow(clippy::result_large_err)] // justified: same AigcError used throughout for consistency
    pub async fn generate_with_events(
        &self,
        request: ImageGenerationRequest,
        events: tokio::sync::mpsc::Sender<ImageGenerationEvent>,
    ) -> Result<ImageGenerationResponse, AigcError> {
        self.generate_inner(request, Some(events)).await
    }

    async fn generate_inner(
        &self,
        mut request: ImageGenerationRequest,
        events: Option<tokio::sync::mpsc::Sender<ImageGenerationEvent>>,
    ) -> Result<ImageGenerationResponse, AigcError> {
        let _span = crate::telemetry::image_create_span(
            self.provider.provider_name(),
            self.provider.model_name(),
        )
        .entered();
        let mut option_adjustments = self.validate_and_coerce_request(&mut request)?;
        self.preprocess_inputs(&mut request).await?;
        let provider_started_at = std::time::Instant::now();
        let provider_job = self.provider.create_image_generation(&request).await?;
        let provider_job = self
            .wait_for_completion(provider_job, &request, events.as_ref())
            .await?;
        metrics::histogram!(
            crate::telemetry::METRIC_PROVIDER_DURATION,
            "provider" => self.provider.provider_name().to_string()
        )
        .record(provider_started_at.elapsed().as_secs_f64());
        if matches!(provider_job.status, ProviderGenerationStatus::Failed) {
            metrics::counter!(
                crate::telemetry::METRIC_ERROR_COUNT,
                "provider" => self.provider.provider_name().to_string(),
                "code" => "provider_generation_failed"
            )
            .increment(1);
            return Err(AigcError::new(
                "provider_generation_failed",
                "provider generation did not complete",
            )
            .provider(self.provider.provider_name()));
        }
        if matches!(provider_job.status, ProviderGenerationStatus::TimedOut) {
            metrics::counter!(
                crate::telemetry::METRIC_ERROR_COUNT,
                "provider" => self.provider.provider_name().to_string(),
                "code" => "provider_generation_timed_out"
            )
            .increment(1);
            return Err(AigcError::new(
                "provider_generation_timed_out",
                "provider generation timed out",
            )
            .provider(self.provider.provider_name()));
        }
        self.emit_status(events.as_ref(), GenerationStatus::PersistingAssets)
            .await;

        let mut images = Vec::with_capacity(provider_job.assets.len());
        for asset in provider_job.assets {
            let persist_started_at = std::time::Instant::now();
            let stored = self
                .asset_store
                .put_stream(
                    asset.source,
                    PutAssetOptions {
                        content_type_hint: asset.mime_type.clone(),
                        max_base64_bytes: self.config.max_base64_bytes,
                        ..Default::default()
                    },
                )
                .await?;
            metrics::histogram!(
                crate::telemetry::METRIC_ASSET_PERSIST_DURATION,
                "provider" => self.provider.provider_name().to_string()
            )
            .record(persist_started_at.elapsed().as_secs_f64());
            metrics::counter!(
                crate::telemetry::METRIC_PERSISTED_BYTES,
                "provider" => self.provider.provider_name().to_string()
            )
            .increment(stored.byte_count);
            self.asset_registry
                .save(&self.config.scope, stored.clone())
                .await?;
            images.push(
                self.public_image(stored, &request.output_config.delivery)
                    .await?,
            );
        }
        option_adjustments.extend(provider_job.option_adjustments);

        let response = ImageGenerationResponse {
            job_id: provider_job.id,
            status: GenerationStatus::Completed,
            images,
            option_adjustments,
            usage: provider_job.usage,
        };
        metrics::counter!(
            crate::telemetry::METRIC_GENERATED_IMAGE_COUNT,
            "provider" => self.provider.provider_name().to_string()
        )
        .increment(response.images.len() as u64);
        if let Some(sender) = events.as_ref() {
            let _ = sender
                .send(ImageGenerationEvent::Completed {
                    response: response.clone(),
                })
                .await;
        }
        Ok(response)
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

    async fn public_image(
        &self,
        stored: StoredAsset,
        delivery: &ImageOutputDelivery,
    ) -> Result<GeneratedImage, AigcError> {
        let asset_id = stored.asset_id.clone();
        let mime_type = Some(stored.content_type.clone());
        let output = match delivery {
            ImageOutputDelivery::Url => {
                let url = self
                    .asset_store
                    .signed_url(&stored, self.config.signed_url_ttl)
                    .await?;
                ImageOutput::Url(ImageUrlOutput {
                    url: url.url,
                    expires_at: url.expires_at,
                })
            }
            ImageOutputDelivery::Base64 => {
                let bytes = self.asset_store.get_bytes(&stored).await?;
                let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                ImageOutput::Base64(ImageBase64Output {
                    data,
                    mime_type: stored.content_type,
                })
            }
        };
        Ok(GeneratedImage {
            asset_id,
            output,
            mime_type,
            width: None,
            height: None,
        })
    }

    async fn preprocess_inputs(
        &self,
        request: &mut ImageGenerationRequest,
    ) -> Result<(), AigcError> {
        let mut processed = Vec::with_capacity(request.inputs.len());
        for input in request.inputs.drain(..) {
            processed.push(self.preprocess_input(input).await?);
        }
        request.inputs = processed;
        Ok(())
    }

    async fn preprocess_input(&self, input: ImageInput) -> Result<ImageInput, AigcError> {
        let asset = match input.asset {
            AssetRef::Stored { asset_id } => {
                let url = self
                    .resolve_asset_url(&asset_id, self.config.signed_url_ttl)
                    .await?;
                AssetRef::Url(url.url)
            }
            AssetRef::LocalPath(path) => {
                let bytes = tokio::fs::read(&path)
                    .await
                    .map_err(|err| AigcError::new("input_read_failed", err.to_string()))?;
                let stored = self
                    .asset_store
                    .put_stream(
                        AssetIngestSource::Bytes {
                            bytes: Bytes::from(bytes),
                            mime_type: input
                                .mime_type
                                .clone()
                                .unwrap_or_else(|| "application/octet-stream".into()),
                        },
                        PutAssetOptions::default(),
                    )
                    .await?;
                self.asset_registry
                    .save(&self.config.scope, stored.clone())
                    .await?;
                let url = self
                    .asset_store
                    .signed_url(&stored, self.config.signed_url_ttl)
                    .await?;
                AssetRef::Url(url.url)
            }
            other => other,
        };
        Ok(ImageInput { asset, ..input })
    }

    async fn wait_for_completion(
        &self,
        mut job: crate::ProviderImageJob,
        request: &ImageGenerationRequest,
        events: Option<&tokio::sync::mpsc::Sender<ImageGenerationEvent>>,
    ) -> Result<crate::ProviderImageJob, AigcError> {
        let started_at = std::time::Instant::now();
        let timeout = request
            .execution_config
            .timeout
            .unwrap_or_else(|| Duration::from_secs(120));
        let poll_interval = request
            .execution_config
            .poll_interval
            .unwrap_or_else(|| Duration::from_millis(500));
        loop {
            self.drain_provider_events(events, &job.events).await?;
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
                    job = self.provider.get_image_generation(&job.id).await?;
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

    fn validate_and_coerce_request(
        &self,
        request: &mut ImageGenerationRequest,
    ) -> Result<Vec<OptionAdjustment>, AigcError> {
        let capabilities = self.provider.capabilities();
        if capabilities.operations.is_empty() {
            return Ok(vec![]);
        }
        let capability = find_operation_capability(&capabilities.operations, &request.operation)
            .ok_or_else(|| {
                AigcError::new(
                    "unsupported_operation",
                    format!(
                        "{} does not advertise support for {:?}",
                        self.provider.provider_name(),
                        request.operation
                    ),
                )
                .provider(self.provider.provider_name())
            })?;
        let mut adjustments = Vec::new();
        if let Some(max_outputs) = capability.max_outputs {
            if let Some(count) = request.generation_config.count {
                if count > max_outputs {
                    if request.compatibility_policy == CompatibilityPolicy::Strict {
                        return Err(AigcError::new(
                            "unsupported_option",
                            format!("count {count} exceeds max_outputs {max_outputs}"),
                        )
                        .provider(self.provider.provider_name()));
                    }
                    request.generation_config.count = Some(max_outputs);
                    adjustments.push(OptionAdjustment {
                        option: "generation_config.count".into(),
                        requested: serde_json::json!(count),
                        applied: serde_json::json!(max_outputs),
                        reason: "clamped to provider max_outputs".into(),
                    });
                }
            }
        }
        if matches!(
            request.generation_config.background,
            Some(ImageBackground::Transparent)
        ) && !capability.supports_transparent_background
        {
            if request.compatibility_policy == CompatibilityPolicy::Strict {
                return Err(AigcError::new(
                    "unsupported_option",
                    "transparent background is not supported by selected model",
                )
                .provider(self.provider.provider_name()));
            }
            request.generation_config.background = Some(ImageBackground::Auto);
            adjustments.push(OptionAdjustment {
                option: "generation_config.background".into(),
                requested: serde_json::json!("transparent"),
                applied: serde_json::json!("auto"),
                reason: "transparent background is not supported by selected model".into(),
            });
        }
        Ok(adjustments)
    }

    async fn drain_provider_events(
        &self,
        sender: Option<&tokio::sync::mpsc::Sender<ImageGenerationEvent>>,
        events: &[ProviderImageEvent],
    ) -> Result<(), AigcError> {
        for event in events {
            match event {
                ProviderImageEvent::Status { status } => {
                    self.emit_status(sender, provider_status_to_public(status.clone()))
                        .await;
                }
                ProviderImageEvent::PartialAsset { asset } if self.config.emit_partial_images => {
                    if let Some(sender) = sender {
                        let image = self.persist_public_asset(asset.clone()).await?;
                        let _ = sender
                            .send(ImageGenerationEvent::PartialImage { image })
                            .await;
                    }
                }
                ProviderImageEvent::PartialAsset { .. } => {}
            }
        }
        Ok(())
    }

    async fn emit_status(
        &self,
        sender: Option<&tokio::sync::mpsc::Sender<ImageGenerationEvent>>,
        status: GenerationStatus,
    ) {
        if let Some(sender) = sender {
            let _ = sender.send(ImageGenerationEvent::Status { status }).await;
        }
    }

    async fn persist_public_asset(
        &self,
        asset: ProviderAsset,
    ) -> Result<GeneratedImage, AigcError> {
        let stored = self
            .asset_store
            .put_stream(
                asset.source,
                PutAssetOptions {
                    content_type_hint: asset.mime_type,
                    max_base64_bytes: self.config.max_base64_bytes,
                    ..Default::default()
                },
            )
            .await?;
        self.asset_registry
            .save(&self.config.scope, stored.clone())
            .await?;
        self.public_image(stored, &ImageOutputDelivery::Url).await
    }
}

fn provider_status_to_public(status: ProviderGenerationStatus) -> GenerationStatus {
    match status {
        ProviderGenerationStatus::Queued => GenerationStatus::Queued,
        ProviderGenerationStatus::Running => GenerationStatus::Running,
        ProviderGenerationStatus::Completed => GenerationStatus::Completed,
        ProviderGenerationStatus::Failed => GenerationStatus::Failed,
        ProviderGenerationStatus::TimedOut => GenerationStatus::TimedOut,
    }
}

fn find_operation_capability<'a>(
    operations: &'a std::collections::HashMap<String, ImageOperationCapability>,
    operation: &ImageOperation,
) -> Option<&'a ImageOperationCapability> {
    operations
        .values()
        .find(|capability| &capability.operation == operation)
}

pub fn mock_provider_asset(data: &'static [u8]) -> crate::ProviderAsset {
    crate::ProviderAsset {
        source: AssetIngestSource::Bytes {
            bytes: Bytes::from_static(data),
            mime_type: "image/png".into(),
        },
        mime_type: Some("image/png".into()),
        width: Some(1),
        height: Some(1),
        expires_at: None,
        metadata: serde_json::json!({ "mock": true, "id": Uuid::new_v4().to_string() }),
    }
}

// ---------------------------------------------------------------------------
// Video gateway
// ---------------------------------------------------------------------------

/// Wraps a [`VideoProvider`] the same way [`ImageGateway`] wraps an
/// [`ImageProvider`]: the provider's own video URL is short-lived (Volcengine's
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
        let stored = self
            .asset_store
            .put_stream(
                AssetIngestSource::Url(provider_url.to_string()),
                PutAssetOptions {
                    content_type_hint: Some(content_type_hint.into()),
                    key_prefix: Some(key_prefix.into()),
                    ..Default::default()
                },
            )
            .await?;
        self.asset_registry
            .save(&self.config.scope, stored.clone())
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
        AssetScope, CompatibilityPolicy, GenerationExecutionMode, ImageGenerationConfig,
        ImageModelCapabilities, ImageOperation, ImageOperationCapability, ImageOutputConfig,
        InMemoryAssetRegistry, LocalAssetStore, ProviderAsset, ProviderGenerationStatus,
        ProviderImageEvent, ProviderImageJob,
    };
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    struct MockImageProvider {
        asset: ProviderAsset,
        create_status: ProviderGenerationStatus,
        polls: AtomicUsize,
        capabilities: ImageModelCapabilities,
    }

    #[async_trait]
    impl ImageProvider for MockImageProvider {
        fn provider_name(&self) -> &str {
            "mock"
        }

        fn model_name(&self) -> &str {
            "mock-image"
        }

        fn capabilities(&self) -> ImageModelCapabilities {
            self.capabilities.clone()
        }

        async fn create_image_generation(
            &self,
            _request: &ImageGenerationRequest,
        ) -> Result<ProviderImageJob, AigcError> {
            Ok(ProviderImageJob {
                id: "job-1".into(),
                status: self.create_status.clone(),
                assets: if self.create_status == ProviderGenerationStatus::Completed {
                    vec![self.asset.clone()]
                } else {
                    vec![]
                },
                events: vec![ProviderImageEvent::Status {
                    status: self.create_status.clone(),
                }],
                metadata: serde_json::json!({}),
                usage: None,
                option_adjustments: vec![],
            })
        }

        async fn get_image_generation(&self, _job_id: &str) -> Result<ProviderImageJob, AigcError> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Ok(ProviderImageJob {
                id: "job-1".into(),
                status: ProviderGenerationStatus::Completed,
                assets: vec![self.asset.clone()],
                events: vec![ProviderImageEvent::Status {
                    status: ProviderGenerationStatus::Completed,
                }],
                metadata: serde_json::json!({}),
                usage: None,
                option_adjustments: vec![],
            })
        }
    }

    fn request(delivery: ImageOutputDelivery) -> ImageGenerationRequest {
        ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "test".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig::default(),
            execution_config: Default::default(),
            output_config: ImageOutputConfig { delivery },
            compatibility_policy: Default::default(),
            provider_options: serde_json::json!({}),
        }
    }

    fn mock_capabilities(max_outputs: Option<u32>) -> ImageModelCapabilities {
        let mut operations = HashMap::new();
        operations.insert(
            "texttoimage".into(),
            ImageOperationCapability {
                operation: ImageOperation::TextToImage,
                execution_modes: vec![GenerationExecutionMode::Sync],
                max_outputs,
                supports_streaming: false,
                supports_transparent_background: false,
                supported_formats: vec![crate::ImageFormat::Png],
                metadata: serde_json::json!({}),
            },
        );
        ImageModelCapabilities {
            operations,
            source: crate::CapabilitySource::Static,
        }
    }

    #[tokio::test]
    async fn url_delivery_returns_usable_url_and_asset_id() {
        let dir = tempfile::tempdir().unwrap();
        let gateway = ImageGateway::new(
            Arc::new(MockImageProvider {
                asset: mock_provider_asset(b"png"),
                create_status: ProviderGenerationStatus::Completed,
                polls: AtomicUsize::new(0),
                capabilities: mock_capabilities(Some(4)),
            }),
            Arc::new(LocalAssetStore::new(
                dir.path(),
                Some("http://localhost/assets".into()),
            )),
            Arc::new(InMemoryAssetRegistry::default()),
            ImageGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
                max_base64_bytes: Some(1024),
                emit_partial_images: false,
            },
        );
        let response = gateway
            .generate(request(ImageOutputDelivery::Url))
            .await
            .unwrap();
        assert_eq!(response.status, GenerationStatus::Completed);
        assert!(!response.images[0].asset_id.is_empty());
        match &response.images[0].output {
            ImageOutput::Url(output) => assert!(output.url.starts_with("http://localhost/assets/")),
            ImageOutput::Base64(_) => panic!("expected URL output"),
        }
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(!serialized.contains("provider/image"));
        assert!(!serialized.contains("object_key"));
    }

    #[tokio::test]
    async fn base64_delivery_still_returns_asset_id() {
        let dir = tempfile::tempdir().unwrap();
        let gateway = ImageGateway::new(
            Arc::new(MockImageProvider {
                asset: mock_provider_asset(b"png"),
                create_status: ProviderGenerationStatus::Completed,
                polls: AtomicUsize::new(0),
                capabilities: mock_capabilities(Some(4)),
            }),
            Arc::new(LocalAssetStore::new(dir.path(), None)),
            Arc::new(InMemoryAssetRegistry::default()),
            ImageGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
                max_base64_bytes: Some(1024),
                emit_partial_images: false,
            },
        );
        let response = gateway
            .generate(request(ImageOutputDelivery::Base64))
            .await
            .unwrap();
        assert!(!response.images[0].asset_id.is_empty());
        match &response.images[0].output {
            ImageOutput::Base64(output) => {
                assert_eq!(output.mime_type, "image/png");
                assert_eq!(output.data, "cG5n");
            }
            ImageOutput::Url(_) => panic!("expected base64 output"),
        }
    }

    #[tokio::test]
    async fn running_provider_job_is_polled_before_persisting_assets() {
        let dir = tempfile::tempdir().unwrap();
        let provider = Arc::new(MockImageProvider {
            asset: mock_provider_asset(b"png"),
            create_status: ProviderGenerationStatus::Running,
            polls: AtomicUsize::new(0),
            capabilities: mock_capabilities(Some(4)),
        });
        let gateway = ImageGateway::new(
            provider.clone(),
            Arc::new(LocalAssetStore::new(
                dir.path(),
                Some("http://localhost/assets".into()),
            )),
            Arc::new(InMemoryAssetRegistry::default()),
            ImageGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
                max_base64_bytes: Some(1024),
                emit_partial_images: false,
            },
        );

        let response = gateway
            .generate(request(ImageOutputDelivery::Url))
            .await
            .unwrap();

        assert_eq!(response.status, GenerationStatus::Completed);
        assert_eq!(provider.polls.load(Ordering::SeqCst), 1);
        assert_eq!(response.images.len(), 1);
    }

    #[tokio::test]
    async fn strict_policy_rejects_unsupported_operation_before_provider_call() {
        let dir = tempfile::tempdir().unwrap();
        let provider = Arc::new(MockImageProvider {
            asset: mock_provider_asset(b"png"),
            create_status: ProviderGenerationStatus::Completed,
            polls: AtomicUsize::new(0),
            capabilities: mock_capabilities(Some(1)),
        });
        let gateway = ImageGateway::new(
            provider.clone(),
            Arc::new(LocalAssetStore::new(dir.path(), None)),
            Arc::new(InMemoryAssetRegistry::default()),
            ImageGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
                max_base64_bytes: Some(1024),
                emit_partial_images: false,
            },
        );
        let mut request = request(ImageOutputDelivery::Url);
        request.operation = ImageOperation::FaceSwap;
        request.compatibility_policy = CompatibilityPolicy::Strict;

        let err = gateway.generate(request).await.unwrap_err();

        assert_eq!(err.code, "unsupported_operation");
        assert_eq!(provider.polls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn coerce_policy_clamps_count_and_records_adjustment() {
        let dir = tempfile::tempdir().unwrap();
        let gateway = ImageGateway::new(
            Arc::new(MockImageProvider {
                asset: mock_provider_asset(b"png"),
                create_status: ProviderGenerationStatus::Completed,
                polls: AtomicUsize::new(0),
                capabilities: mock_capabilities(Some(1)),
            }),
            Arc::new(LocalAssetStore::new(dir.path(), None)),
            Arc::new(InMemoryAssetRegistry::default()),
            ImageGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
                max_base64_bytes: Some(1024),
                emit_partial_images: false,
            },
        );
        let mut request = request(ImageOutputDelivery::Url);
        request.generation_config.count = Some(9);
        request.compatibility_policy = CompatibilityPolicy::Coerce;

        let response = gateway.generate(request).await.unwrap();

        assert_eq!(response.option_adjustments.len(), 1);
        assert_eq!(
            response.option_adjustments[0].option,
            "generation_config.count"
        );
        assert_eq!(response.option_adjustments[0].applied, serde_json::json!(1));
    }

    #[tokio::test]
    async fn generate_with_events_emits_provider_status_persisting_and_completed() {
        let dir = tempfile::tempdir().unwrap();
        let gateway = ImageGateway::new(
            Arc::new(MockImageProvider {
                asset: mock_provider_asset(b"png"),
                create_status: ProviderGenerationStatus::Running,
                polls: AtomicUsize::new(0),
                capabilities: mock_capabilities(Some(4)),
            }),
            Arc::new(LocalAssetStore::new(dir.path(), None)),
            Arc::new(InMemoryAssetRegistry::default()),
            ImageGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
                max_base64_bytes: Some(1024),
                emit_partial_images: false,
            },
        );
        let (tx, mut rx) = mpsc::channel(8);

        let response = gateway
            .generate_with_events(request(ImageOutputDelivery::Url), tx)
            .await
            .unwrap();

        let mut statuses = Vec::new();
        while let Some(event) = rx.recv().await {
            match event {
                crate::ImageGenerationEvent::Status { status } => statuses.push(status),
                crate::ImageGenerationEvent::Completed { .. } => break,
                crate::ImageGenerationEvent::PartialImage { .. } => {}
            }
        }
        assert_eq!(response.status, GenerationStatus::Completed);
        assert!(statuses.contains(&GenerationStatus::Running));
        assert!(statuses.contains(&GenerationStatus::PersistingAssets));
    }

    // -----------------------------------------------------------------
    // VideoGateway
    // -----------------------------------------------------------------

    use crate::{
        ProviderVideoJob, VideoContentItem, VideoExecutionConfig, VideoGenerationConfig,
        VideoTaskListQuery,
    };

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
