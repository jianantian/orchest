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
    pub fn parse_model_metadata(
        model: &str,
        metadata: Value,
    ) -> Result<ImageModelCapabilities, AigcError> {
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
            provider: "openrouter".into(),
            model: model.into(),
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
        super::basic_capabilities(
            "openrouter",
            &self.config.model,
            vec![ImageOperation::TextToImage],
        )
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
mod tests {
    use super::*;
    use crate::{ImageGenerationConfig, ImageOutputConfig, ImageSize};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn request_uses_chat_completions_image_config() {
        let adapter = OpenRouterImageAdapter::from_config(OpenRouterImageConfig {
            model: "google/gemini".into(),
            api_key: "key".into(),
            api_url: None,
            timeout: None,
            app_title: None,
            site_url: None,
        })
        .unwrap();
        let body = adapter
            .build_request(&ImageGenerationRequest {
                operation: ImageOperation::TextToImage,
                prompt: "poster".into(),
                negative_prompt: None,
                inputs: vec![],
                generation_config: ImageGenerationConfig {
                    size: ImageSize::AspectRatio("16:9".into()),
                    ..Default::default()
                },
                execution_config: Default::default(),
                output_config: ImageOutputConfig::default(),
                compatibility_policy: Default::default(),
                provider_options: json!({}),
            })
            .unwrap();
        assert_eq!(body["modalities"][0], "image");
        assert_eq!(body["image_config"]["aspect_ratio"], "16:9");
    }

    #[test]
    fn rejects_unverified_image_to_image_mapping() {
        let adapter = OpenRouterImageAdapter::from_config(OpenRouterImageConfig {
            model: "google/gemini".into(),
            api_key: "key".into(),
            api_url: None,
            timeout: None,
            app_title: None,
            site_url: None,
        })
        .unwrap();
        let err = adapter
            .build_request(&ImageGenerationRequest {
                operation: ImageOperation::ImageToImage,
                prompt: "restyle".into(),
                negative_prompt: None,
                inputs: vec![],
                generation_config: ImageGenerationConfig::default(),
                execution_config: Default::default(),
                output_config: ImageOutputConfig::default(),
                compatibility_policy: Default::default(),
                provider_options: json!({"model_specific": true}),
            })
            .unwrap_err();

        assert_eq!(err.code, "unsupported_operation");
    }

    #[test]
    fn image_config_maps_model_specific_provider_options_and_stream_deltas() {
        let adapter = OpenRouterImageAdapter::from_config(OpenRouterImageConfig {
            model: "recraft/recraft-v3".into(),
            api_key: "key".into(),
            api_url: None,
            timeout: None,
            app_title: None,
            site_url: None,
        })
        .unwrap();
        let body = adapter
            .build_request(&ImageGenerationRequest {
                operation: ImageOperation::TextToImage,
                prompt: "poster".into(),
                negative_prompt: None,
                inputs: vec![],
                generation_config: ImageGenerationConfig {
                    size: ImageSize::ResolutionTier("4K".into()),
                    style: Some(crate::ImageStyleConfig {
                        style: Some("Photorealism".into()),
                        colors: vec!["#ff0000".into(), "#008000".into()],
                    }),
                    ..Default::default()
                },
                execution_config: Default::default(),
                output_config: ImageOutputConfig::default(),
                compatibility_policy: Default::default(),
                provider_options: json!({
                    "include_text": true,
                    "strength": 0.7,
                    "text_layout": [{"text": "SALE", "bbox": [0, 0, 100, 100]}]
                }),
            })
            .unwrap();
        assert_eq!(body["modalities"], json!(["image", "text"]));
        assert_eq!(body["image_config"]["image_size"], "4K");
        assert_eq!(body["image_config"]["strength"], 0.7);
        assert_eq!(body["image_config"]["style"], "Photorealism");
        assert_eq!(
            body["image_config"]["rgb_colors"],
            json!([[255, 0, 0], [0, 128, 0]])
        );
        assert_eq!(body["image_config"]["text_layout"][0]["text"], "SALE");

        let events = adapter.parse_stream_delta(json!({
            "choices": [{"delta": {"images": [{"image_url": {"url": "data:image/png;base64,cG5n"}}]}}]
        }));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            crate::ProviderImageEvent::PartialAsset { .. }
        ));
    }

    #[test]
    fn model_metadata_parser_records_image_modalities() {
        let caps = OpenRouterImageAdapter::parse_model_metadata(
            "google/gemini-2.5-flash-image",
            json!({
                "id": "google/gemini-2.5-flash-image",
                "architecture": {
                    "input_modalities": ["text"],
                    "output_modalities": ["image", "text"]
                },
                "supported_parameters": ["image_config"]
            }),
        )
        .unwrap();
        let op = caps.operations.get("texttoimage").unwrap();
        assert_eq!(caps.source, crate::CapabilitySource::ProviderMetadata);
        assert_eq!(op.metadata["output_modalities"], json!(["image", "text"]));
    }

    #[tokio::test]
    async fn create_generation_posts_to_openrouter_api() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut socket).await;
            let lower_request = request.to_ascii_lowercase();
            assert!(request.starts_with("POST /api/v1/chat/completions "));
            assert!(lower_request.contains("authorization: bearer key"));
            assert!(request.contains("\"modalities\":[\"image\",\"text\"]"));
            let body = r#"{"id":"or-1","choices":[{"message":{"images":[{"image_url":{"url":"data:image/png;base64,cG5n"}}]}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let adapter = OpenRouterImageAdapter::from_config(OpenRouterImageConfig {
            model: "google/gemini-2.5-flash-image".into(),
            api_key: "key".into(),
            api_url: Some(format!("http://{addr}/api/v1")),
            timeout: None,
            app_title: None,
            site_url: None,
        })
        .unwrap();
        let job = adapter
            .create_image_generation(&ImageGenerationRequest {
                operation: ImageOperation::TextToImage,
                prompt: "poster".into(),
                negative_prompt: None,
                inputs: vec![],
                generation_config: ImageGenerationConfig::default(),
                execution_config: Default::default(),
                output_config: ImageOutputConfig::default(),
                compatibility_policy: Default::default(),
                provider_options: json!({}),
            })
            .await
            .unwrap();

        server.await.unwrap();
        assert_eq!(job.id, "or-1");
        assert!(matches!(
            job.assets[0].source,
            AssetIngestSource::DataUrl(_)
        ));
    }

    async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut buf = [0; 1024];
        loop {
            let n = socket.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buf[..n]);
            if request_is_complete(&request) {
                break;
            }
        }
        String::from_utf8(request).unwrap()
    }

    fn request_is_complete(request: &[u8]) -> bool {
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        request.len() >= header_end + 4 + content_length
    }
}
