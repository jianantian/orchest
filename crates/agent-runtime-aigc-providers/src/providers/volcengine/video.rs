//! Volcengine Ark (火山方舟) video generation adapter — Seedance models.
//!
//! Endpoints:
//!   POST   <https://ark.cn-beijing.volces.com/api/v3/contents/generations/tasks>
//!   GET    <https://ark.cn-beijing.volces.com/api/v3/contents/generations/tasks/>{id}
//!   DELETE <https://ark.cn-beijing.volces.com/api/v3/contents/generations/tasks/>{id}
//!   GET    <https://ark.cn-beijing.volces.com/api/v3/contents/generations/tasks>
//! Auth: Authorization: Bearer $ARK_API_KEY
//!
//! Unlike image generation this API is asynchronous: create returns a task id
//! immediately, and callers must poll the query endpoint until the task
//! reaches a terminal status (`succeeded`/`failed`/`cancelled`/`expired`).
//! Generated video URLs are valid for 24h; tasks are queryable for 7 days.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{
    AigcError, AssetRef, ProviderGenerationStatus, ProviderVideoJob, VideoContentItem,
    VideoGenerationRequest, VideoImageRole, VideoProvider, VideoTaskListQuery,
};

const DEFAULT_API_URL: &str = "https://ark.cn-beijing.volces.com/api/v3";

#[derive(Debug, Clone)]
pub struct VolcengineVideoConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: Option<String>,
    pub timeout: Option<Duration>,
}

pub struct VolcengineVideoAdapter {
    config: VolcengineVideoConfig,
    api_base_url: String,
}

impl VolcengineVideoAdapter {
    pub fn from_config(config: VolcengineVideoConfig) -> Result<Self, AigcError> {
        if config.model.trim().is_empty() {
            return Err(AigcError::new("invalid_model", "model cannot be empty"));
        }
        if config.api_key.trim().is_empty() {
            return Err(AigcError::new("invalid_api_key", "API key cannot be empty"));
        }
        let api_base_url = config
            .api_url
            .clone()
            .unwrap_or_else(|| DEFAULT_API_URL.into());
        let api_base_url = api_base_url.trim_end_matches('/').to_string();
        Ok(Self {
            config,
            api_base_url,
        })
    }

    fn asset_to_url_string(asset: &AssetRef) -> Result<Value, AigcError> {
        match asset {
            AssetRef::Url(url) | AssetRef::DataUrl(url) => Ok(json!(url)),
            AssetRef::Base64 { data, mime_type } => {
                Ok(json!(format!("data:{mime_type};base64,{data}")))
            }
            _ => Err(AigcError::new(
                "unsupported_input",
                "Volcengine video adapter requires URL or base64 data URL inputs",
            )
            .provider("volcengine")),
        }
    }

    pub fn build_create_request(
        &self,
        request: &VideoGenerationRequest,
    ) -> Result<Value, AigcError> {
        if request.content.is_empty() {
            return Err(AigcError::new(
                "missing_input",
                "video generation requires at least one content item",
            )
            .provider("volcengine"));
        }

        let content: Vec<Value> = request
            .content
            .iter()
            .map(|item| match item {
                VideoContentItem::Text { text } => Ok(json!({"type": "text", "text": text})),
                VideoContentItem::Image { asset, role } => {
                    let url = Self::asset_to_url_string(asset)?;
                    let role_str = match role {
                        VideoImageRole::FirstFrame => "first_frame",
                        VideoImageRole::LastFrame => "last_frame",
                        VideoImageRole::ReferenceImage => "reference_image",
                    };
                    Ok(json!({
                        "type": "image_url",
                        "image_url": {"url": url},
                        "role": role_str,
                    }))
                }
                VideoContentItem::Video { asset } => {
                    let url = Self::asset_to_url_string(asset)?;
                    Ok(json!({
                        "type": "video_url",
                        "video_url": {"url": url},
                        "role": "reference_video",
                    }))
                }
                VideoContentItem::Audio { asset } => {
                    let url = Self::asset_to_url_string(asset)?;
                    Ok(json!({
                        "type": "audio_url",
                        "audio_url": {"url": url},
                        "role": "reference_audio",
                    }))
                }
                VideoContentItem::DraftTask { id } => Ok(json!({
                    "type": "draft_task",
                    "draft_task": {"id": id},
                })),
            })
            .collect::<Result<_, AigcError>>()?;

        let mut body = json!({
            "model": self.config.model,
            "content": content,
        });

        let cfg = &request.generation_config;
        if let Some(resolution) = &cfg.resolution {
            body["resolution"] = json!(resolution);
        }
        if let Some(ratio) = &cfg.ratio {
            body["ratio"] = json!(ratio);
        }
        if let Some(duration) = cfg.duration_secs {
            body["duration"] = json!(duration);
        }
        if let Some(frames) = cfg.frames {
            body["frames"] = json!(frames);
        }
        if let Some(seed) = cfg.seed {
            body["seed"] = json!(seed);
        }
        if cfg.camera_fixed {
            body["camera_fixed"] = json!(true);
        }
        if cfg.watermark {
            body["watermark"] = json!(true);
        }
        if let Some(generate_audio) = cfg.generate_audio {
            body["generate_audio"] = json!(generate_audio);
        }
        if let Some(service_tier) = &cfg.service_tier {
            body["service_tier"] = json!(service_tier);
        }
        if let Some(priority) = cfg.priority {
            body["priority"] = json!(priority);
        }
        if cfg.draft {
            body["draft"] = json!(true);
        }
        if cfg.return_last_frame {
            body["return_last_frame"] = json!(true);
        }

        if let Some(callback_url) = request
            .provider_options
            .get("callback_url")
            .and_then(|v| v.as_str())
        {
            body["callback_url"] = json!(callback_url);
        }
        if let Some(safety_identifier) = request
            .provider_options
            .get("safety_identifier")
            .and_then(|v| v.as_str())
        {
            body["safety_identifier"] = json!(safety_identifier);
        }

        Ok(body)
    }

    pub fn parse_job(&self, response: Value) -> ProviderVideoJob {
        let id = response
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let raw_status = response
            .get("status")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let status = match raw_status.as_deref() {
            Some("queued") => ProviderGenerationStatus::Queued,
            Some("running") => ProviderGenerationStatus::Running,
            Some("succeeded") => ProviderGenerationStatus::Completed,
            Some("expired") => ProviderGenerationStatus::TimedOut,
            // "cancelled" and any unrecognized status are treated as terminal failures;
            // the raw string is preserved in `raw_status` for callers that need it.
            _ => ProviderGenerationStatus::Failed,
        };
        // Newly created tasks only return `{"id": "..."}` with no status field yet.
        let status = if raw_status.is_none() {
            ProviderGenerationStatus::Queued
        } else {
            status
        };

        let video_url = response
            .pointer("/content/video_url")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let last_frame_url = response
            .pointer("/content/last_frame_url")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let error = response
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        ProviderVideoJob {
            id,
            status,
            raw_status,
            video_url,
            last_frame_url,
            error,
            metadata: response,
        }
    }

    fn tasks_url(&self) -> String {
        format!("{}/contents/generations/tasks", self.api_base_url)
    }
}

#[async_trait]
impl VideoProvider for VolcengineVideoAdapter {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    async fn create_video_generation(
        &self,
        request: &VideoGenerationRequest,
    ) -> Result<ProviderVideoJob, AigcError> {
        let body = self.build_create_request(request)?;
        let mut builder = crate::http::shared_client()
            .post(self.tasks_url())
            .bearer_auth(&self.config.api_key)
            .json(&body);
        if let Some(timeout) = self.config.timeout {
            builder = builder.timeout(timeout);
        }
        let response = builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("volcengine")
        })?;
        let response = crate::providers::parse_json_response("volcengine", response).await?;
        Ok(self.parse_job(response))
    }

    async fn get_video_generation(&self, job_id: &str) -> Result<ProviderVideoJob, AigcError> {
        let url = format!("{}/{job_id}", self.tasks_url());
        let mut builder = crate::http::shared_client()
            .get(url)
            .bearer_auth(&self.config.api_key);
        if let Some(timeout) = self.config.timeout {
            builder = builder.timeout(timeout);
        }
        let response = builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("volcengine")
        })?;
        let response = crate::providers::parse_json_response("volcengine", response).await?;
        Ok(self.parse_job(response))
    }

    async fn cancel_video_generation(&self, job_id: &str) -> Result<(), AigcError> {
        let url = format!("{}/{job_id}", self.tasks_url());
        let mut builder = crate::http::shared_client()
            .delete(url)
            .bearer_auth(&self.config.api_key);
        if let Some(timeout) = self.config.timeout {
            builder = builder.timeout(timeout);
        }
        let response = builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("volcengine")
        })?;
        crate::providers::parse_json_response("volcengine", response).await?;
        Ok(())
    }

    async fn list_video_generations(
        &self,
        query: &VideoTaskListQuery,
    ) -> Result<(Vec<ProviderVideoJob>, u64), AigcError> {
        let mut url = reqwest::Url::parse(&self.tasks_url())
            .map_err(|err| AigcError::new("invalid_url", err.to_string()).provider("volcengine"))?;
        {
            let mut pairs = url.query_pairs_mut();
            if let Some(page_num) = query.page_num {
                pairs.append_pair("page_num", &page_num.to_string());
            }
            if let Some(page_size) = query.page_size {
                pairs.append_pair("page_size", &page_size.to_string());
            }
            if let Some(status) = &query.status {
                pairs.append_pair("filter.status", status);
            }
            if let Some(model) = &query.model {
                pairs.append_pair("filter.model", model);
            }
            for task_id in &query.task_ids {
                pairs.append_pair("filter.task_ids", task_id);
            }
        }

        let mut builder = crate::http::shared_client()
            .get(url)
            .bearer_auth(&self.config.api_key);
        if let Some(timeout) = self.config.timeout {
            builder = builder.timeout(timeout);
        }
        let response = builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("volcengine")
        })?;
        let response = crate::providers::parse_json_response("volcengine", response).await?;
        let total = response.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        let items = response
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|item| self.parse_job(item))
            .collect();
        Ok((items, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VideoGenerationConfig;
    use serde_json::json;

    fn adapter() -> VolcengineVideoAdapter {
        VolcengineVideoAdapter::from_config(VolcengineVideoConfig {
            model: "doubao-seedance-1-0-pro-250528".into(),
            api_key: "test-key".into(),
            api_url: None,
            timeout: None,
        })
        .unwrap()
    }

    #[test]
    fn build_create_request_text_only() {
        let request = VideoGenerationRequest {
            content: vec![VideoContentItem::Text {
                text: "a cat playing piano".into(),
            }],
            generation_config: VideoGenerationConfig {
                resolution: Some("480p".into()),
                duration_secs: Some(4),
                ..Default::default()
            },
            execution_config: Default::default(),
            provider_options: json!({}),
        };
        let body = adapter().build_create_request(&request).unwrap();
        assert_eq!(body["model"], "doubao-seedance-1-0-pro-250528");
        assert_eq!(body["content"][0]["type"], "text");
        assert_eq!(body["content"][0]["text"], "a cat playing piano");
        assert_eq!(body["resolution"], "480p");
        assert_eq!(body["duration"], 4);
    }

    #[test]
    fn build_create_request_with_first_frame_image() {
        let request = VideoGenerationRequest {
            content: vec![
                VideoContentItem::Text {
                    text: "zoom in slowly".into(),
                },
                VideoContentItem::Image {
                    asset: AssetRef::Url("https://example.com/frame.png".into()),
                    role: VideoImageRole::FirstFrame,
                },
            ],
            generation_config: Default::default(),
            execution_config: Default::default(),
            provider_options: json!({}),
        };
        let body = adapter().build_create_request(&request).unwrap();
        assert_eq!(body["content"][1]["type"], "image_url");
        assert_eq!(
            body["content"][1]["image_url"]["url"],
            "https://example.com/frame.png"
        );
        assert_eq!(body["content"][1]["role"], "first_frame");
    }

    #[test]
    fn rejects_empty_content() {
        let request = VideoGenerationRequest {
            content: vec![],
            generation_config: Default::default(),
            execution_config: Default::default(),
            provider_options: json!({}),
        };
        let err = adapter().build_create_request(&request).unwrap_err();
        assert_eq!(err.code, "missing_input");
    }

    #[test]
    fn parse_job_create_response_has_no_status_yet() {
        let job = adapter().parse_job(json!({"id": "cgt-123"}));
        assert_eq!(job.id, "cgt-123");
        assert_eq!(job.status, ProviderGenerationStatus::Queued);
        assert!(job.raw_status.is_none());
    }

    #[test]
    fn parse_job_maps_succeeded_to_completed_with_video_url() {
        let job = adapter().parse_job(json!({
            "id": "cgt-123",
            "status": "succeeded",
            "content": {"video_url": "https://example.com/out.mp4"}
        }));
        assert_eq!(job.status, ProviderGenerationStatus::Completed);
        assert_eq!(job.raw_status, Some("succeeded".into()));
        assert_eq!(job.video_url, Some("https://example.com/out.mp4".into()));
    }

    #[test]
    fn parse_job_maps_running_and_queued() {
        let running = adapter().parse_job(json!({"id": "a", "status": "running"}));
        assert_eq!(running.status, ProviderGenerationStatus::Running);
        let queued = adapter().parse_job(json!({"id": "a", "status": "queued"}));
        assert_eq!(queued.status, ProviderGenerationStatus::Queued);
    }

    #[test]
    fn parse_job_maps_failed_and_cancelled_to_failed() {
        let failed = adapter().parse_job(json!({
            "id": "a", "status": "failed", "error": {"message": "boom"}
        }));
        assert_eq!(failed.status, ProviderGenerationStatus::Failed);
        assert_eq!(failed.error, Some("boom".into()));

        let cancelled = adapter().parse_job(json!({"id": "a", "status": "cancelled"}));
        assert_eq!(cancelled.status, ProviderGenerationStatus::Failed);
        assert_eq!(cancelled.raw_status, Some("cancelled".into()));
    }

    #[test]
    fn parse_job_maps_expired_to_timed_out() {
        let job = adapter().parse_job(json!({"id": "a", "status": "expired"}));
        assert_eq!(job.status, ProviderGenerationStatus::TimedOut);
    }
}
