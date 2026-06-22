use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{
    AigcError, AssetIngestSource, CapabilitySource, GenerationExecutionMode,
    ImageGenerationRequest, ImageModelCapabilities, ImageOperation, ImageOperationCapability,
    ImageProvider, ProviderAsset, ProviderGenerationStatus, ProviderImageJob,
};

#[derive(Debug, Clone)]
pub struct RenderfulImageConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: Option<String>,
    pub timeout: Option<Duration>,
    pub webhook: Option<String>,
}

pub struct RenderfulImageAdapter {
    config: RenderfulImageConfig,
    api_base_url: String,
}

impl RenderfulImageAdapter {
    pub fn from_config(config: RenderfulImageConfig) -> Result<Self, AigcError> {
        if config.model.trim().is_empty() {
            return Err(AigcError::new("invalid_model", "model cannot be empty"));
        }
        if config.api_key.trim().is_empty() {
            return Err(AigcError::new("invalid_api_key", "API key cannot be empty"));
        }
        Ok(Self {
            api_base_url: normalize_renderful_base_url(
                &config
                    .api_url
                    .clone()
                    .unwrap_or_else(|| "https://api.renderful.ai/api/v1".into()),
            ),
            config,
        })
    }

    pub fn build_request(&self, request: &ImageGenerationRequest) -> Result<Value, AigcError> {
        super::ensure_no_unresolved_inputs(request)?;
        let task_type = match request.operation {
            ImageOperation::TextToImage => "text-to-image",
            ImageOperation::ImageToImage | ImageOperation::EditImage => "image-to-image",
            _ => {
                return Err(AigcError::new(
                    "unsupported_operation",
                    "operation is not enabled for first milestone",
                ))
            }
        };
        let mut body = json!({
            "type": task_type,
            "model": self.config.model,
            "prompt": request.prompt,
            "webhook": request.execution_config.webhook_url.clone().or_else(|| self.config.webhook.clone()),
        });
        if let Some(negative_prompt) = &request.negative_prompt {
            body["negative_prompt"] = json!(negative_prompt);
        }
        if let Some(count) = request.generation_config.count {
            body["num_outputs"] = json!(count);
        }
        if let Some(seed) = request.generation_config.seed {
            body["seed"] = json!(seed);
        }
        match &request.generation_config.size {
            crate::ImageSize::AspectRatio(value) => body["aspect_ratio"] = json!(value),
            crate::ImageSize::ResolutionTier(value) => body["resolution"] = json!(value),
            crate::ImageSize::Pixels { width, height } => {
                body["width"] = json!(width);
                body["height"] = json!(height);
            }
            crate::ImageSize::Auto => {}
        }
        if matches!(
            request.operation,
            ImageOperation::ImageToImage | ImageOperation::EditImage
        ) {
            let mut image_urls = Vec::new();
            for input in &request.inputs {
                match &input.asset {
                    crate::AssetRef::Url(url) | crate::AssetRef::DataUrl(url) => {
                        image_urls.push(url.clone());
                    }
                    crate::AssetRef::Base64 { .. } | crate::AssetRef::Bytes { .. } => {
                        return Err(AigcError::new(
                            "unsupported_input",
                            "Renderful image-to-image requires URL or data URL inputs",
                        ))
                    }
                    crate::AssetRef::LocalPath(_) | crate::AssetRef::Stored { .. } => {
                        return Err(AigcError::new(
                            "unresolved_input",
                            "adapter received unresolved local or stored input; gateway preprocessing is required",
                        ))
                    }
                }
            }
            if image_urls.is_empty() {
                return Err(AigcError::new(
                    "missing_input_image",
                    "Renderful image-to-image requires at least one image input",
                ));
            }
            body["image_url"] = json!(image_urls[0]);
            if image_urls.len() > 1 {
                body["images"] = json!(image_urls);
            }
        }
        Ok(body)
    }

    pub fn parse_poll_response(&self, response: Value) -> ProviderImageJob {
        let status = match response
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("completed")
        {
            "queued" => ProviderGenerationStatus::Queued,
            "processing" => ProviderGenerationStatus::Running,
            "failed" => ProviderGenerationStatus::Failed,
            _ => ProviderGenerationStatus::Completed,
        };
        let assets = response
            .get("outputs")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .map(|url| ProviderAsset {
                source: AssetIngestSource::Url(url.into()),
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
                .unwrap_or("renderful-job")
                .into(),
            status,
            assets,
            events: vec![],
            metadata: response,
            usage: None,
            option_adjustments: vec![],
        }
    }

    #[allow(dead_code)]
    pub fn parse_models_metadata(
        model: &str,
        task_type: &str,
        response: Value,
    ) -> Result<ImageModelCapabilities, AigcError> {
        let models = response
            .get("models")
            .or_else(|| response.get("data"))
            .and_then(|value| value.as_array())
            .ok_or_else(|| {
                AigcError::new(
                    "provider_response_parse_failed",
                    "Renderful models response must contain models or data array",
                )
                .provider("renderful")
            })?;
        let model_metadata = models
            .iter()
            .find(|item| {
                item.get("id")
                    .or_else(|| item.get("name"))
                    .and_then(|value| value.as_str())
                    == Some(model)
            })
            .cloned()
            .ok_or_else(|| {
                AigcError::new(
                    "model_metadata_not_found",
                    format!("Renderful metadata for model '{model}' was not returned"),
                )
                .provider("renderful")
            })?;
        let operation = match task_type {
            "image-to-image" => ImageOperation::ImageToImage,
            _ => ImageOperation::TextToImage,
        };
        let mut operations = std::collections::HashMap::new();
        operations.insert(
            format!("{:?}", operation).to_ascii_lowercase(),
            ImageOperationCapability {
                operation,
                execution_modes: vec![GenerationExecutionMode::Async],
                max_outputs: model_metadata
                    .get("max_outputs")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as u32),
                supports_streaming: false,
                supports_transparent_background: false,
                supported_formats: vec![],
                metadata: json!({
                    "aspect_ratios": model_metadata
                        .get("aspect_ratios")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                    "resolutions": model_metadata
                        .get("resolutions")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                    "cost": model_metadata
                        .get("cost")
                        .cloned()
                        .unwrap_or(Value::Null),
                    "supports_webhook": model_metadata
                        .get("supports_webhook")
                        .cloned()
                        .unwrap_or(Value::Bool(false)),
                }),
            },
        );
        Ok(ImageModelCapabilities {
            operations,
            source: CapabilitySource::ProviderMetadata,
        })
    }
}

#[async_trait]
impl ImageProvider for RenderfulImageAdapter {
    fn provider_name(&self) -> &str {
        "renderful"
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
        let mut request_builder = crate::http::shared_client()
            .post(format!("{}/generations", self.api_base_url))
            .bearer_auth(&self.config.api_key)
            .json(&body);
        if let Some(timeout) = self.config.timeout {
            request_builder = request_builder.timeout(timeout);
        }
        let response = request_builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("renderful")
        })?;
        let response = super::parse_json_response("renderful", response).await?;
        Ok(self.parse_poll_response(response))
    }

    async fn get_image_generation(&self, job_id: &str) -> Result<ProviderImageJob, AigcError> {
        let mut request_builder = crate::http::shared_client()
            .get(format!("{}/generations/{job_id}", self.api_base_url))
            .bearer_auth(&self.config.api_key);
        if let Some(timeout) = self.config.timeout {
            request_builder = request_builder.timeout(timeout);
        }
        let response = request_builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("renderful")
        })?;
        let response = super::parse_json_response("renderful", response).await?;
        Ok(self.parse_poll_response(response))
    }
}

fn normalize_renderful_base_url(api_url: &str) -> String {
    api_url
        .trim_end_matches('/')
        .trim_end_matches("/generations")
        .to_string()
}

#[cfg(test)]
mod tests;
