//! Crazyrouter image generation as the spine [`GenTask`] (Issue 007). Ported from
//! `agent-runtime-aigc-providers`'s `providers/crazyrouter`, over the spine
//! [`ProtocolError`] / [`GenResult`].
//!
//! Crazyrouter is an OpenAI-images-compatible, **synchronous** dialect (Bearer):
//! `POST {base}/v1/images/generations` returns `data[].url` inline — there is no
//! job to poll. It uses [`SyncGenCache`] to present the
//! submit → poll → fetch surface.

use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenHandle, GenRequest, GenResult,
    GenStatus, GenTask, Modality, ProtocolError,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::{shared_client, warn_unconsumed_params};
use serde_json::{json, Value};

use super::SyncGenCache;

const DEFAULT_API_URL: &str = "https://cn.crazyrouter.com";

/// Crazyrouter gen-task configuration.
#[derive(Debug, Clone)]
pub struct CrazyrouterConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Crazyrouter HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Crazyrouter HTTP {code}: {body}"),
    )
    .with_status(code)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_err(e: serde_json::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("failed to parse Crazyrouter response: {e}"),
    )
}

/// [`GenRequest::params`] keys forwarded verbatim onto the images-generations
/// body. `n`/`size` are handled explicitly (with defaults) and so are excluded
/// here; both lists together form the consumed set for the unconsumed-key
/// warning.
const PASSTHROUGH_KEYS: &[&str] = &[
    "quality",
    "background",
    "output_format",
    "user",
    "response_format",
];

/// Build the OpenAI-images `/v1/images/generations` body: `model` + `prompt` +
/// `n` (default 1) + `size` (default `1024x1024`), plus passthrough knobs
/// (`quality` / `background` / `output_format` / `user` / `response_format`).
/// Any other params key warns via [`warn_unconsumed_params`].
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    warn_unconsumed_params(
        "crazyrouter",
        &[PASSTHROUGH_KEYS, &["n", "size"]].concat(),
        &request.params,
    );
    let params = |key: &str| request.params.get(key);
    let mut body = json!({
        "model": model,
        "prompt": request.prompt,
        "n": params("n").and_then(Value::as_u64).unwrap_or(1),
        "size": params("size").and_then(Value::as_str).unwrap_or("1024x1024"),
    });
    if let Some(obj) = request.params.as_object() {
        for key in PASSTHROUGH_KEYS {
            if let Some(value) = obj.get(*key) {
                body[*key] = value.clone();
            }
        }
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
        })
        .collect()
}

/// The Crazyrouter image gen provider as the spine [`GenTask`].
pub struct CrazyrouterGen {
    config: CrazyrouterConfig,
    cache: SyncGenCache,
}

impl CrazyrouterGen {
    pub fn new(config: CrazyrouterConfig) -> Self {
        Self {
            config,
            cache: SyncGenCache::default(),
        }
    }
}

/// The static descriptor the registry filters on for the crazyrouter dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("crazyrouter", "crazyrouter-default", Capability::GenTask)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Image])
}

/// Build a [`CrazyrouterGen`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<CrazyrouterGen, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "crazyrouter requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        "crazyrouter-default".to_string()
    } else {
        cfg.model.clone()
    };
    let api_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_API_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    Ok(CrazyrouterGen::new(CrazyrouterConfig {
        model,
        api_key,
        api_url,
    }))
}

#[async_trait]
impl GenTask for CrazyrouterGen {
    fn provider_name(&self) -> &str {
        "crazyrouter"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new(
            "crazyrouter",
            self.config.model.clone(),
            Capability::GenTask,
        )
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Image])
    }

    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError> {
        let body = build_submit_body(&self.config.model, &req);
        let response = shared_client()
            .post(format!("{}/v1/images/generations", self.config.api_url))
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
            diagnostic_metadata: json!({ "provider": "crazyrouter" }),
            timed_text: None,
        };
        Ok(self.cache.store("crazyrouter", result))
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
    fn submit_body_defaults_and_passthrough() {
        let body = build_submit_body("gpt-image-1", &request("a dog", json!({})));
        assert_eq!(body["model"], "gpt-image-1");
        assert_eq!(body["prompt"], "a dog");
        assert_eq!(body["n"], 1);
        assert_eq!(body["size"], "1024x1024");

        let custom = build_submit_body(
            "m",
            &request(
                "p",
                json!({"n": 3, "size": "512x512", "quality": "high", "output_format": "webp"}),
            ),
        );
        assert_eq!(custom["n"], 3);
        assert_eq!(custom["size"], "512x512");
        assert_eq!(custom["quality"], "high");
        assert_eq!(custom["output_format"], "webp");
    }

    #[test]
    fn parses_data_urls_into_assets() {
        let assets = parse_assets(
            &json!({"data": [{"url": "https://c/a.png"}, {"url": "https://c/b.png"}]}),
        );
        assert_eq!(assets.len(), 2);
        assert_eq!(
            assets[0],
            GenAsset::Url {
                url: "https://c/a.png".to_string(),
                media_type: Some("image/png".to_string()),
            }
        );
        assert!(parse_assets(&json!({})).is_empty());
    }
}
