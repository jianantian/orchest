use std::sync::Arc;

use agent_runtime_aigc_providers::{
    mock_provider_asset, AssetScope, ImageGateway, ImageGatewayConfig, ImageGenerationConfig,
    ImageGenerationRequest, ImageModelCapabilities, ImageOperation, ImageOutputConfig,
    ImageOutputDelivery, ImageProvider, InMemoryAssetRegistry, LocalAssetStore,
    ProviderGenerationStatus, ProviderImageJob,
};
use async_trait::async_trait;

struct ExampleProvider;

#[async_trait]
impl ImageProvider for ExampleProvider {
    fn provider_name(&self) -> &str {
        "example"
    }

    fn model_name(&self) -> &str {
        "example-image"
    }

    fn capabilities(&self) -> ImageModelCapabilities {
        ImageModelCapabilities {
            provider: "example".into(),
            model: "example-image".into(),
            operations: Default::default(),
            source: Default::default(),
        }
    }

    async fn create_image_generation(
        &self,
        _request: &ImageGenerationRequest,
    ) -> Result<ProviderImageJob, agent_runtime_aigc_providers::AigcError> {
        Ok(ProviderImageJob {
            id: "example-job".into(),
            status: ProviderGenerationStatus::Completed,
            assets: vec![mock_provider_asset(b"example")],
            events: vec![],
            metadata: serde_json::json!({}),
            usage: None,
            option_adjustments: vec![],
        })
    }

    async fn get_image_generation(
        &self,
        _job_id: &str,
    ) -> Result<ProviderImageJob, agent_runtime_aigc_providers::AigcError> {
        unreachable!("example provider completes synchronously")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let gateway = ImageGateway::new(
        Arc::new(ExampleProvider),
        Arc::new(LocalAssetStore::new(dir.path(), None)),
        Arc::new(InMemoryAssetRegistry::default()),
        ImageGatewayConfig {
            scope: AssetScope::test(),
            signed_url_ttl: None,
            max_base64_bytes: Some(1024 * 1024),
        },
    );

    let response = gateway
        .generate(ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "a compact product mockup".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig::default(),
            execution_config: Default::default(),
            output_config: ImageOutputConfig {
                delivery: ImageOutputDelivery::Base64,
            },
            compatibility_policy: Default::default(),
            provider_options: serde_json::json!({}),
        })
        .await?;
    println!("{}", response.images[0].asset_id);
    Ok(())
}
