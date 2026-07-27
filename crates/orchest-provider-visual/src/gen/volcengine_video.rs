//! Volcengine Ark **video** generation as the spine [`GenTask`] (Issue 007).
//! Ported from `agent-runtime-aigc-providers`'s `providers/volcengine/video`, over
//! the spine [`ProtocolError`] / [`GenResult`].
//!
//! Ark video is a genuine **submit → poll → fetch** async dialect (Bearer): `POST
//! {base}/contents/generations/tasks` returns `{id}`; `GET
//! {base}/contents/generations/tasks/{id}` reports `status` (queued / running /
//! succeeded / failed / cancelled / expired) and `content.video_url`.

use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, ErrorCode, GenAsset, GenAssetRole, GenHandle, GenRequest,
    GenResult, GenStatus, GenTask, Modality, ProtocolError,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::{shared_client, warn_unconsumed_params};
use serde_json::{json, Value};

const DEFAULT_API_URL: &str = "https://ark.cn-beijing.volces.com/api/v3";
const DEFAULT_MODEL: &str = "doubao-seedance-1-0-pro";

/// Volcengine Ark video gen-task configuration.
#[derive(Debug, Clone)]
pub struct VolcengineVideoConfig {
    pub model: String,
    pub api_key: String,
    pub api_base_url: String,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn http_err(e: reqwest::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Volcengine video HTTP error: {e}"),
    )
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn status_err(code: u16, body: String) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("Volcengine video HTTP {code}: {body}"),
    )
    .with_status(code)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_err(what: &str, e: serde_json::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderHttpError,
        format!("failed to parse Volcengine video {what} response: {e}"),
    )
}

/// [`GenRequest::params`] keys the Ark video create-task API understands (a
/// `content` override for image/last-frame roles, plus the config knobs).
/// `model` is set explicitly and skipped in the passthrough; anything outside
/// this set warns via [`warn_unconsumed_params`] — it would be forwarded
/// verbatim but have no effect on the API.
const CONSUMED_PARAMS: &[&str] = &["content", "resolution", "ratio", "duration", "seed"];

/// Build the create-task body. `content` defaults to a single text item from the
/// prompt; any [`GenRequest::params`] (a `content` override for image/last-frame
/// roles, plus `resolution` / `ratio` / `duration` / `seed` / … config) pass
/// through.
pub fn build_submit_body(model: &str, request: &GenRequest) -> Value {
    warn_unconsumed_params("volcengine", CONSUMED_PARAMS, &request.params);
    let mut body = json!({
        "model": model,
        "content": [{ "type": "text", "text": request.prompt }],
    });
    if let Some(params) = request.params.as_object() {
        for (key, value) in params {
            if key != "model" {
                body[key] = value.clone();
            }
        }
    }
    body
}

/// Map the Ark video task `status` onto the spine lifecycle. A freshly created
/// task with no `status` yet is `Pending`; `queued` → `Pending`, `running` →
/// `Running`, `succeeded` → `Done`, anything else (failed/cancelled/expired) →
/// `Failed`.
pub fn map_status(response: &Value) -> GenStatus {
    match response.get("status").and_then(Value::as_str) {
        None | Some("queued") => GenStatus::Pending,
        Some("running") => GenStatus::Running,
        Some("succeeded") => GenStatus::Done,
        _ => GenStatus::Failed,
    }
}

/// Collect `content.video_url` (and `content.last_frame_url` if present) as assets.
pub fn parse_assets(response: &Value) -> Vec<GenAsset> {
    let mut assets = Vec::new();
    if let Some(url) = response
        .pointer("/content/video_url")
        .and_then(Value::as_str)
    {
        assets.push(GenAsset::Url {
            url: url.to_string(),
            media_type: Some("video/mp4".to_string()),
            role: GenAssetRole::Primary,
        });
    }
    if let Some(url) = response
        .pointer("/content/last_frame_url")
        .and_then(Value::as_str)
    {
        // The last-frame image is a still derived from the video, not the
        // primary product (the MP4 above is) — tagged Preview so consumers
        // collecting Primary assets never pick up a stray image.
        assets.push(GenAsset::Url {
            url: url.to_string(),
            media_type: Some("image/png".to_string()),
            role: GenAssetRole::Preview,
        });
    }
    assets
}

fn parse_task_id(response: &Value) -> String {
    response
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("volcengine-video-task")
        .to_string()
}

/// The Volcengine Ark video gen provider as the spine [`GenTask`].
pub struct VolcengineVideoGen {
    config: VolcengineVideoConfig,
}

impl VolcengineVideoGen {
    pub fn new(config: VolcengineVideoConfig) -> Self {
        Self { config }
    }

    async fn get_task(&self, id: &str) -> Result<Value, ProtocolError> {
        let response = shared_client()
            .get(format!(
                "{}/contents/generations/tasks/{id}",
                self.config.api_base_url
            ))
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

/// The static descriptor the registry filters on for the volcengine video dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("volcengine", DEFAULT_MODEL, Capability::GenTask)
        .with_input_modalities([Modality::Text, Modality::Image])
        .with_output_modalities([Modality::Video])
}

/// Build a [`VolcengineVideoGen`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<VolcengineVideoGen, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(
            ErrorCode::MissingApiKey,
            "volcengine video requires api_key",
        )
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
    Ok(VolcengineVideoGen::new(VolcengineVideoConfig {
        model,
        api_key,
        api_base_url,
    }))
}

#[async_trait]
impl GenTask for VolcengineVideoGen {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("volcengine", self.config.model.clone(), Capability::GenTask)
            .with_input_modalities([Modality::Text, Modality::Image])
            .with_output_modalities([Modality::Video])
    }

    async fn submit(&self, req: GenRequest) -> Result<GenHandle, ProtocolError> {
        let body = build_submit_body(&self.config.model, &req);
        let response = shared_client()
            .post(format!(
                "{}/contents/generations/tasks",
                self.config.api_base_url
            ))
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
            id: parse_task_id(&value),
            provider: Some("volcengine".to_string()),
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
            diagnostic_metadata: json!({ "provider": "volcengine", "status": map_status(&response) }),
            timed_text: None,
            duration_secs: None,
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
            music: None,
        }
    }

    #[test]
    fn submit_body_defaults_text_content_and_passes_config() {
        let body = build_submit_body(
            DEFAULT_MODEL,
            &request(
                "a drone shot",
                json!({"resolution": "1080p", "duration": 5}),
            ),
        );
        assert_eq!(body["model"], DEFAULT_MODEL);
        assert_eq!(body["content"][0]["type"], "text");
        assert_eq!(body["content"][0]["text"], "a drone shot");
        assert_eq!(body["resolution"], "1080p");
        assert_eq!(body["duration"], 5);

        // params may override content for image-to-video
        let override_content = build_submit_body(
            "m",
            &request(
                "ignored",
                json!({"content": [{"type": "image_url", "image_url": {"url": "https://x/in.png"}, "role": "first_frame"}]}),
            ),
        );
        assert_eq!(override_content["content"][0]["type"], "image_url");
    }

    #[test]
    fn maps_video_task_status() {
        assert_eq!(map_status(&json!({})), GenStatus::Pending); // freshly created, no status
        assert_eq!(map_status(&json!({"status": "queued"})), GenStatus::Pending);
        assert_eq!(
            map_status(&json!({"status": "running"})),
            GenStatus::Running
        );
        assert_eq!(map_status(&json!({"status": "succeeded"})), GenStatus::Done);
        assert_eq!(map_status(&json!({"status": "expired"})), GenStatus::Failed);
        assert_eq!(
            map_status(&json!({"status": "cancelled"})),
            GenStatus::Failed
        );
    }

    #[test]
    fn parses_video_url_and_task_id() {
        let response = json!({
            "id": "task-9",
            "status": "succeeded",
            "content": {"video_url": "https://v/out.mp4"}
        });
        assert_eq!(parse_task_id(&response), "task-9");
        let assets = parse_assets(&response);
        assert_eq!(
            assets,
            vec![GenAsset::Url {
                url: "https://v/out.mp4".to_string(),
                media_type: Some("video/mp4".to_string()),
                role: GenAssetRole::Primary,
            }]
        );
    }

    #[test]
    fn last_frame_is_tagged_preview_not_primary() {
        let response = json!({
            "id": "task-10",
            "status": "succeeded",
            "content": {"video_url": "https://v/out.mp4", "last_frame_url": "https://v/last.png"}
        });
        let assets = parse_assets(&response);
        assert_eq!(assets.len(), 2);
        assert!(matches!(
            &assets[0],
            GenAsset::Url {
                role: GenAssetRole::Primary,
                ..
            }
        ));
        assert!(matches!(
            &assets[1],
            GenAsset::Url {
                role: GenAssetRole::Preview,
                ..
            }
        ));
    }
}
