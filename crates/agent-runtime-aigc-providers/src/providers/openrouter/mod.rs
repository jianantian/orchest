use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{
    AigcError, AssetIngestSource, CapabilitySource, GenerationExecutionMode,
    ImageGenerationRequest, ImageModelCapabilities, ImageOperation, ImageOperationCapability,
    ImageProvider, ImageSize, ProviderAsset, ProviderGenerationStatus, ProviderImageEvent,
    ProviderImageJob,
};

#[derive(Debug, Clone)]
pub struct OpenRouterImageConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: Option<String>,
    pub timeout: Option<Duration>,
    pub app_title: Option<String>,
    pub site_url: Option<String>,
}

pub struct OpenRouterImageAdapter {
    config: OpenRouterImageConfig,
    api_base_url: String,
}

impl OpenRouterImageAdapter {
    pub fn from_config(config: OpenRouterImageConfig) -> Result<Self, AigcError> {
        if config.model.trim().is_empty() {
            return Err(AigcError::new("invalid_model", "model cannot be empty"));
        }
        if config.api_key.trim().is_empty() {
            return Err(AigcError::new("invalid_api_key", "API key cannot be empty"));
        }
        Ok(Self {
            api_base_url: normalize_openrouter_base_url(
                &config
                    .api_url
                    .clone()
                    .unwrap_or_else(|| "https://openrouter.ai/api/v1".into()),
            ),
            config,
        })
    }

    pub fn build_request(&self, request: &ImageGenerationRequest) -> Result<Value, AigcError> {
        if !matches!(request.operation, ImageOperation::TextToImage) {
            return Err(AigcError::new(
                "unsupported_operation",
                "OpenRouter image input mapping is model-specific and is not enabled by default",
            ));
        }
        let mut modalities = vec!["image"];
        if request
            .provider_options
            .get("include_text")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| self.config.model.contains("gemini"))
        {
            modalities.push("text");
        }
        let mut image_config = json!({
            "aspect_ratio": match &request.generation_config.size {
                ImageSize::AspectRatio(value) => value.clone(),
                _ => "1:1".into(),
            },
            "image_size": match &request.generation_config.size {
                ImageSize::ResolutionTier(value) => value.clone(),
                _ => "1K".into(),
            }
        });
        for key in [
            "strength",
            "text_layout",
            "font_inputs",
            "super_resolution_references",
        ] {
            if let Some(value) = request.provider_options.get(key) {
                image_config[key] = value.clone();
            }
        }
        if let Some(style) = request
            .generation_config
            .style
            .as_ref()
            .and_then(|style| style.style.as_ref())
        {
            image_config["style"] = json!(style);
        }
        if let Some(style) = &request.generation_config.style {
            let colors = style
                .colors
                .iter()
                .filter_map(|color| parse_hex_rgb(color))
                .collect::<Vec<_>>();
            if !colors.is_empty() {
                image_config["rgb_colors"] = json!(colors);
            }
        }
        Ok(json!({
            "model": self.config.model,
            "modalities": modalities,
            "messages": [{ "role": "user", "content": request.prompt }],
            "image_config": image_config
        }))
    }

    pub fn parse_response(&self, response: Value) -> ProviderImageJob {
        let images = response
            .pointer("/choices/0/message/images")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten();
        let assets = images
            .filter_map(|item| {
                item.get("image_url")
                    .and_then(|v| v.get("url"))
                    .and_then(|v| v.as_str())
            })
            .map(|data_url| ProviderAsset {
                source: AssetIngestSource::DataUrl(data_url.into()),
                mime_type: Some("image/png".into()),
                width: None,
                height: None,
                expires_at: None,
                metadata: json!({}),
            })
            .collect();
        ProviderImageJob {
            id: response
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("openrouter-sync")
                .into(),
            status: ProviderGenerationStatus::Completed,
            assets,
            events: vec![],
            metadata: response,
            usage: None,
            option_adjustments: vec![],
        }
    }

    #[allow(dead_code)]
    pub fn parse_stream_delta(&self, response: Value) -> Vec<ProviderImageEvent> {
        response
            .pointer("/choices/0/delta/images")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| {
                item.get("image_url")
                    .and_then(|v| v.get("url"))
                    .and_then(|v| v.as_str())
            })
            .map(|url| ProviderImageEvent::PartialAsset {
                asset: ProviderAsset {
                    source: AssetIngestSource::DataUrl(url.into()),
                    mime_type: Some("image/png".into()),
                    width: None,
                    height: None,
                    expires_at: None,
                    metadata: json!({}),
                },
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn parse_model_metadata(metadata: Value) -> Result<ImageModelCapabilities, AigcError> {
        let output_modalities = metadata
            .pointer("/architecture/output_modalities")
            .or_else(|| metadata.get("output_modalities"))
            .cloned()
            .unwrap_or_else(|| json!([]));
        let output_has_image = output_modalities
            .as_array()
            .map(|values| values.iter().any(|value| value.as_str() == Some("image")))
            .unwrap_or(false);
        if !output_has_image {
            return Err(AigcError::new(
                "unsupported_model",
                "OpenRouter model metadata does not advertise image output",
            )
            .provider("openrouter"));
        }
        let mut operations = std::collections::HashMap::new();
        operations.insert(
            "texttoimage".into(),
            ImageOperationCapability {
                operation: ImageOperation::TextToImage,
                execution_modes: vec![
                    GenerationExecutionMode::Sync,
                    GenerationExecutionMode::Stream,
                ],
                max_outputs: None,
                supports_streaming: true,
                supports_transparent_background: false,
                supported_formats: vec![],
                metadata: json!({
                    "output_modalities": output_modalities,
                    "input_modalities": metadata
                        .pointer("/architecture/input_modalities")
                        .or_else(|| metadata.get("input_modalities"))
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                    "supported_parameters": metadata
                        .get("supported_parameters")
                        .cloned()
                        .unwrap_or_else(|| json!([]))
                }),
            },
        );
        Ok(ImageModelCapabilities {
            operations,
            source: CapabilitySource::ProviderMetadata,
        })
    }
}

fn parse_hex_rgb(color: &str) -> Option<[u8; 3]> {
    let color = color.strip_prefix('#').unwrap_or(color);
    if color.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&color[0..2], 16).ok()?;
    let g = u8::from_str_radix(&color[2..4], 16).ok()?;
    let b = u8::from_str_radix(&color[4..6], 16).ok()?;
    Some([r, g, b])
}

#[async_trait]
impl ImageProvider for OpenRouterImageAdapter {
    fn provider_name(&self) -> &str {
        "openrouter"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> crate::ImageModelCapabilities {
        super::basic_capabilities(vec![ImageOperation::TextToImage])
    }

    async fn create_image_generation(
        &self,
        request: &ImageGenerationRequest,
    ) -> Result<ProviderImageJob, AigcError> {
        let body = self.build_request(request)?;
        let mut request_builder = crate::http::shared_client()
            .post(format!("{}/chat/completions", self.api_base_url))
            .bearer_auth(&self.config.api_key)
            .json(&body);
        if let Some(title) = &self.config.app_title {
            request_builder = request_builder.header("X-Title", title);
        }
        if let Some(site_url) = &self.config.site_url {
            request_builder = request_builder.header("HTTP-Referer", site_url);
        }
        if let Some(timeout) = self.config.timeout {
            request_builder = request_builder.timeout(timeout);
        }
        let response = request_builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("openrouter")
        })?;
        let response = super::parse_json_response("openrouter", response).await?;
        Ok(self.parse_response(response))
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

fn normalize_openrouter_base_url(api_url: &str) -> String {
    api_url
        .trim_end_matches('/')
        .trim_end_matches("/chat/completions")
        .to_string()
}

#[cfg(test)]
mod tests;
