use super::*;
use crate::{ImageGenerationConfig, ImageInput, ImageInputRole, ImageOutputConfig, ImageSize};
use serde_json::json;

fn adapter() -> VolcengineImageAdapter {
    VolcengineImageAdapter::from_config(VolcengineImageConfig {
        model: "doubao-seedream-5-0-260128".into(),
        api_key: "test-key".into(),
        api_url: None,
        timeout: None,
    })
    .unwrap()
}

#[test]
fn build_request_text_to_image() {
    let request = ImageGenerationRequest {
        operation: ImageOperation::TextToImage,
        prompt: "a cat".into(),
        negative_prompt: None,
        inputs: vec![],
        generation_config: ImageGenerationConfig {
            count: Some(2),
            size: ImageSize::ResolutionTier("2K".into()),
            ..Default::default()
        },
        execution_config: Default::default(),
        output_config: ImageOutputConfig::default(),
        compatibility_policy: Default::default(),
        provider_options: json!({}),
    };
    let body = adapter().build_request(&request).unwrap();
    assert_eq!(body["model"], "doubao-seedream-5-0-260128");
    assert_eq!(body["prompt"], "a cat");
    assert_eq!(body["n"], 2);
    assert_eq!(body["size"], "2K");
    assert_eq!(body["watermark"], false);
    assert!(body.get("image").is_none());
}

#[test]
fn build_request_image_to_image_with_url() {
    let request = ImageGenerationRequest {
        operation: ImageOperation::ImageToImage,
        prompt: "make it blue".into(),
        negative_prompt: None,
        inputs: vec![ImageInput {
            role: ImageInputRole::Source,
            asset: AssetRef::Url("https://example.com/in.png".into()),
            mime_type: None,
        }],
        generation_config: Default::default(),
        execution_config: Default::default(),
        output_config: Default::default(),
        compatibility_policy: Default::default(),
        provider_options: json!({}),
    };
    let body = adapter().build_request(&request).unwrap();
    assert_eq!(body["image"], "https://example.com/in.png");
}

#[test]
fn build_request_image_to_image_multi_input_uses_array() {
    let request = ImageGenerationRequest {
        operation: ImageOperation::ImageToImage,
        prompt: "fuse these".into(),
        negative_prompt: None,
        inputs: vec![
            ImageInput {
                role: ImageInputRole::Source,
                asset: AssetRef::Url("https://example.com/a.png".into()),
                mime_type: None,
            },
            ImageInput {
                role: ImageInputRole::Reference,
                asset: AssetRef::Url("https://example.com/b.png".into()),
                mime_type: None,
            },
        ],
        generation_config: Default::default(),
        execution_config: Default::default(),
        output_config: Default::default(),
        compatibility_policy: Default::default(),
        provider_options: json!({}),
    };
    let body = adapter().build_request(&request).unwrap();
    assert!(body["image"].is_array());
    assert_eq!(body["image"].as_array().unwrap().len(), 2);
}

#[test]
fn rejects_image_to_image_without_inputs() {
    let request = ImageGenerationRequest {
        operation: ImageOperation::ImageToImage,
        prompt: "make it better".into(),
        negative_prompt: None,
        inputs: vec![],
        generation_config: Default::default(),
        execution_config: Default::default(),
        output_config: Default::default(),
        compatibility_policy: Default::default(),
        provider_options: json!({}),
    };
    let err = adapter().build_request(&request).unwrap_err();
    assert_eq!(err.code, "missing_input");
}

#[test]
fn rejects_unsupported_operation() {
    let request = ImageGenerationRequest {
        operation: ImageOperation::Upscale,
        prompt: "upscale this".into(),
        negative_prompt: None,
        inputs: vec![],
        generation_config: Default::default(),
        execution_config: Default::default(),
        output_config: Default::default(),
        compatibility_policy: Default::default(),
        provider_options: json!({}),
    };
    let err = adapter().build_request(&request).unwrap_err();
    assert_eq!(err.code, "unsupported_operation");
}

#[test]
fn parse_response_extracts_url() {
    let response = json!({
        "id": "gen-123",
        "data": [{"url": "https://example.com/image.png"}]
    });
    let job = adapter().parse_response(response).unwrap();
    assert_eq!(job.id, "gen-123");
    assert_eq!(job.assets.len(), 1);
    assert!(matches!(job.assets[0].source, AssetIngestSource::Url(_)));
}
