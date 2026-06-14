use std::time::Duration;

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::{json, Value};

use crate::{
    AigcError, AssetIngestSource, ImageGenerationRequest, ImageOperation, ImageProvider,
    ProviderAsset, ProviderGenerationStatus, ProviderImageJob,
};

#[derive(Debug, Clone)]
pub struct AliyunImageConfig {
    pub model: String,
    pub api_key: String,
    pub region: Option<String>,
    pub api_url: Option<String>,
    pub timeout: Option<Duration>,
}

pub struct AliyunImageAdapter {
    config: AliyunImageConfig,
    api_base_url: String,
}

impl AliyunImageAdapter {
    pub fn from_config(config: AliyunImageConfig) -> Result<Self, AigcError> {
        if config.model.trim().is_empty() {
            return Err(AigcError::new("invalid_model", "model cannot be empty"));
        }
        if config.api_key.trim().is_empty() {
            return Err(AigcError::new("invalid_api_key", "API key cannot be empty"));
        }
        let api_base_url =
            config
                .api_url
                .clone()
                .unwrap_or_else(|| match config.region.as_deref() {
                    Some("singapore") | Some("ap-southeast-1") => {
                        "https://dashscope-intl.aliyuncs.com/api/v1".into()
                    }
                    Some("virginia") | Some("us-east-1") => {
                        "https://dashscope-us.aliyuncs.com/api/v1".into()
                    }
                    _ => "https://dashscope.aliyuncs.com/api/v1".into(),
                });
        Ok(Self {
            config,
            api_base_url: normalize_dashscope_base_url(&api_base_url),
        })
    }

    fn is_wanx_model(&self) -> bool {
        self.config.model.starts_with("wanx")
    }

    pub fn build_request(&self, request: &ImageGenerationRequest) -> Result<Value, AigcError> {
        let parameters = self.build_parameters(request);
        if self.is_wanx_model() {
            Ok(json!({
                "model": self.config.model,
                "input": { "prompt": request.prompt },
                "parameters": parameters,
            }))
        } else {
            let mut content = vec![json!({ "text": request.prompt })];
            for input in &request.inputs {
                match &input.asset {
                    crate::AssetRef::Url(url) | crate::AssetRef::DataUrl(url) => {
                        content.push(json!({ "image": url }));
                    }
                    crate::AssetRef::Base64 { data, mime_type } => {
                        content.push(json!({ "image": format!("data:{mime_type};base64,{data}") }));
                    }
                    _ => {
                        return Err(AigcError::new(
                            "unsupported_input",
                            "Aliyun adapter requires URL or base64 data URL inputs",
                        ))
                    }
                }
            }
            Ok(json!({
                "model": self.config.model,
                "input": { "messages": [{ "role": "user", "content": content }] },
                "parameters": parameters,
            }))
        }
    }

    fn build_parameters(&self, request: &ImageGenerationRequest) -> Value {
        let mut parameters = json!({
            "size": aliyun_size_to_string(&request.generation_config.size),
        });
        if let Some(count) = request.generation_config.count {
            parameters["n"] = json!(count);
        }
        if let Some(negative_prompt) = &request.negative_prompt {
            parameters["negative_prompt"] = json!(negative_prompt);
        }
        if let Some(seed) = request.generation_config.seed {
            parameters["seed"] = json!(seed);
        }
        for key in [
            "prompt_extend",
            "watermark",
            "enable_sequential",
            "thinking_mode",
            "color_palette",
        ] {
            if let Some(value) = request.provider_options.get(key) {
                parameters[key] = value.clone();
            }
        }
        if let Some(edit) = &request.generation_config.edit {
            let boxes = edit
                .regions
                .iter()
                .filter_map(|region| match region {
                    crate::ImageRegion::BoundingBox {
                        x,
                        y,
                        width,
                        height,
                    } => Some(json!([x, y, x + width, y + height])),
                    crate::ImageRegion::Mask => None,
                })
                .collect::<Vec<_>>();
            if !boxes.is_empty() {
                parameters["bbox_list"] = json!(boxes);
            }
        }
        parameters
    }

    pub fn parse_response(&self, response: Value) -> ProviderImageJob {
        // wanx async task-start response: output.task_id + output.task_status
        if let Some(task_id) = response
            .pointer("/output/task_id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        {
            return self.parse_task_response(response, &task_id);
        }
        // multimodal synchronous response: output.choices[0].message.content[].image
        let urls = response
            .pointer("/output/choices/0/message/content")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("image").and_then(|v| v.as_str()));
        let assets = urls
            .map(|url| ProviderAsset {
                source: AssetIngestSource::Url(url.into()),
                mime_type: Some("image/png".into()),
                width: None,
                height: None,
                expires_at: Some(Utc::now() + ChronoDuration::hours(24)),
                metadata: json!({}),
            })
            .collect();
        ProviderImageJob {
            id: response
                .get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or("aliyun-sync")
                .into(),
            status: ProviderGenerationStatus::Completed,
            assets,
            events: vec![],
            metadata: response,
            usage: None,
            option_adjustments: vec![],
        }
    }
}

fn aliyun_size_to_string(size: &crate::ImageSize) -> String {
    match size {
        crate::ImageSize::Pixels { width, height } => format!("{width}*{height}"),
        other => super::size_to_string(other),
    }
}

fn normalize_dashscope_base_url(api_url: &str) -> String {
    api_url
        .trim_end_matches('/')
        .trim_end_matches("/services/aigc/multimodal-generation/generation")
        .trim_end_matches("/services/aigc/image-generation/generation")
        .trim_end_matches("/services/aigc/text2image/image-synthesis")
        .to_string()
}

#[async_trait]
impl ImageProvider for AliyunImageAdapter {
    fn provider_name(&self) -> &str {
        "aliyun"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> crate::ImageModelCapabilities {
        super::basic_capabilities(
            "aliyun",
            &self.config.model,
            vec![
                ImageOperation::TextToImage,
                ImageOperation::ImageToImage,
                ImageOperation::EditImage,
            ],
        )
    }

    async fn create_image_generation(
        &self,
        request: &ImageGenerationRequest,
    ) -> Result<ProviderImageJob, AigcError> {
        let body = self.build_request(request)?;
        let endpoint = if self.is_wanx_model() {
            self.text2image_endpoint()
        } else {
            self.multimodal_generation_endpoint()
        };
        let mut request_builder = crate::http::shared_client()
            .post(endpoint)
            .bearer_auth(&self.config.api_key)
            .json(&body);
        if let Some(timeout) = self.config.timeout {
            request_builder = request_builder.timeout(timeout);
        }
        // wanx models require async task-based execution; other models support both
        let use_async = self.is_wanx_model() || request.execution_config.prefer_async;
        if use_async {
            request_builder = request_builder.header("X-DashScope-Async", "enable");
        }
        let response = request_builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("aliyun")
        })?;
        let response = super::parse_json_response("aliyun", response).await?;
        let mut job = self.parse_response(response);
        if use_async && job.assets.is_empty() {
            job.status = ProviderGenerationStatus::Queued;
        }
        Ok(job)
    }

    async fn get_image_generation(&self, job_id: &str) -> Result<ProviderImageJob, AigcError> {
        let endpoint = format!("{}/tasks/{job_id}", self.api_base_url);
        let mut request_builder = crate::http::shared_client()
            .get(endpoint)
            .bearer_auth(&self.config.api_key);
        if let Some(timeout) = self.config.timeout {
            request_builder = request_builder.timeout(timeout);
        }
        let response = request_builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("aliyun")
        })?;
        let response = super::parse_json_response("aliyun", response).await?;
        Ok(self.parse_task_response(response, job_id))
    }
}

impl AliyunImageAdapter {
    fn multimodal_generation_endpoint(&self) -> String {
        format!(
            "{}/services/aigc/multimodal-generation/generation",
            self.api_base_url
        )
    }

    fn text2image_endpoint(&self) -> String {
        format!(
            "{}/services/aigc/text2image/image-synthesis",
            self.api_base_url
        )
    }

    fn parse_task_response(&self, response: Value, fallback_id: &str) -> ProviderImageJob {
        let task_status = response
            .pointer("/output/task_status")
            .and_then(|v| v.as_str())
            .unwrap_or("SUCCEEDED");
        let status = match task_status {
            "PENDING" => ProviderGenerationStatus::Queued,
            "RUNNING" => ProviderGenerationStatus::Running,
            "FAILED" | "UNKNOWN" => ProviderGenerationStatus::Failed,
            _ => ProviderGenerationStatus::Completed,
        };
        let assets = response
            .pointer("/output/results")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| {
                item.get("url")
                    .or_else(|| item.get("image"))
                    .and_then(|v| v.as_str())
            })
            .map(|url| ProviderAsset {
                source: AssetIngestSource::Url(url.into()),
                mime_type: Some("image/png".into()),
                width: None,
                height: None,
                expires_at: Some(Utc::now() + ChronoDuration::hours(24)),
                metadata: json!({}),
            })
            .collect();
        ProviderImageJob {
            id: response
                .pointer("/output/task_id")
                .and_then(|v| v.as_str())
                .unwrap_or(fallback_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ImageEditConfig, ImageGenerationConfig, ImageOutputConfig, ImageRegion, ImageSize,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn qwen_request_uses_multimodal_shape() {
        let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
            model: "qwen-image".into(),
            api_key: "key".into(),
            region: None,
            api_url: None,
            timeout: None,
        })
        .unwrap();
        let body = adapter
            .build_request(&ImageGenerationRequest {
                operation: ImageOperation::TextToImage,
                prompt: "mountain".into(),
                negative_prompt: Some("fog".into()),
                inputs: vec![],
                generation_config: ImageGenerationConfig::default(),
                execution_config: Default::default(),
                output_config: ImageOutputConfig::default(),
                compatibility_policy: Default::default(),
                provider_options: json!({}),
            })
            .unwrap();
        assert_eq!(
            body["input"]["messages"][0]["content"][0]["text"],
            "mountain"
        );
        assert_eq!(body["parameters"]["negative_prompt"], "fog");
    }

    #[test]
    fn request_maps_prompt_extend_watermark_wan_options_and_bbox() {
        let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
            model: "wan2.7-image-pro".into(),
            api_key: "key".into(),
            region: None,
            api_url: None,
            timeout: None,
        })
        .unwrap();
        let body = adapter
            .build_request(&ImageGenerationRequest {
                operation: ImageOperation::EditImage,
                prompt: "replace object".into(),
                negative_prompt: None,
                inputs: vec![],
                generation_config: ImageGenerationConfig {
                    size: ImageSize::ResolutionTier("2K".into()),
                    edit: Some(ImageEditConfig {
                        regions: vec![ImageRegion::BoundingBox {
                            x: 10,
                            y: 20,
                            width: 30,
                            height: 40,
                        }],
                    }),
                    ..Default::default()
                },
                execution_config: Default::default(),
                output_config: ImageOutputConfig::default(),
                compatibility_policy: Default::default(),
                provider_options: json!({
                    "prompt_extend": true,
                    "watermark": false,
                    "enable_sequential": true,
                    "thinking_mode": false,
                    "color_palette": [{"color": "#ff0000", "ratio": 100.0}]
                }),
            })
            .unwrap();
        assert_eq!(body["parameters"]["size"], "2K");
        assert_eq!(body["parameters"]["prompt_extend"], true);
        assert_eq!(body["parameters"]["watermark"], false);
        assert_eq!(body["parameters"]["enable_sequential"], true);
        assert_eq!(body["parameters"]["thinking_mode"], false);
        assert_eq!(body["parameters"]["color_palette"][0]["color"], "#ff0000");
        assert_eq!(body["parameters"]["bbox_list"], json!([[10, 20, 40, 60]]));
    }

    #[test]
    fn wanx_request_uses_prompt_field() {
        let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
            model: "wanx2.1-t2i-turbo".into(),
            api_key: "key".into(),
            region: None,
            api_url: None,
            timeout: None,
        })
        .unwrap();
        let body = adapter
            .build_request(&ImageGenerationRequest {
                operation: ImageOperation::TextToImage,
                prompt: "mountain".into(),
                negative_prompt: None,
                inputs: vec![],
                generation_config: ImageGenerationConfig::default(),
                execution_config: Default::default(),
                output_config: ImageOutputConfig::default(),
                compatibility_policy: Default::default(),
                provider_options: json!({}),
            })
            .unwrap();
        assert_eq!(body["input"]["prompt"], "mountain");
        assert!(body["input"].get("messages").is_none());
    }

    #[tokio::test]
    async fn wanx_model_posts_to_text2image_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut socket).await;
            assert!(request.starts_with("POST /api/v1/services/aigc/text2image/image-synthesis "));
            assert!(request
                .to_ascii_lowercase()
                .contains("x-dashscope-async: enable"));
            assert!(request.contains("\"model\":\"wanx2.1-t2i-turbo\""));
            let body = r#"{"request_id":"req-wanx","output":{"task_id":"task-123","task_status":"PENDING"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
            model: "wanx2.1-t2i-turbo".into(),
            api_key: "key".into(),
            region: None,
            api_url: Some(format!("http://{addr}/api/v1")),
            timeout: None,
        })
        .unwrap();
        let job = adapter
            .create_image_generation(&ImageGenerationRequest {
                operation: ImageOperation::TextToImage,
                prompt: "mountain".into(),
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
        assert_eq!(job.id, "task-123");
        assert!(matches!(job.status, ProviderGenerationStatus::Queued));
    }

    #[tokio::test]
    async fn create_generation_posts_to_dashscope_api() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut socket).await;
            let lower_request = request.to_ascii_lowercase();
            assert!(
                request.starts_with("POST /api/v1/services/aigc/multimodal-generation/generation ")
            );
            assert!(lower_request.contains("authorization: bearer key"));
            assert!(request.contains("\"model\":\"qwen-image\""));
            let body = r#"{"request_id":"req-1","output":{"choices":[{"message":{"content":[{"image":"https://dashscope-result/image.png"}]}}]}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
            model: "qwen-image".into(),
            api_key: "key".into(),
            region: None,
            api_url: Some(format!("http://{addr}/api/v1")),
            timeout: None,
        })
        .unwrap();
        let job = adapter
            .create_image_generation(&ImageGenerationRequest {
                operation: ImageOperation::TextToImage,
                prompt: "mountain".into(),
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
        assert_eq!(job.id, "req-1");
        assert!(matches!(job.assets[0].source, AssetIngestSource::Url(_)));
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
