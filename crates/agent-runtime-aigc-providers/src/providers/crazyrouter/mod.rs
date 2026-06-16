use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use reqwest::multipart::{Form, Part};
use serde_json::{json, Value};

use crate::{
    AigcError, AssetIngestSource, AssetRef, CompatibilityPolicy, ImageBackground, ImageFormat,
    ImageGenerationRequest, ImageInputRole, ImageOperation, ImageProvider, ImageQuality,
    ProviderAsset, ProviderGenerationStatus, ProviderImageJob,
};

#[derive(Debug, Clone)]
pub struct CrazyrouterImageConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: Option<String>,
    pub timeout: Option<Duration>,
}

pub struct CrazyrouterImageAdapter {
    config: CrazyrouterImageConfig,
    api_url: String,
}

impl CrazyrouterImageAdapter {
    pub fn from_config(config: CrazyrouterImageConfig) -> Result<Self, AigcError> {
        if config.model.trim().is_empty() {
            return Err(AigcError::new("invalid_model", "model cannot be empty"));
        }
        if config.api_key.trim().is_empty() {
            return Err(AigcError::new("invalid_api_key", "API key cannot be empty"));
        }
        Ok(Self {
            api_url: config
                .api_url
                .clone()
                .unwrap_or_else(|| "https://cn.crazyrouter.com".into()),
            config,
        })
    }

    pub fn build_request(
        &self,
        request: &ImageGenerationRequest,
    ) -> Result<(String, Value), AigcError> {
        let endpoint = match request.operation {
            ImageOperation::TextToImage => "/v1/images/generations",
            ImageOperation::ImageToImage | ImageOperation::EditImage => "/v1/images/edits",
            _ => {
                return Err(AigcError::new(
                    "unsupported_operation",
                    "Crazyrouter does not support this operation",
                ))
            }
        };
        let mut body = json!({
            "model": self.config.model,
            "prompt": request.prompt,
            "n": request.generation_config.count.unwrap_or(1),
            "size": super::size_to_string(&request.generation_config.size),
        });
        if let Some(user) = &request.execution_config.user {
            body["user"] = json!(user);
        }
        self.apply_generation_options(request, &mut body)?;
        if request.execution_config.stream {
            body["stream"] = json!(true);
        }
        if let Some(partials) = request.execution_config.partial_image_count {
            body["partial_images"] = json!(partials);
        }
        Ok((format!("{}{}", self.api_url, endpoint), body))
    }

    fn apply_generation_options(
        &self,
        request: &ImageGenerationRequest,
        body: &mut Value,
    ) -> Result<(), AigcError> {
        if let Some(count) = request.generation_config.count {
            if !(1..=10).contains(&count) {
                return Err(AigcError::new(
                    "unsupported_option",
                    "Crazyrouter n must be between 1 and 10",
                )
                .provider("crazyrouter"));
            }
        }
        if let Some(quality) = &request.generation_config.quality {
            let value = match quality {
                ImageQuality::Low => "low",
                ImageQuality::Medium => "medium",
                ImageQuality::High | ImageQuality::Hd => "high",
                ImageQuality::Auto => "auto",
                ImageQuality::Standard => {
                    if request.compatibility_policy == CompatibilityPolicy::Strict {
                        return Err(AigcError::new(
                            "unsupported_option",
                            "Crazyrouter rejects quality=standard",
                        )
                        .provider("crazyrouter"));
                    }
                    "auto"
                }
            };
            body["quality"] = json!(value);
        }
        if let Some(background) = &request.generation_config.background {
            let value = match background {
                ImageBackground::Auto => "auto",
                ImageBackground::Opaque => "opaque",
                ImageBackground::Transparent => {
                    if request.compatibility_policy == CompatibilityPolicy::Strict {
                        return Err(AigcError::new(
                            "unsupported_option",
                            "Crazyrouter rejects transparent background",
                        )
                        .provider("crazyrouter"));
                    }
                    "auto"
                }
            };
            body["background"] = json!(value);
        }
        if let Some(format) = &request.generation_config.format {
            body["output_format"] = json!(match format {
                ImageFormat::Png => "png",
                ImageFormat::Jpeg => "jpeg",
                ImageFormat::Webp => "webp",
            });
        }
        if let Some(compression) = request
            .provider_options
            .get("output_compression")
            .and_then(|value| value.as_u64())
        {
            if compression > 100 {
                return Err(AigcError::new(
                    "unsupported_option",
                    "Crazyrouter output_compression must be 0-100",
                )
                .provider("crazyrouter"));
            }
            if matches!(request.generation_config.format, Some(ImageFormat::Png)) {
                if request.compatibility_policy == CompatibilityPolicy::Strict {
                    return Err(AigcError::new(
                        "unsupported_option",
                        "Crazyrouter rejects png output_compression",
                    )
                    .provider("crazyrouter"));
                }
            } else {
                body["output_compression"] = json!(compression);
            }
        }
        if let Some(moderation) = request
            .generation_config
            .safety
            .as_ref()
            .and_then(|safety| safety.moderation.as_ref())
        {
            body["moderation"] = json!(moderation);
        }
        Ok(())
    }

    async fn build_edit_form(&self, request: &ImageGenerationRequest) -> Result<Form, AigcError> {
        if request
            .generation_config
            .edit
            .as_ref()
            .map(|edit| !edit.regions.is_empty())
            .unwrap_or(false)
        {
            return Err(AigcError::new(
                "unsupported_region_edit",
                "Crazyrouter region edits require caller-provided mask input",
            )
            .provider("crazyrouter"));
        }

        let mut form = Form::new()
            .text("model", self.config.model.clone())
            .text("prompt", request.prompt.clone())
            .text(
                "n",
                request.generation_config.count.unwrap_or(1).to_string(),
            )
            .text(
                "size",
                super::size_to_string(&request.generation_config.size),
            );
        if let Some(user) = &request.execution_config.user {
            form = form.text("user", user.clone());
        }

        let mut image_count = 0;
        let mut has_mask = false;
        for input in &request.inputs {
            match input.role {
                ImageInputRole::Source | ImageInputRole::Reference => {
                    image_count += 1;
                    if image_count > 16 {
                        return Err(AigcError::new(
                            "too_many_inputs",
                            "Crazyrouter edit supports at most 16 source/reference images",
                        )
                        .provider("crazyrouter"));
                    }
                    form = form.part(
                        "image[]",
                        self.asset_part(&input.asset, input.mime_type.as_deref())
                            .await?,
                    );
                }
                ImageInputRole::Mask => {
                    if has_mask {
                        return Err(AigcError::new(
                            "too_many_masks",
                            "Crazyrouter edit supports one mask input",
                        )
                        .provider("crazyrouter"));
                    }
                    has_mask = true;
                    form = form.part(
                        "mask",
                        self.asset_part(&input.asset, input.mime_type.as_deref())
                            .await?,
                    );
                }
                _ => {}
            }
        }
        if image_count == 0 {
            return Err(AigcError::new(
                "missing_input_image",
                "Crazyrouter edit requires at least one source or reference image",
            )
            .provider("crazyrouter"));
        }
        Ok(form)
    }

    async fn asset_part(
        &self,
        asset: &AssetRef,
        mime_type_hint: Option<&str>,
    ) -> Result<Part, AigcError> {
        let (bytes, mime_type) = match asset {
            AssetRef::Bytes { bytes, mime_type } => (bytes.clone(), mime_type.clone()),
            AssetRef::Base64 { data, mime_type } => (
                Bytes::from(
                    base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|err| AigcError::new("invalid_base64", err.to_string()))?,
                ),
                mime_type.clone(),
            ),
            AssetRef::DataUrl(data_url) => parse_data_url(data_url)?,
            AssetRef::Url(url) => {
                let response = crate::http::shared_client()
                    .get(url)
                    .send()
                    .await
                    .map_err(|err| {
                        AigcError::new("input_download_failed", err.to_string())
                            .provider("crazyrouter")
                    })?;
                if !response.status().is_success() {
                    return Err(AigcError::new(
                        "input_download_failed",
                        format!("input download failed with status {}", response.status()),
                    )
                    .provider("crazyrouter"));
                }
                let mime_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string)
                    .or_else(|| mime_type_hint.map(str::to_string))
                    .unwrap_or_else(|| "application/octet-stream".into());
                let bytes = response.bytes().await.map_err(|err| {
                    AigcError::new("input_download_failed", err.to_string())
                        .provider("crazyrouter")
                })?;
                (bytes, mime_type)
            }
            AssetRef::LocalPath(_) | AssetRef::Stored { .. } => {
                return Err(AigcError::new(
                    "unresolved_input",
                    "adapter received unresolved local or stored input; gateway preprocessing is required",
                )
                .provider("crazyrouter"))
            }
        };
        Part::bytes(bytes.to_vec())
            .mime_str(&mime_type)
            .map_err(|err| {
                AigcError::new("invalid_mime_type", err.to_string()).provider("crazyrouter")
            })
    }

    pub fn parse_response(&self, response: Value) -> Result<ProviderImageJob, AigcError> {
        let assets = response
            .get("data")
            .and_then(|data| data.as_array())
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|item| item.get("url").and_then(|url| url.as_str()))
            .map(|url| ProviderAsset {
                source: AssetIngestSource::Url(url.to_string()),
                mime_type: Some("image/png".into()),
                width: None,
                height: None,
                expires_at: None,
                metadata: json!({}),
            })
            .collect();
        Ok(ProviderImageJob {
            id: response
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("crazyrouter-sync")
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
impl ImageProvider for CrazyrouterImageAdapter {
    fn provider_name(&self) -> &str {
        "crazyrouter"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> crate::ImageModelCapabilities {
        super::basic_capabilities(vec![
            ImageOperation::TextToImage,
            ImageOperation::ImageToImage,
            ImageOperation::EditImage,
        ])
    }

    async fn create_image_generation(
        &self,
        request: &ImageGenerationRequest,
    ) -> Result<ProviderImageJob, AigcError> {
        let mut request_builder = crate::http::shared_client()
            .post(match request.operation {
                ImageOperation::TextToImage => {
                    format!(
                        "{}/v1/images/generations",
                        self.api_url.trim_end_matches('/')
                    )
                }
                ImageOperation::ImageToImage | ImageOperation::EditImage => {
                    format!("{}/v1/images/edits", self.api_url.trim_end_matches('/'))
                }
                _ => {
                    return Err(AigcError::new(
                        "unsupported_operation",
                        "Crazyrouter does not support this operation",
                    )
                    .provider("crazyrouter"))
                }
            })
            .bearer_auth(&self.config.api_key);
        request_builder = match request.operation {
            ImageOperation::TextToImage => {
                let (_, body) = self.build_request(request)?;
                request_builder.json(&body)
            }
            ImageOperation::ImageToImage | ImageOperation::EditImage => {
                request_builder.multipart(self.build_edit_form(request).await?)
            }
            _ => unreachable!("unsupported operation returned earlier"),
        };
        if let Some(timeout) = self.config.timeout {
            request_builder = request_builder.timeout(timeout);
        }
        let response = request_builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("crazyrouter")
        })?;
        let response = super::parse_json_response("crazyrouter", response).await?;
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

fn parse_data_url(data_url: &str) -> Result<(Bytes, String), AigcError> {
    let Some(rest) = data_url.strip_prefix("data:") else {
        return Err(AigcError::new(
            "invalid_data_url",
            "data URL must start with data:",
        ));
    };
    let Some((meta, data)) = rest.split_once(',') else {
        return Err(AigcError::new(
            "invalid_data_url",
            "data URL is missing comma",
        ));
    };
    if !meta.split(';').any(|value| value == "base64") {
        return Err(AigcError::new(
            "invalid_data_url",
            "only base64 data URLs are supported",
        ));
    }
    let mime_type = meta
        .split(';')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map(Bytes::from)
        .map_err(|err| AigcError::new("invalid_base64", err.to_string()))?;
    Ok((bytes, mime_type))
}

#[cfg(test)]
mod tests;
