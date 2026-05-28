use std::env;
use std::sync::Arc;
use std::time::Duration;

use agent_runtime_aigc_providers::{
    create_image_provider_from_config, AigcProviderRuntimeConfig, AssetScope, ImageGateway,
    ImageGatewayConfig, ImageGenerationConfig, ImageGenerationRequest, ImageOperation, ImageOutput,
    ImageOutputConfig, ImageOutputDelivery, ImageSize, InMemoryAssetRegistry, OssAssetStore,
    OssStorageConfig,
};

#[tokio::test]
#[ignore = "requires real provider API credentials and AIGC_OSS_* credentials"]
async fn live_text_to_image_persists_to_oss_and_returns_controlled_url() {
    load_dotenv_if_present();
    let provider_name =
        env::var("AIGC_LIVE_PROVIDER").expect("AIGC_LIVE_PROVIDER is required for live smoke");
    let model = env::var("AIGC_LIVE_MODEL").expect("AIGC_LIVE_MODEL is required for live smoke");
    let api_url = match provider_name.as_str() {
        "aliyun" | "dashscope" => env::var("DASHSCOPE_API_URL").ok(),
        "openrouter" => env::var("OPENROUTER_API_URL").ok(),
        "renderful" => env::var("RENDERFUL_API_URL").ok(),
        "crazyrouter" => env::var("CRAZYROUTER_API_URL").ok(),
        _ => None,
    };
    let provider = create_image_provider_from_config(AigcProviderRuntimeConfig {
        provider: provider_name,
        model,
        api_url,
        timeout: Some(Duration::from_secs(240)),
        ..Default::default()
    })
    .expect("provider config should be valid");
    let gateway = ImageGateway::new(
        Arc::from(provider),
        Arc::new(OssAssetStore::new(
            OssStorageConfig::from_env().expect("AIGC_STORAGE_OSS_* config should be set"),
        )),
        Arc::new(InMemoryAssetRegistry::default()),
        ImageGatewayConfig {
            scope: AssetScope {
                tenant: env::var("AIGC_LIVE_TENANT").unwrap_or_else(|_| "live".into()),
                workspace: env::var("AIGC_LIVE_WORKSPACE").unwrap_or_else(|_| "default".into()),
                app: env::var("AIGC_LIVE_APP").unwrap_or_else(|_| "aigc-gateway".into()),
                namespace: "image-smoke".into(),
            },
            signed_url_ttl: Some(Duration::from_secs(900)),
            max_base64_bytes: Some(16 * 1024 * 1024),
            emit_partial_images: false,
        },
    );

    let response = gateway
        .generate(ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "A small blue square icon on a plain white background".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig {
                size: ImageSize::Pixels {
                    width: 1024,
                    height: 1024,
                },
                count: Some(1),
                ..Default::default()
            },
            execution_config: Default::default(),
            output_config: ImageOutputConfig {
                delivery: ImageOutputDelivery::Url,
            },
            compatibility_policy: Default::default(),
            provider_options: serde_json::json!({}),
        })
        .await
        .expect("live image generation should succeed");

    assert_eq!(response.images.len(), 1);
    assert!(!response.images[0].asset_id.is_empty());
    match &response.images[0].output {
        ImageOutput::Url(output) => assert!(!output.url.is_empty()),
        ImageOutput::Base64(_) => panic!("expected URL delivery"),
    }
}

fn load_dotenv_if_present() {
    let mut path = std::env::current_dir().ok();
    let mut contents = None;
    while let Some(dir) = path {
        let candidate = dir.join(".env");
        if let Ok(value) = std::fs::read_to_string(&candidate) {
            contents = Some(value);
            break;
        }
        path = dir.parent().map(|parent| parent.to_path_buf());
    }
    let Some(contents) = contents else {
        return;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key
            .trim()
            .strip_prefix("export ")
            .unwrap_or(key.trim())
            .trim();
        if env::var_os(key).is_some() {
            continue;
        }
        env::set_var(key, value.trim().trim_matches('"'));
    }
}
