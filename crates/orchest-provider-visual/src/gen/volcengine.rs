//! Volcengine Ark image generation as the spine [`GenTask`] (Issue 007). Ported
//! from `agent-runtime-aigc-providers`'s `providers/volcengine/image`, over the
//! spine [`ProtocolError`] / [`GenResult`].
//!
//! Ark image is OpenAI-images-compatible and **synchronous** (Bearer auth): `POST
//! {base}/images/generations` returns `data[].url` inline. Like crazyrouter it
//! uses [`SyncGenCache`] for the submit → poll → fetch
//! surface. (The Ark image API authenticates with a Bearer key, not AK/SK
//! signing.)

use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenAssetRole, GenHandle, GenRequest,
    GenResult, GenStatus, GenTask, Modality, ProtocolError,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::{shared_client, warn_unconsumed_params};
use serde_json::{json, Value};

use super::SyncGenCache;

const DEFAULT_API_URL: &str = "https://ark.cn-beijing.volces.com/api/v3";
const DEFAULT_MODEL: &str = "doubao-seedream-5-0-260128";

/// Volcengine Ark gen-task configuration.
#[derive(Debug, Clone)]
pub struct VolcengineGenConfig {
    pub model: String,
    pub api_key: String,
    pub api_base_url: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Volcengine gen HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Volcengine gen HTTP {code}: {body}"),
    )
    .with_status(code)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_err(e: serde_json::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("failed to parse Volcengine gen response: {e}"),
    )
}

/// [`GenRequest::params`] keys the Ark images API consumes here. Anything else
/// warns via [`warn_unconsumed_params`] — it is dropped from the body.
const CONSUMED_PARAMS: &[&str] = &["n", "watermark", "size", "image"];

/// Build the Ark `/images/generations` body. Carries `model`, `prompt`, `n`
/// (default 1) and `response_format: url`, plus `watermark` (default false), an
/// optional `size` (omitted when `auto`), and an optional `image` (a URL/data-url
/// for img2img) — the latter three drawn from [`GenRequest::params`].
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    warn_unconsumed_params("volcengine", CONSUMED_PARAMS, &request.params);
    let params = |key: &str| request.params.get(key);
    let mut body = json!({
        "model": model,
        "prompt": request.prompt,
        "n": params("n").and_then(Value::as_u64).unwrap_or(1),
        "response_format": "url",
        "watermark": params("watermark").and_then(Value::as_bool).unwrap_or(false),
    });
    if let Some(size) = params("size").and_then(Value::as_str) {
        if size != "auto" {
            body["size"] = json!(size);
        }
    }
    if let Some(image) = params("image") {
        body["image"] = image.clone();
    }
    body
}

/// Collect `data[].url` into spine assets.
pub fn parse_assets(response: &Value) -> Vec<GenAsset> {
    response
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("url").and_then(Value::as_str))
        .map(|url| GenAsset::Url {
            url: url.to_string(),
            media_type: Some("image/png".to_string()),
            role: GenAssetRole::Primary,
        })
        .collect()
}

/// The Volcengine Ark image gen provider as the spine [`GenTask`].
pub struct VolcengineGen {
    config: VolcengineGenConfig,
    cache: SyncGenCache,
}

impl VolcengineGen {
    pub fn new(config: VolcengineGenConfig) -> Self {
        Self {
            config,
            cache: SyncGenCache::default(),
        }
    }
}

/// The static descriptor the registry filters on for the volcengine gen dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("volcengine", DEFAULT_MODEL, Capability::GenTask)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Image])
        .default_for_provider(true)
}

/// Build a [`VolcengineGen`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<VolcengineGen, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "volcengine gen requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        cfg.model.clone()
    };
    let api_base_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_API_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    Ok(VolcengineGen::new(VolcengineGenConfig {
        model,
        api_key,
        api_base_url,
    }))
}

#[async_trait]
impl GenTask for VolcengineGen {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("volcengine", self.config.model.clone(), Capability::GenTask)
            .with_input_modalities([Modality::Text])
            .with_output_modalities([Modality::Image])
    }

    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError> {
        let body = build_submit_body(&self.config.model, &req);
        let response = shared_client()
            .post(format!("{}/images/generations", self.config.api_base_url))
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
        let value: Value = serde_json::from_str(&text).map_err(parse_err)?;
        let result = GenResult {
            assets: parse_assets(&value),
            diagnostic_metadata: json!({ "provider": "volcengine" }),
            timed_text: None,
            duration_secs: None,
            track_meta: Vec::new(),
        };
        Ok(self.cache.store("volcengine", result))
    }
    async fn poll(&self, handle: &GenHandle) -> Result<GenStatus, ProtocolError> {
        Ok(self.cache.status(handle))
    }

    async fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError> {
        self.cache.fetch(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(prompt: &str, params: Value) -> GenRequest {
        GenRequest {
            prompt: prompt.to_string(),
            params,
            music: None,
        }
    }

    #[test]
    fn submit_body_defaults_and_optional_fields() {
        let body = build_submit_body(DEFAULT_MODEL, &request("a ship", json!({})));
        assert_eq!(body["model"], DEFAULT_MODEL);
        assert_eq!(body["prompt"], "a ship");
        assert_eq!(body["n"], 1);
        assert_eq!(body["response_format"], "url");
        assert_eq!(body["watermark"], false);
        assert!(body.get("size").is_none());
        assert!(body.get("image").is_none());

        let custom = build_submit_body(
            "m",
            &request(
                "p",
                json!({"n": 2, "size": "1024x1024", "watermark": true, "image": "https://r/in.png"}),
            ),
        );
        assert_eq!(custom["n"], 2);
        assert_eq!(custom["size"], "1024x1024");
        assert_eq!(custom["watermark"], true);
        assert_eq!(custom["image"], "https://r/in.png");
    }

    #[test]
    fn size_auto_is_omitted() {
        let body = build_submit_body("m", &request("p", json!({"size": "auto"})));
        assert!(body.get("size").is_none());
    }

    #[test]
    fn parses_data_urls_into_assets() {
        let assets = parse_assets(&json!({"data": [{"url": "https://v/1.png"}]}));
        assert_eq!(
            assets,
            vec![GenAsset::Url {
                url: "https://v/1.png".to_string(),
                media_type: Some("image/png".to_string()),
                role: GenAssetRole::Primary,
            }]
        );
    }
}
