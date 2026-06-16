use super::*;
use crate::gateway::mock_provider_asset;
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
