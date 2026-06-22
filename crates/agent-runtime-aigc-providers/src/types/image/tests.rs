use super::*;
use crate::types::common::CompatibilityPolicy;
use std::collections::HashMap;

fn sample_request(delivery: ImageOutputDelivery) -> ImageGenerationRequest {
    ImageGenerationRequest {
        operation: ImageOperation::TextToImage,
        prompt: "a red cube".into(),
        negative_prompt: Some("blur".into()),
        inputs: vec![],
        generation_config: ImageGenerationConfig::default(),
        execution_config: GenerationExecutionConfig::default(),
        output_config: ImageOutputConfig { delivery },
        compatibility_policy: CompatibilityPolicy::Coerce,
        provider_options: serde_json::json!({}),
    }
}

#[test]
fn request_serde_roundtrip() {
    let request = sample_request(ImageOutputDelivery::Url);
    let json = serde_json::to_string(&request).unwrap();
    let restored: ImageGenerationRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, restored);
}

#[test]
fn response_serde_roundtrip() {
    let response = ImageGenerationResponse {
        job_id: "job".into(),
        status: GenerationStatus::Completed,
        images: vec![GeneratedImage {
            asset_id: "asset".into(),
            output: ImageOutput::Url(ImageUrlOutput {
                url: "https://assets.example/a.png".into(),
                expires_at: None,
            }),
            mime_type: Some("image/png".into()),
            width: Some(1024),
            height: Some(1024),
        }],
        option_adjustments: vec![],
        usage: Some(ImageUsage {
            image_count: 1,
            ..Default::default()
        }),
    };
    let json = serde_json::to_string(&response).unwrap();
    let restored: ImageGenerationResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, restored);
}

#[test]
fn provider_job_serde_roundtrip() {
    let job = ProviderImageJob {
        id: "job".into(),
        status: ProviderGenerationStatus::Completed,
        assets: vec![ProviderAsset {
            source: AssetIngestSource::Url("https://provider/image.png".into()),
            mime_type: Some("image/png".into()),
            width: None,
            height: None,
            expires_at: None,
            metadata: serde_json::json!({}),
        }],
        events: vec![],
        metadata: serde_json::json!({}),
        usage: None,
        option_adjustments: vec![],
    };
    let json = serde_json::to_string(&job).unwrap();
    let restored: ProviderImageJob = serde_json::from_str(&json).unwrap();
    assert_eq!(job, restored);
}

#[test]
fn generated_image_asset_id_exists_for_url_and_base64() {
    let url = GeneratedImage {
        asset_id: "asset-url".into(),
        output: ImageOutput::Url(ImageUrlOutput {
            url: "https://assets.example/a.png".into(),
            expires_at: None,
        }),
        mime_type: None,
        width: None,
        height: None,
    };
    let base64 = GeneratedImage {
        asset_id: "asset-b64".into(),
        output: ImageOutput::Base64(ImageBase64Output {
            data: "aGVsbG8=".into(),
            mime_type: "image/png".into(),
        }),
        mime_type: None,
        width: None,
        height: None,
    };
    assert!(!url.asset_id.is_empty());
    assert!(!base64.asset_id.is_empty());
}

#[test]
fn public_url_output_does_not_expose_storage_descriptors() {
    let image = GeneratedImage {
        asset_id: "asset".into(),
        output: ImageOutput::Url(ImageUrlOutput {
            url: "https://assets.example/a.png".into(),
            expires_at: None,
        }),
        mime_type: None,
        width: None,
        height: None,
    };
    let json = serde_json::to_value(&image).unwrap();
    let serialized = json.to_string();
    for forbidden in [
        "bucket",
        "object_key",
        "endpoint",
        "access_key",
        "provider_url",
    ] {
        assert!(!serialized.contains(forbidden));
    }
}

#[test]
fn capabilities_serde_roundtrip() {
    use crate::types::common::CapabilitySource;
    use serde_json::Value;

    let mut operations = HashMap::new();
    operations.insert(
        "text_to_image".into(),
        ImageOperationCapability {
            operation: ImageOperation::TextToImage,
            execution_modes: vec![GenerationExecutionMode::Sync],
            max_outputs: Some(4),
            supports_streaming: false,
            supports_transparent_background: true,
            supported_formats: vec![ImageFormat::Png],
            metadata: Value::Null,
        },
    );
    let caps = ImageModelCapabilities {
        operations,
        source: CapabilitySource::Static,
    };
    let json = serde_json::to_string(&caps).unwrap();
    let restored: ImageModelCapabilities = serde_json::from_str(&json).unwrap();
    assert_eq!(caps, restored);
}
