use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    resolve_asset_url, AigcError, AssetIngestSource, AssetRef, AssetRegistry, AssetScope,
    AssetStore, GenerationStatus, ImageBase64Output, ImageGenerationRequest,
    ImageGenerationResponse, ImageInput, ImageOutput, ImageOutputDelivery, ImageUrlOutput,
    ProviderGenerationStatus, PutAssetOptions, StoredAsset,
};
use crate::{GeneratedImage, ImageProvider};

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

    #[allow(clippy::result_large_err)]
    pub async fn generate(
        &self,
        mut request: ImageGenerationRequest,
    ) -> Result<ImageGenerationResponse, AigcError> {
        self.preprocess_inputs(&mut request).await?;
        let provider_job = self.provider.create_image_generation(&request).await?;
        let provider_job = self.wait_for_completion(provider_job, &request).await?;
        if matches!(provider_job.status, ProviderGenerationStatus::Failed) {
            return Err(AigcError::new(
                "provider_generation_failed",
                "provider generation did not complete",
            )
            .provider(self.provider.provider_name()));
        }
        if matches!(provider_job.status, ProviderGenerationStatus::TimedOut) {
            return Err(AigcError::new(
                "provider_generation_timed_out",
                "provider generation timed out",
            )
            .provider(self.provider.provider_name()));
        }

        let mut images = Vec::with_capacity(provider_job.assets.len());
        for asset in provider_job.assets {
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
            self.asset_registry
                .save(&self.config.scope, stored.clone())
                .await?;
            images.push(
                self.public_image(stored, &request.output_config.delivery)
                    .await?,
            );
        }

        Ok(ImageGenerationResponse {
            job_id: provider_job.id,
            status: GenerationStatus::Completed,
            images,
            option_adjustments: provider_job.option_adjustments,
            usage: provider_job.usage,
        })
    }

    #[allow(clippy::result_large_err)]
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
            match job.status {
                ProviderGenerationStatus::Completed
                | ProviderGenerationStatus::Failed
                | ProviderGenerationStatus::TimedOut => return Ok(job),
                ProviderGenerationStatus::Queued | ProviderGenerationStatus::Running => {
                    if started_at.elapsed() >= timeout {
                        job.status = ProviderGenerationStatus::TimedOut;
                        return Ok(job);
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AssetScope, ImageGenerationConfig, ImageModelCapabilities, ImageOperation,
        ImageOutputConfig, InMemoryAssetRegistry, LocalAssetStore, ProviderAsset,
        ProviderGenerationStatus, ProviderImageJob,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockImageProvider {
        asset: ProviderAsset,
        create_status: ProviderGenerationStatus,
        polls: AtomicUsize,
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
            ImageModelCapabilities {
                provider: "mock".into(),
                model: "mock-image".into(),
                operations: Default::default(),
                source: crate::CapabilitySource::Static,
            }
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
                events: vec![],
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
                events: vec![],
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

    #[tokio::test]
    async fn url_delivery_returns_usable_url_and_asset_id() {
        let dir = tempfile::tempdir().unwrap();
        let gateway = ImageGateway::new(
            Arc::new(MockImageProvider {
                asset: mock_provider_asset(b"png"),
                create_status: ProviderGenerationStatus::Completed,
                polls: AtomicUsize::new(0),
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
            }),
            Arc::new(LocalAssetStore::new(dir.path(), None)),
            Arc::new(InMemoryAssetRegistry::default()),
            ImageGatewayConfig {
                scope: AssetScope::test(),
                signed_url_ttl: None,
                max_base64_bytes: Some(1024),
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
}
