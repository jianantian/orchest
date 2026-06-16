use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::{
    resolve_asset_url, AigcError, AssetIngestSource, AssetRef, AssetRegistry, AssetScope,
    AssetStore, CompatibilityPolicy, GenerationStatus, ImageBackground, ImageBase64Output,
    ImageGenerationEvent, ImageGenerationRequest, ImageGenerationResponse, ImageInput,
    ImageOperation, ImageOperationCapability, ImageOutput, ImageOutputDelivery, ImageUrlOutput,
    OptionAdjustment, ProviderAsset, ProviderGenerationStatus, ProviderImageEvent, PutAssetOptions,
    StoredAsset,
};
use crate::{GeneratedImage, ImageProvider};

use super::{persist_provider_asset, provider_status_to_public, AssetStorageContext};

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
            let stored = persist_provider_asset(
                AssetStorageContext {
                    asset_store: self.asset_store.as_ref(),
                    asset_registry: self.asset_registry.as_ref(),
                    scope: &self.config.scope,
                    provider_name: self.provider.provider_name(),
                },
                asset.source,
                PutAssetOptions {
                    content_type_hint: asset.mime_type.clone(),
                    max_base64_bytes: self.config.max_base64_bytes,
                    ..Default::default()
                },
            )
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
        let stored = persist_provider_asset(
            AssetStorageContext {
                asset_store: self.asset_store.as_ref(),
                asset_registry: self.asset_registry.as_ref(),
                scope: &self.config.scope,
                provider_name: self.provider.provider_name(),
            },
            asset.source,
            PutAssetOptions {
                content_type_hint: asset.mime_type,
                max_base64_bytes: self.config.max_base64_bytes,
                ..Default::default()
            },
        )
        .await?;
        self.public_image(stored, &ImageOutputDelivery::Url).await
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

#[cfg(test)]
mod tests;
