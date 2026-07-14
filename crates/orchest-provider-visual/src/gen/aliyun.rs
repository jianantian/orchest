//! Aliyun DashScope (wanx) image generation as the spine [`GenTask`] (Issue 007).
//! Ported from `agent-runtime-aigc-providers`'s `providers/aliyun` (the wanx async
//! path), over the spine [`ProtocolError`] / [`GenResult`].
//!
//! Wanx is a genuine **submit → poll → fetch** async dialect (Bearer): `POST
//! {base}/services/aigc/text2image/image-synthesis` with `X-DashScope-Async:
//! enable` returns `output.task_id`; `GET {base}/tasks/{id}` reports
//! `output.task_status` (PENDING / RUNNING / SUCCEEDED / FAILED) and
//! `output.results[].url`.

use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenHandle, GenRequest, GenResult,
    GenStatus, GenTask, Modality, ProtocolError,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::shared_client;
use serde_json::{json, Value};

const DEFAULT_API_URL: &str = "https://dashscope.aliyuncs.com/api/v1";

/// Aliyun (wanx) gen-task configuration.
#[derive(Debug, Clone)]
pub struct AliyunGenConfig {
    pub model: String,
    pub api_key: String,
    pub api_base_url: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Aliyun gen HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Aliyun gen HTTP {code}: {body}"),
    )
    .with_status(code)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_err(what: &str, e: serde_json::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("failed to parse Aliyun gen {what} response: {e}"),
    )
}

/// Trim a configured base URL to the API root (drop a trailing service path).
fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/')
        .trim_end_matches("/services/aigc/text2image/image-synthesis")
        .trim_end_matches("/tasks")
        .to_string()
}

/// Build the `image-synthesis` body: `{ model, input: { prompt }, parameters }`.
/// Spine [`GenRequest::params`] passes through as `parameters` (size / n / seed /
/// negative_prompt / prompt_extend / …), defaulting `size` to `1024*1024`.
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    let mut parameters = json!({});
    if let Some(params) = request.params.as_object() {
        for (key, value) in params {
            parameters[key] = value.clone();
        }
    }
    if parameters.get("size").is_none() {
        parameters["size"] = json!("1024*1024");
    }
    json!({
        "model": model,
        "input": { "prompt": request.prompt },
        "parameters": parameters,
    })
}

/// Map the wanx `output.task_status` onto the spine lifecycle.
pub fn map_status(response: &Value) -> GenStatus {
    match response
        .pointer("/output/task_status")
        .and_then(Value::as_str)
        .unwrap_or("SUCCEEDED")
    {
        "PENDING" => GenStatus::Pending,
        "RUNNING" => GenStatus::Running,
        "FAILED" | "UNKNOWN" => GenStatus::Failed,
        _ => GenStatus::Done,
    }
}

/// Collect `output.results[].url` (or `.image`) into spine assets.
pub fn parse_assets(response: &Value) -> Vec<GenAsset> {
    response
        .pointer("/output/results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.get("url")
                .or_else(|| item.get("image"))
                .and_then(Value::as_str)
        })
        .map(|url| GenAsset::Url {
            url: url.to_string(),
            media_type: Some("image/png".to_string()),
        })
        .collect()
}

fn parse_task_id(response: &Value) -> String {
    response
        .pointer("/output/task_id")
        .and_then(Value::as_str)
        .unwrap_or("aliyun-task")
        .to_string()
}

/// The Aliyun (wanx) image gen provider as the spine [`GenTask`].
pub struct AliyunGen {
    config: AliyunGenConfig,
}

impl AliyunGen {
    pub fn new(config: AliyunGenConfig) -> Self {
        Self { config }
    }

    async fn get_task(&self, id: &str) -> Result<Value, ProtocolError> {
        let response = shared_client()
            .get(format!("{}/tasks/{id}", self.config.api_base_url))
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

/// The static descriptor the registry filters on for the aliyun gen dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("aliyun", "wanx2.1-t2i-turbo", Capability::GenTask)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Image])
}

/// Build an [`AliyunGen`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<AliyunGen, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "aliyun gen requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        "wanx2.1-t2i-turbo".to_string()
    } else {
        cfg.model.clone()
    };
    let api_base_url = normalize_base_url(
        &cfg.api_url
            .clone()
            .unwrap_or_else(|| DEFAULT_API_URL.to_string()),
    );
    Ok(AliyunGen::new(AliyunGenConfig {
        model,
        api_key,
        api_base_url,
    }))
}

#[async_trait]
impl GenTask for AliyunGen {
    fn provider_name(&self) -> &str {
        "aliyun"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("aliyun", self.config.model.clone(), Capability::GenTask)
            .with_input_modalities([Modality::Text])
            .with_output_modalities([Modality::Image])
    }

    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError> {
        let body = build_submit_body(&self.config.model, &req);
        let response = shared_client()
            .post(format!(
                "{}/services/aigc/text2image/image-synthesis",
                self.config.api_base_url
            ))
            .bearer_auth(&self.config.api_key)
            .header("X-DashScope-Async", "enable")
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
            id: parse_task_id(&value),
            provider: Some("aliyun".to_string()),
        })
    }

    async fn poll(&self, handle: &GenHandle) -> Result<GenStatus, ProtocolError> {
        let response = self.get_task(&handle.id).await?;
        Ok(map_status(&response))
    }

    async fn fetch(&self, handle: &GenHandle) -> Result<GenResult, ProtocolError> {
        let response = self.get_task(&handle.id).await?;
        Ok(GenResult {
            assets: parse_assets(&response),
            diagnostic_metadata: json!({ "provider": "aliyun", "status": map_status(&response) }),
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
    fn submit_body_nests_prompt_and_passes_params_with_size_default() {
        let body = build_submit_body(
            "wanx2.1-t2i-turbo",
            &request("a fox", json!({"n": 2, "seed": 9})),
        );
        assert_eq!(body["model"], "wanx2.1-t2i-turbo");
        assert_eq!(body["input"]["prompt"], "a fox");
        assert_eq!(body["parameters"]["n"], 2);
        assert_eq!(body["parameters"]["seed"], 9);
        assert_eq!(body["parameters"]["size"], "1024*1024");

        let sized = build_submit_body("m", &request("p", json!({"size": "768*768"})));
        assert_eq!(sized["parameters"]["size"], "768*768");
    }

    #[test]
    fn maps_wanx_task_status() {
        assert_eq!(
            map_status(&json!({"output": {"task_status": "PENDING"}})),
            GenStatus::Pending
        );
        assert_eq!(
            map_status(&json!({"output": {"task_status": "RUNNING"}})),
            GenStatus::Running
        );
        assert_eq!(
            map_status(&json!({"output": {"task_status": "FAILED"}})),
            GenStatus::Failed
        );
        assert_eq!(
            map_status(&json!({"output": {"task_status": "SUCCEEDED"}})),
            GenStatus::Done
        );
    }

    #[test]
    fn parses_results_into_assets_and_task_id() {
        let response = json!({"output": {
            "task_id": "t-123",
            "results": [{"url": "https://a/1.png"}, {"image": "https://a/2.png"}]
        }});
        assert_eq!(parse_task_id(&response), "t-123");
        let assets = parse_assets(&response);
        assert_eq!(assets.len(), 2);
        assert_eq!(
            assets[1],
            GenAsset::Url {
                url: "https://a/2.png".to_string(),
                media_type: Some("image/png".to_string())
            }
        );
    }

    #[test]
    fn normalize_base_url_drops_service_suffixes() {
        assert_eq!(
            normalize_base_url(
                "https://dashscope.aliyuncs.com/api/v1/services/aigc/text2image/image-synthesis"
            ),
            "https://dashscope.aliyuncs.com/api/v1"
        );
    }
}
