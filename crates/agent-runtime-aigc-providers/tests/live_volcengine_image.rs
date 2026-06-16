//! Live acceptance tests against the real Volcengine Ark image generation API.
//!
//! Requires `ARK_API_KEY` (see `.env` at the repo root — `VOLCENGINE_API_KEY`
//! is the same Ark key and works interchangeably). Run with:
//!
//!   cargo test -p agent-runtime-aigc-providers --test live_volcengine_image -- --ignored --nocapture

use std::env;

use agent_runtime_aigc_providers::{
    AssetIngestSource, AssetRef, ImageGenerationConfig, ImageGenerationRequest, ImageInput,
    ImageInputRole, ImageOperation, ImageOutputConfig, ImageSize,
};

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

fn adapter() -> agent_runtime_aigc_providers::AigcProviderRuntimeConfig {
    load_dotenv_if_present();
    let api_key = env::var("ARK_API_KEY").expect("ARK_API_KEY must be set for live test");
    agent_runtime_aigc_providers::AigcProviderRuntimeConfig {
        provider: "volcengine".into(),
        model: "doubao-seedream-5-0-260128".into(),
        api_key: Some(api_key),
        timeout: Some(std::time::Duration::from_secs(120)),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "requires real ARK_API_KEY"]
async fn live_text_to_image_returns_url() {
    let provider =
        agent_runtime_aigc_providers::create_image_provider_from_config(adapter()).unwrap();

    let job = provider
        .create_image_generation(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "A small blue square icon on a plain white background".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig {
                // doubao-seedream-5.0 requires >= 3,686,400 px total.
                size: ImageSize::Pixels {
                    width: 2048,
                    height: 2048,
                },
                count: Some(1),
                ..Default::default()
            },
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: serde_json::json!({}),
        })
        .await
        .expect("live text-to-image should succeed");

    assert_eq!(job.assets.len(), 1);
    let AssetIngestSource::Url(url) = &job.assets[0].source else {
        panic!("expected a URL asset");
    };
    assert!(
        url.starts_with("https://"),
        "expected https URL, got: {url}"
    );
    println!("text-to-image URL: {url}");
}

#[tokio::test]
#[ignore = "requires real ARK_API_KEY"]
async fn live_image_to_image_with_reference() {
    let provider =
        agent_runtime_aigc_providers::create_image_provider_from_config(adapter()).unwrap();

    // First generate a base image to use as the reference input.
    let base_job = provider
        .create_image_generation(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "A red circle on a white background".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig {
                size: ImageSize::Pixels {
                    width: 2048,
                    height: 2048,
                },
                count: Some(1),
                ..Default::default()
            },
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: serde_json::json!({}),
        })
        .await
        .expect("base image generation should succeed");

    let AssetIngestSource::Url(base_url) = &base_job.assets[0].source else {
        panic!("expected a URL asset");
    };

    let job = provider
        .create_image_generation(&ImageGenerationRequest {
            operation: ImageOperation::ImageToImage,
            prompt: "Change the circle's color to blue, keep everything else the same".into(),
            negative_prompt: None,
            inputs: vec![ImageInput {
                role: ImageInputRole::Source,
                asset: AssetRef::Url(base_url.clone()),
                mime_type: None,
            }],
            generation_config: ImageGenerationConfig {
                size: ImageSize::Pixels {
                    width: 2048,
                    height: 2048,
                },
                count: Some(1),
                ..Default::default()
            },
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: serde_json::json!({}),
        })
        .await
        .expect("live image-to-image should succeed");

    assert_eq!(job.assets.len(), 1);
    let AssetIngestSource::Url(url) = &job.assets[0].source else {
        panic!("expected a URL asset");
    };
    assert!(
        url.starts_with("https://"),
        "expected https URL, got: {url}"
    );
    println!("image-to-image URL: {url}");
}
