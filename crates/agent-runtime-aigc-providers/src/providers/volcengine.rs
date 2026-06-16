//! Volcengine Ark (火山方舟) image generation adapter.
//!
//! Endpoint: POST https://ark.cn-beijing.volces.com/api/v3/images/generations
//! Auth:     Authorization: Bearer $ARK_API_KEY
//! Models:   doubao-seedream-5-0-260128 (confirmed against the live API; see
//!           catalog.rs for notes on other model name variants).
//!
//! The API is OpenAI images.generate-compatible. It also supports
//! image-to-image / multi-image fusion via the `image` request parameter,
//! which accepts a single URL/base64 data URL or an array of up to 14.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{
    AigcError, AssetIngestSource, AssetRef, ImageGenerationRequest, ImageOperation, ImageProvider,
    ProviderAsset, ProviderGenerationStatus, ProviderImageJob,
};

const DEFAULT_API_URL: &str = "https://ark.cn-beijing.volces.com/api/v3";

#[derive(Debug, Clone)]
pub struct VolcengineImageConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: Option<String>,
    pub timeout: Option<Duration>,
}

pub struct VolcengineImageAdapter {
    config: VolcengineImageConfig,
    api_base_url: String,
}

impl VolcengineImageAdapter {
    pub fn from_config(config: VolcengineImageConfig) -> Result<Self, AigcError> {
        if config.model.trim().is_empty() {
            return Err(AigcError::new("invalid_model", "model cannot be empty"));
        }
        if config.api_key.trim().is_empty() {
            return Err(AigcError::new("invalid_api_key", "API key cannot be empty"));
        }
        let api_base_url = config
            .api_url
            .clone()
            .unwrap_or_else(|| DEFAULT_API_URL.into());
        let api_base_url = api_base_url.trim_end_matches('/').to_string();
        Ok(Self {
            config,
            api_base_url,
        })
    }

    pub fn build_request(&self, request: &ImageGenerationRequest) -> Result<Value, AigcError> {
        if !matches!(
            request.operation,
            ImageOperation::TextToImage | ImageOperation::ImageToImage
        ) {
            return Err(AigcError::new(
                "unsupported_operation",
                "Volcengine image generation only supports text-to-image and image-to-image",
            )
            .provider("volcengine"));
        }

        if matches!(request.operation, ImageOperation::ImageToImage) && request.inputs.is_empty() {
            return Err(AigcError::new(
                "missing_input",
                "image-to-image requires at least one reference image input",
            )
            .provider("volcengine"));
        }

        let mut body = json!({
            "model": self.config.model,
            "prompt": request.prompt,
            "n": request.generation_config.count.unwrap_or(1),
            "response_format": "url",
            "watermark": false,
        });

        // Map size
        let size_str = super::size_to_string(&request.generation_config.size);
        if size_str != "auto" {
            body["size"] = json!(size_str);
        }

        // Reference image(s) for image-to-image / multi-image fusion.
        // Docs: docs/external/volceengine/aigc/image/api.md — `image` accepts a
        // single URL/base64 data URL string, or an array of up to 14.
        if !request.inputs.is_empty() {
            let images: Vec<Value> = request
                .inputs
                .iter()
                .map(|input| match &input.asset {
                    AssetRef::Url(url) | AssetRef::DataUrl(url) => Ok(json!(url)),
                    AssetRef::Base64 { data, mime_type } => {
                        Ok(json!(format!("data:{mime_type};base64,{data}")))
                    }
                    _ => Err(AigcError::new(
                        "unsupported_input",
                        "Volcengine adapter requires URL or base64 data URL inputs",
                    )
                    .provider("volcengine")),
                })
                .collect::<Result<_, AigcError>>()?;
            body["image"] = if images.len() == 1 {
                images.into_iter().next().unwrap()
            } else {
                json!(images)
            };
        }

        // Optional watermark override from provider_options
        if let Some(wm) = request
            .provider_options
            .get("watermark")
            .and_then(|v| v.as_bool())
        {
            body["watermark"] = json!(wm);
        }

        Ok(body)
    }

    pub fn parse_response(&self, response: Value) -> Result<ProviderImageJob, AigcError> {
        let assets: Vec<ProviderAsset> = response
            .get("data")
            .and_then(|d| d.as_array())
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|item| {
                item.get("url")
                    .and_then(|u| u.as_str())
                    .map(|url| ProviderAsset {
                        source: AssetIngestSource::Url(url.to_string()),
                        mime_type: Some("image/png".into()),
                        width: None,
                        height: None,
                        expires_at: None,
                        metadata: json!({}),
                    })
            })
            .collect();

        Ok(ProviderImageJob {
            id: response
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("volcengine-sync")
                .into(),
            status: ProviderGenerationStatus::Completed,
            assets,
            events: vec![],
            metadata: response,
            usage: None,
            option_adjustments: vec![],
        })
    }
}

#[async_trait]
impl ImageProvider for VolcengineImageAdapter {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> crate::ImageModelCapabilities {
        super::basic_capabilities(vec![
            ImageOperation::TextToImage,
            ImageOperation::ImageToImage,
        ])
    }

    async fn create_image_generation(
        &self,
        request: &ImageGenerationRequest,
    ) -> Result<ProviderImageJob, AigcError> {
        let body = self.build_request(request)?;
        let url = format!("{}/images/generations", self.api_base_url);
        let mut builder = crate::http::shared_client()
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body);
        if let Some(timeout) = self.config.timeout {
            builder = builder.timeout(timeout);
        }
        let response = builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("volcengine")
        })?;
        let response = super::parse_json_response("volcengine", response).await?;
        self.parse_response(response)
    }

    async fn get_image_generation(&self, job_id: &str) -> Result<ProviderImageJob, AigcError> {
        Ok(ProviderImageJob {
            id: job_id.into(),
            status: ProviderGenerationStatus::Completed,
            assets: vec![],
            events: vec![],
            metadata: json!({}),
            usage: None,
            option_adjustments: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
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
}
