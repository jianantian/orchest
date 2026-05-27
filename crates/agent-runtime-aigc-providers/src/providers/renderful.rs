use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{
    AigcError, AssetIngestSource, ImageGenerationRequest, ImageOperation, ImageProvider,
    ProviderAsset, ProviderGenerationStatus, ProviderImageJob,
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
        super::basic_capabilities(
            "renderful",
            &self.config.model,
            vec![ImageOperation::TextToImage, ImageOperation::ImageToImage],
        )
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
mod tests {
    use super::*;
    use crate::{AssetRef, ImageGenerationConfig, ImageInput, ImageInputRole, ImageOutputConfig};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn rejects_unresolved_stored_inputs() {
        let adapter = RenderfulImageAdapter::from_config(RenderfulImageConfig {
            model: "flux".into(),
            api_key: "key".into(),
            api_url: None,
            timeout: None,
            webhook: None,
        })
        .unwrap();
        let err = adapter
            .build_request(&ImageGenerationRequest {
                operation: ImageOperation::ImageToImage,
                prompt: "edit".into(),
                negative_prompt: None,
                inputs: vec![ImageInput {
                    role: ImageInputRole::Source,
                    asset: AssetRef::Stored {
                        asset_id: "asset".into(),
                    },
                    mime_type: None,
                }],
                generation_config: ImageGenerationConfig::default(),
                execution_config: Default::default(),
                output_config: ImageOutputConfig::default(),
                compatibility_policy: Default::default(),
                provider_options: json!({}),
            })
            .unwrap_err();
        assert_eq!(err.code, "unresolved_input");
    }

    #[test]
    fn image_to_image_maps_resolved_url_input() {
        let adapter = RenderfulImageAdapter::from_config(RenderfulImageConfig {
            model: "grok-imagine-image-i2i".into(),
            api_key: "key".into(),
            api_url: None,
            timeout: None,
            webhook: None,
        })
        .unwrap();
        let body = adapter
            .build_request(&ImageGenerationRequest {
                operation: ImageOperation::ImageToImage,
                prompt: "restyle".into(),
                negative_prompt: None,
                inputs: vec![ImageInput {
                    role: ImageInputRole::Source,
                    asset: AssetRef::Url("https://assets.example/input.png".into()),
                    mime_type: None,
                }],
                generation_config: ImageGenerationConfig::default(),
                execution_config: Default::default(),
                output_config: ImageOutputConfig::default(),
                compatibility_policy: Default::default(),
                provider_options: json!({}),
            })
            .unwrap();

        assert_eq!(body["type"], "image-to-image");
        assert_eq!(body["image_url"], "https://assets.example/input.png");
    }

    #[tokio::test]
    async fn create_and_get_generation_use_renderful_api() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut create_socket, _) = listener.accept().await.unwrap();
            let create_request = read_http_request(&mut create_socket).await;
            let lower_create_request = create_request.to_ascii_lowercase();
            assert!(create_request.starts_with("POST /api/v1/generations "));
            assert!(lower_create_request.contains("authorization: bearer key"));
            assert!(create_request.contains("\"type\":\"text-to-image\""));
            let create_body = r#"{"id":"gen_1","status":"processing","outputs":[]}"#;
            let create_response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                create_body.len(),
                create_body
            );
            create_socket
                .write_all(create_response.as_bytes())
                .await
                .unwrap();

            let (mut get_socket, _) = listener.accept().await.unwrap();
            let get_request = read_http_request(&mut get_socket).await;
            let lower_get_request = get_request.to_ascii_lowercase();
            assert!(get_request.starts_with("GET /api/v1/generations/gen_1 "));
            assert!(lower_get_request.contains("authorization: bearer key"));
            let get_body =
                r#"{"id":"gen_1","status":"completed","outputs":["https://provider/image.png"]}"#;
            let get_response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                get_body.len(),
                get_body
            );
            get_socket.write_all(get_response.as_bytes()).await.unwrap();
        });

        let adapter = RenderfulImageAdapter::from_config(RenderfulImageConfig {
            model: "flux-dev".into(),
            api_key: "key".into(),
            api_url: Some(format!("http://{addr}/api/v1")),
            timeout: None,
            webhook: None,
        })
        .unwrap();
        let created = adapter
            .create_image_generation(&ImageGenerationRequest {
                operation: ImageOperation::TextToImage,
                prompt: "sunset".into(),
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
        let completed = adapter.get_image_generation(&created.id).await.unwrap();

        server.await.unwrap();
        assert_eq!(created.status, ProviderGenerationStatus::Running);
        assert_eq!(completed.status, ProviderGenerationStatus::Completed);
        assert!(matches!(
            completed.assets[0].source,
            AssetIngestSource::Url(_)
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
