//! Renderful image generation as the spine [`GenTask`] (Issue 007). Ported from
//! `agent-runtime-aigc-providers`'s `providers/renderful`, over the spine
//! [`ProtocolError`] / [`GenResult`].
//!
//! Renderful is a **submit → poll → fetch** REST dialect (Bearer auth): `POST
//! {base}/generations` returns a job, `GET {base}/generations/{id}` reports
//! `status` (queued / processing / failed / completed) and `outputs[]` URLs.

use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenHandle, GenRequest, GenResult,
    GenStatus, GenTask, Modality, ProtocolError,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::shared_client;
use serde_json::{json, Value};

const DEFAULT_API_URL: &str = "https://api.renderful.ai/api/v1";

/// Renderful gen-task configuration.
#[derive(Debug, Clone)]
pub struct RenderfulConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Renderful HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Renderful HTTP {code}: {body}"),
    )
    .with_status(code)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_err(what: &str, e: serde_json::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("failed to parse Renderful {what} response: {e}"),
    )
}

/// Trim a configured base URL down to the API root (drop a trailing `/generations`).
fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/')
        .trim_end_matches("/generations")
        .to_string()
}

/// Build the `/generations` submit body: `prompt` + `model` + the dialect knobs
/// (`type`, `negative_prompt`, `num_outputs`, `seed`, `aspect_ratio`/`resolution`/
/// `width`/`height`, `image_url`/`images`, `webhook`) passed through from
/// [`GenRequest::params`].
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    let mut body = json!({
        "type": request.params.get("type").and_then(Value::as_str).unwrap_or("text-to-image"),
        "model": model,
        "prompt": request.prompt,
    });
    if let Some(params) = request.params.as_object() {
        for key in [
            "negative_prompt",
            "num_outputs",
            "seed",
            "aspect_ratio",
            "resolution",
            "width",
            "height",
            "image_url",
            "images",
            "webhook",
        ] {
            if let Some(value) = params.get(key) {
                body[key] = value.clone();
            }
        }
    }
    body
}

/// Map the Renderful `status` field onto the spine lifecycle.
pub fn map_status(response: &Value) -> GenStatus {
    match response
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed")
    {
        "queued" => GenStatus::Pending,
        "processing" => GenStatus::Running,
        "failed" => GenStatus::Failed,
        _ => GenStatus::Done,
    }
}

/// Collect the `outputs[]` URLs into spine assets (Renderful returns PNG URLs).
pub fn parse_assets(response: &Value) -> Vec<GenAsset> {
    response
        .get("outputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|url| GenAsset::Url {
            url: url.to_string(),
            media_type: Some("image/png".to_string()),
        })
        .collect()
}

fn parse_job_id(response: &Value) -> String {
    response
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("renderful-job")
        .to_string()
}

/// The Renderful image gen provider as the spine [`GenTask`].
pub struct RenderfulGen {
    config: RenderfulConfig,
}

impl RenderfulGen {
    pub fn new(config: RenderfulConfig) -> Self {
        Self { config }
    }

    async fn get_job(&self, id: &str) -> Result<Value, ProtocolError> {
        let response = shared_client()
            .get(format!("{}/generations/{id}", self.config.api_url))
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(http_err)?;
        let status = response.status();
        let text = response.text().await.map_err(http_err)?;
        if !status.is_success() {
            return Err(status_err(status.as_u16(), text));
        }
        serde_json::from_str(&text).map_err(|e| parse_err("poll", e))
    }
}

/// The static descriptor the registry filters on for the renderful dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("renderful", "renderful-default", Capability::GenTask)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Image])
}

/// Build a [`RenderfulGen`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<RenderfulGen, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "renderful requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        "renderful-default".to_string()
    } else {
        cfg.model.clone()
    };
    let api_url = normalize_base_url(
        &cfg.api_url
            .clone()
            .unwrap_or_else(|| DEFAULT_API_URL.to_string()),
    );
    Ok(RenderfulGen::new(RenderfulConfig {
        model,
        api_key,
        api_url,
    }))
}

#[async_trait]
impl GenTask for RenderfulGen {
    fn provider_name(&self) -> &str {
        "renderful"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("renderful", self.config.model.clone(), Capability::GenTask)
            .with_input_modalities([Modality::Text])
            .with_output_modalities([Modality::Image])
    }

    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError> {
        let body = build_submit_body(&self.config.model, &req);
        let response = shared_client()
            .post(format!("{}/generations", self.config.api_url))
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(http_err)?;
        let status = response.status();
        let text = response.text().await.map_err(http_err)?;
        if !status.is_success() {
            return Err(status_err(status.as_u16(), text));
        }
        let value: Value = serde_json::from_str(&text).map_err(|e| parse_err("submit", e))?;
        Ok(GenHandle {
            id: parse_job_id(&value),
            provider: Some("renderful".to_string()),
        })
    }

    async fn poll(&self, handle: &GenHandle) -> Result<GenStatus, ProtocolError> {
        let response = self.get_job(&handle.id).await?;
        Ok(map_status(&response))
    }

    async fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError> {
        let response = self.get_job(&handle.id).await?;
        Ok(GenResult {
            assets: parse_assets(&response),
            diagnostic_metadata: json!({ "provider": "renderful", "status": map_status(&response) }),
            lrc: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(prompt: &str, params: Value) -> GenRequest {
        GenRequest {
            prompt: prompt.to_string(),
            params,
        }
    }

    #[test]
    fn submit_body_carries_prompt_model_and_passthrough_params() {
        let body = build_submit_body(
            "flux-pro",
            &request(
                "a cat",
                json!({"seed": 7, "aspect_ratio": "16:9", "negative_prompt": "blurry"}),
            ),
        );
        assert_eq!(body["prompt"], "a cat");
        assert_eq!(body["model"], "flux-pro");
        assert_eq!(body["type"], "text-to-image");
        assert_eq!(body["seed"], 7);
        assert_eq!(body["aspect_ratio"], "16:9");
        assert_eq!(body["negative_prompt"], "blurry");
    }

    #[test]
    fn maps_status_lifecycle() {
        assert_eq!(map_status(&json!({"status": "queued"})), GenStatus::Pending);
        assert_eq!(
            map_status(&json!({"status": "processing"})),
            GenStatus::Running
        );
        assert_eq!(map_status(&json!({"status": "failed"})), GenStatus::Failed);
        assert_eq!(map_status(&json!({"status": "completed"})), GenStatus::Done);
        assert_eq!(map_status(&json!({})), GenStatus::Done);
    }

    #[test]
    fn parses_output_urls_into_assets() {
        let assets = parse_assets(&json!({"outputs": ["https://r/1.png", "https://r/2.png"]}));
        assert_eq!(assets.len(), 2);
        assert_eq!(
            assets[0],
            GenAsset::Url {
                url: "https://r/1.png".to_string(),
                media_type: Some("image/png".to_string()),
            }
        );
    }

    #[test]
    fn normalize_base_url_drops_trailing_generations() {
        assert_eq!(
            normalize_base_url("https://api.renderful.ai/api/v1/generations/"),
            "https://api.renderful.ai/api/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.renderful.ai/api/v1"),
            "https://api.renderful.ai/api/v1"
        );
    }

    #[test]
    fn job_id_falls_back_when_absent() {
        assert_eq!(parse_job_id(&json!({"id": "abc"})), "abc");
        assert_eq!(parse_job_id(&json!({})), "renderful-job");
    }
}
