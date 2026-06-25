//! Minimax video generation adapter (Hailuo + T2V/I2V/Frame2V/S2V models).
//!
//! Endpoints (Minimax video API,见 `docs/external/minimax/video/`):
//!   POST `https://api.minimaxi.com/v1/video_generation` — 创建任务
//!   GET  `https://api.minimaxi.com/v1/query/video_generation?task_id={id}` — 查询状态
//!   GET  `https://api.minimaxi.com/v1/files/retrieve?file_id={id}` — 拿 download_url
//! Auth: `Authorization: Bearer $MINIMAX_API_KEY`
//!
//! 5 个 video 变体(T2V / I2V / Frame2V / Subject Reference / `S2V-01`)走同一 POST,
//! 仅 model 字段与必填的图片角色不同。状态映射:
//!   Preparing/Queueing → Queued;Processing → Running;
//!   Success → Completed(file_id 取 download_url);Fail → Failed。
//!
//! 不实现 `callback_url` webhook(PRD 非目标 / 设计文档 §5.5 / §七 Q4)。

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use serde_json::{json, Value};

use crate::{
    AigcError, AssetRef, ProviderGenerationStatus, ProviderVideoJob, VideoContentItem,
    VideoGenerationRequest, VideoImageRole, VideoProvider, VideoTaskListQuery,
};

const DEFAULT_API_URL: &str = "https://api.minimaxi.com";

#[derive(Debug, Clone)]
pub struct MinimaxVideoConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: Option<String>,
    pub timeout: Option<Duration>,
}

pub struct MinimaxVideoAdapter {
    config: MinimaxVideoConfig,
    api_base_url: String,
}

impl MinimaxVideoAdapter {
    pub fn from_config(config: MinimaxVideoConfig) -> Result<Self, AigcError> {
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

    /// 把 `AssetRef` 6 variant 具体化为 Minimax 接受的 URL 或 `data:<mime>;base64,...`
    /// 字符串(spec §3e 映射表)。
    pub(crate) async fn asset_to_minimax_input(asset: &AssetRef) -> Result<String, AigcError> {
        match asset {
            AssetRef::Url(url) => Ok(url.clone()),
            AssetRef::DataUrl(data_url) => Ok(data_url.clone()),
            AssetRef::Base64 { data, mime_type } => Ok(format!("data:{mime_type};base64,{data}")),
            AssetRef::Bytes { bytes, mime_type } => {
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                Ok(format!("data:{mime_type};base64,{encoded}"))
            }
            AssetRef::LocalPath(path) => {
                let bytes = tokio::fs::read(path).await.map_err(|err| {
                    AigcError::new(
                        "asset_read_failed",
                        format!("failed to read local asset {path}: {err}"),
                    )
                    .provider("minimax")
                })?;
                let mime_type = infer_mime_from_path(path);
                let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                Ok(format!("data:{mime_type};base64,{encoded}"))
            }
            AssetRef::Stored { .. } => Err(AigcError::new(
                "unsupported_input",
                "Minimax 视频图片输入需要可外网访问的 URL 或内联 base64;请先通过 \
                 resolve_asset_url 把 Stored asset 解析为 Url",
            )
            .provider("minimax")),
        }
    }

    pub async fn build_create_request(
        &self,
        request: &VideoGenerationRequest,
    ) -> Result<Value, AigcError> {
        if request.content.is_empty() {
            return Err(AigcError::new(
                "missing_input",
                "video generation requires at least one content item",
            )
            .provider("minimax"));
        }

        let mut prompt: Option<String> = None;
        let mut first_frame_image: Option<String> = None;
        let mut last_frame_image: Option<String> = None;
        let mut subject_reference: Vec<String> = Vec::new();

        for item in &request.content {
            match item {
                VideoContentItem::Text { text } => {
                    prompt = Some(match prompt.take() {
                        Some(existing) => format!("{existing}\n{text}"),
                        None => text.clone(),
                    });
                }
                VideoContentItem::Image { asset, role } => {
                    let url = Self::asset_to_minimax_input(asset).await?;
                    match role {
                        VideoImageRole::FirstFrame => first_frame_image = Some(url),
                        VideoImageRole::LastFrame => last_frame_image = Some(url),
                        VideoImageRole::ReferenceImage => subject_reference.push(url),
                    }
                }
                VideoContentItem::Video { .. } => {
                    return Err(AigcError::new(
                        "unsupported_input",
                        "Minimax video API does not accept video inputs",
                    )
                    .provider("minimax"));
                }
                VideoContentItem::Audio { .. } => {
                    return Err(AigcError::new(
                        "unsupported_input",
                        "Minimax video API does not accept audio inputs",
                    )
                    .provider("minimax"));
                }
                VideoContentItem::DraftTask { .. } => {
                    return Err(AigcError::new(
                        "unsupported_input",
                        "Minimax video API does not accept draft_task inputs",
                    )
                    .provider("minimax"));
                }
            }
        }

        let mut body = json!({ "model": self.config.model });
        if let Some(p) = prompt {
            body["prompt"] = json!(p);
        }
        if let Some(ff) = first_frame_image {
            body["first_frame_image"] = json!(ff);
        }
        if let Some(lf) = last_frame_image {
            body["last_frame_image"] = json!(lf);
        }
        if !subject_reference.is_empty() {
            // Minimax `subject_reference` 是数组(`video/refvideo.md`)。
            body["subject_reference"] = json!(subject_reference
                .iter()
                .map(|u| json!({ "image": [u] }))
                .collect::<Vec<_>>());
        }

        let cfg = &request.generation_config;
        if let Some(resolution) = &cfg.resolution {
            body["resolution"] = json!(resolution);
        }
        if let Some(duration) = cfg.duration_secs {
            body["duration"] = json!(duration);
        }
        if cfg.watermark {
            body["aigc_watermark"] = json!(true);
        }

        // Minimax-only `provider_options` 字段:`prompt_optimizer` / `fast_pretreatment`
        // (`video/t2v.md:77,80`)。
        if let Some(prompt_optimizer) = request
            .provider_options
            .get("prompt_optimizer")
            .and_then(|v| v.as_bool())
        {
            body["prompt_optimizer"] = json!(prompt_optimizer);
        }
        if let Some(fast_pretreatment) = request
            .provider_options
            .get("fast_pretreatment")
            .and_then(|v| v.as_bool())
        {
            body["fast_pretreatment"] = json!(fast_pretreatment);
        }
        // 注意:不接 `callback_url`(PRD 非目标)。

        Ok(body)
    }
}

fn infer_mime_from_path(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    match std::path::Path::new(&lower)
        .extension()
        .and_then(|s| s.to_str())
    {
        Some("png") => "image/png".into(),
        Some("jpg") | Some("jpeg") => "image/jpeg".into(),
        Some("gif") => "image/gif".into(),
        Some("webp") => "image/webp".into(),
        Some("mp4") => "video/mp4".into(),
        Some("mov") => "video/mov".into(),
        Some("webm") => "video/webm".into(),
        _ => "application/octet-stream".into(),
    }
}

impl MinimaxVideoAdapter {
    /// 把 Minimax `/v1/query/video_generation` 响应解析成 `ProviderVideoJob`。
    /// 状态映射规则(`video/status.md:79-90`):
    ///   Preparing/Queueing → Queued;Processing → Running;Success → Completed;Fail → Failed
    /// Minimax 无显式 timeout 状态;TimedOut 由 gateway 轮询超预算时本地产生。
    pub fn parse_job(&self, response: Value) -> ProviderVideoJob {
        let id = response
            .get("task_id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_default();
        let raw_status = response
            .get("status")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let status = match raw_status.as_deref() {
            Some("Preparing") | Some("Queueing") => ProviderGenerationStatus::Queued,
            Some("Processing") => ProviderGenerationStatus::Running,
            Some("Success") => ProviderGenerationStatus::Completed,
            Some("Fail") => ProviderGenerationStatus::Failed,
            None => {
                // create-task 响应只带 task_id;还未进入状态机,视为已入队。
                ProviderGenerationStatus::Queued
            }
            _ => ProviderGenerationStatus::Failed,
        };

        let file_id = response
            .get("file_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let error = response
            .pointer("/base_resp/status_msg")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && *s != "success")
            .map(str::to_string);

        // Minimax 在查询响应中只给 file_id;真正的 download_url 要再调 /v1/files/retrieve。
        // 这里把 file_id 暂时塞进 metadata,VideoProvider impl 在 get_video_generation 完成
        // 后会自行解析并补全 video_url 字段。
        let mut metadata = response.clone();
        if let Some(fid) = &file_id {
            metadata["file_id"] = json!(fid);
        }

        ProviderVideoJob {
            id,
            status,
            raw_status,
            video_url: None,
            last_frame_url: None,
            error,
            metadata,
        }
    }

    fn create_task_url(&self) -> String {
        format!("{}/v1/video_generation", self.api_base_url)
    }

    fn query_task_url(&self, task_id: &str) -> String {
        format!(
            "{}/v1/query/video_generation?task_id={}",
            self.api_base_url, task_id
        )
    }

    fn files_retrieve_url(&self, file_id: &str) -> String {
        format!(
            "{}/v1/files/retrieve?file_id={}",
            self.api_base_url, file_id
        )
    }

    async fn fetch_download_url(&self, file_id: &str) -> Result<String, AigcError> {
        let mut builder = crate::http::shared_client()
            .get(self.files_retrieve_url(file_id))
            .bearer_auth(&self.config.api_key);
        if let Some(timeout) = self.config.timeout {
            builder = builder.timeout(timeout);
        }
        let response = builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("minimax")
        })?;
        let response = crate::providers::parse_json_response("minimax", response).await?;
        response
            .pointer("/file/download_url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| {
                AigcError::new(
                    "missing_download_url",
                    "Minimax /v1/files/retrieve response missing file.download_url",
                )
                .provider("minimax")
            })
    }
}

#[async_trait]
impl VideoProvider for MinimaxVideoAdapter {
    fn provider_name(&self) -> &str {
        "minimax"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    async fn create_video_generation(
        &self,
        request: &VideoGenerationRequest,
    ) -> Result<ProviderVideoJob, AigcError> {
        let body = self.build_create_request(request).await?;
        let mut builder = crate::http::shared_client()
            .post(self.create_task_url())
            .bearer_auth(&self.config.api_key)
            .json(&body);
        if let Some(timeout) = self.config.timeout {
            builder = builder.timeout(timeout);
        }
        let response = builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("minimax")
        })?;
        let response = crate::providers::parse_json_response("minimax", response).await?;
        Ok(self.parse_job(response))
    }

    async fn get_video_generation(&self, job_id: &str) -> Result<ProviderVideoJob, AigcError> {
        let mut builder = crate::http::shared_client()
            .get(self.query_task_url(job_id))
            .bearer_auth(&self.config.api_key);
        if let Some(timeout) = self.config.timeout {
            builder = builder.timeout(timeout);
        }
        let response = builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("minimax")
        })?;
        let response = crate::providers::parse_json_response("minimax", response).await?;
        let mut job = self.parse_job(response);
        // Completed 且带 file_id 时,补全 video_url。`video/retrive.md:90-93` 注:
        // download_url 有效期 1 小时,gateway 必须在此期间下载完成。
        if matches!(job.status, ProviderGenerationStatus::Completed) {
            if let Some(file_id) = job
                .metadata
                .get("file_id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                let download_url = self.fetch_download_url(&file_id).await?;
                job.video_url = Some(download_url);
            }
        }
        Ok(job)
    }

    async fn cancel_video_generation(&self, _job_id: &str) -> Result<(), AigcError> {
        // Minimax video API 不暴露取消端点(`docs/external/minimax/video/` 未列)。
        Err(AigcError::new(
            "unsupported_operation",
            "Minimax video API does not expose a task cancel endpoint",
        )
        .provider("minimax"))
    }

    async fn list_video_generations(
        &self,
        _query: &VideoTaskListQuery,
    ) -> Result<(Vec<ProviderVideoJob>, u64), AigcError> {
        // Minimax video API 不暴露列任务端点。
        Err(AigcError::new(
            "unsupported_operation",
            "Minimax video API does not expose a list-tasks endpoint",
        )
        .provider("minimax"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VideoGenerationConfig;
    use bytes::Bytes;
    use serde_json::json;

    fn adapter() -> MinimaxVideoAdapter {
        MinimaxVideoAdapter::from_config(MinimaxVideoConfig {
            model: "MiniMax-Hailuo-02".into(),
            api_key: "test-key".into(),
            api_url: None,
            timeout: None,
        })
        .unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn build_create_request_t2v() {
        let request = VideoGenerationRequest {
            content: vec![VideoContentItem::Text {
                text: "a cat playing piano".into(),
            }],
            generation_config: VideoGenerationConfig {
                resolution: Some("1080P".into()),
                duration_secs: Some(6),
                watermark: true,
                ..Default::default()
            },
            execution_config: Default::default(),
            provider_options: json!({"prompt_optimizer": true}),
        };
        let body = adapter().build_create_request(&request).await.unwrap();
        assert_eq!(body["model"], "MiniMax-Hailuo-02");
        assert_eq!(body["prompt"], "a cat playing piano");
        assert_eq!(body["resolution"], "1080P");
        assert_eq!(body["duration"], 6);
        assert_eq!(body["aigc_watermark"], true);
        assert_eq!(body["prompt_optimizer"], true);
        assert!(body.get("first_frame_image").is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn build_create_request_i2v_with_first_frame() {
        let request = VideoGenerationRequest {
            content: vec![
                VideoContentItem::Text {
                    text: "[左移] slowly".into(),
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
        let body = adapter().build_create_request(&request).await.unwrap();
        assert_eq!(body["first_frame_image"], "https://example.com/frame.png");
        assert_eq!(body["prompt"], "[左移] slowly");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn build_create_request_frame2v_with_last_frame() {
        let request = VideoGenerationRequest {
            content: vec![
                VideoContentItem::Image {
                    asset: AssetRef::Url("https://example.com/start.png".into()),
                    role: VideoImageRole::FirstFrame,
                },
                VideoContentItem::Image {
                    asset: AssetRef::Url("https://example.com/end.png".into()),
                    role: VideoImageRole::LastFrame,
                },
            ],
            generation_config: Default::default(),
            execution_config: Default::default(),
            provider_options: json!({"fast_pretreatment": true}),
        };
        let body = adapter().build_create_request(&request).await.unwrap();
        assert_eq!(body["first_frame_image"], "https://example.com/start.png");
        assert_eq!(body["last_frame_image"], "https://example.com/end.png");
        assert_eq!(body["fast_pretreatment"], true);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn build_create_request_subject_reference() {
        let request = VideoGenerationRequest {
            content: vec![
                VideoContentItem::Text {
                    text: "the person dancing".into(),
                },
                VideoContentItem::Image {
                    asset: AssetRef::Url("https://example.com/face.png".into()),
                    role: VideoImageRole::ReferenceImage,
                },
            ],
            generation_config: Default::default(),
            execution_config: Default::default(),
            provider_options: json!({}),
        };
        let s2v = MinimaxVideoAdapter::from_config(MinimaxVideoConfig {
            model: "S2V-01".into(),
            api_key: "k".into(),
            api_url: None,
            timeout: None,
        })
        .unwrap();
        let body = s2v.build_create_request(&request).await.unwrap();
        assert_eq!(body["model"], "S2V-01");
        assert_eq!(
            body["subject_reference"][0]["image"][0],
            "https://example.com/face.png"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_empty_content() {
        let request = VideoGenerationRequest {
            content: vec![],
            generation_config: Default::default(),
            execution_config: Default::default(),
            provider_options: json!({}),
        };
        let err = adapter().build_create_request(&request).await.unwrap_err();
        assert_eq!(err.code, "missing_input");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_video_audio_drafttask_inputs() {
        for content in [
            VideoContentItem::Video {
                asset: AssetRef::Url("u".into()),
            },
            VideoContentItem::Audio {
                asset: AssetRef::Url("u".into()),
            },
            VideoContentItem::DraftTask { id: "x".into() },
        ] {
            let request = VideoGenerationRequest {
                content: vec![content],
                generation_config: Default::default(),
                execution_config: Default::default(),
                provider_options: json!({}),
            };
            let err = adapter().build_create_request(&request).await.unwrap_err();
            assert_eq!(err.code, "unsupported_input");
        }
    }

    // AssetRef 6-variant materialization (spec §3e).
    #[tokio::test(flavor = "current_thread")]
    async fn asset_ref_url_passes_through() {
        let out = MinimaxVideoAdapter::asset_to_minimax_input(&AssetRef::Url("https://x".into()))
            .await
            .unwrap();
        assert_eq!(out, "https://x");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn asset_ref_data_url_passes_through() {
        let out = MinimaxVideoAdapter::asset_to_minimax_input(&AssetRef::DataUrl(
            "data:image/png;base64,AA".into(),
        ))
        .await
        .unwrap();
        assert_eq!(out, "data:image/png;base64,AA");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn asset_ref_base64_builds_data_uri() {
        let out = MinimaxVideoAdapter::asset_to_minimax_input(&AssetRef::Base64 {
            data: "AAAA".into(),
            mime_type: "image/png".into(),
        })
        .await
        .unwrap();
        assert_eq!(out, "data:image/png;base64,AAAA");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn asset_ref_bytes_encodes_base64() {
        let out = MinimaxVideoAdapter::asset_to_minimax_input(&AssetRef::Bytes {
            bytes: Bytes::from_static(&[1, 2, 3, 4]),
            mime_type: "image/jpeg".into(),
        })
        .await
        .unwrap();
        assert_eq!(out, "data:image/jpeg;base64,AQIDBA==");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn asset_ref_local_path_reads_and_encodes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("img.png");
        tokio::fs::write(&path, [0xFFu8, 0xD8, 0xFF]).await.unwrap();
        let out = MinimaxVideoAdapter::asset_to_minimax_input(&AssetRef::LocalPath(
            path.to_string_lossy().into(),
        ))
        .await
        .unwrap();
        assert_eq!(out, "data:image/png;base64,/9j/");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn asset_ref_stored_returns_unsupported_operation() {
        let err = MinimaxVideoAdapter::asset_to_minimax_input(&AssetRef::Stored {
            asset_id: "abc".into(),
        })
        .await
        .unwrap_err();
        assert_eq!(err.code, "unsupported_input");
        assert!(err.message.contains("resolve_asset_url"));
    }

    // Status mapping (spec §3c).
    #[test]
    fn parse_job_create_response_has_no_status_yet() {
        let job = adapter().parse_job(json!({"task_id": "tk-1"}));
        assert_eq!(job.id, "tk-1");
        assert_eq!(job.status, ProviderGenerationStatus::Queued);
        assert!(job.raw_status.is_none());
    }

    #[test]
    fn parse_job_status_mapping_table() {
        for (raw, expected) in [
            ("Preparing", ProviderGenerationStatus::Queued),
            ("Queueing", ProviderGenerationStatus::Queued),
            ("Processing", ProviderGenerationStatus::Running),
            ("Success", ProviderGenerationStatus::Completed),
            ("Fail", ProviderGenerationStatus::Failed),
        ] {
            let job = adapter().parse_job(json!({"task_id": "t", "status": raw}));
            assert_eq!(job.status, expected, "{raw}");
            assert_eq!(job.raw_status, Some(raw.to_string()));
        }
    }

    #[test]
    fn parse_job_success_extracts_file_id_into_metadata() {
        let job = adapter().parse_job(json!({
            "task_id": "t", "status": "Success", "file_id": "f-1"
        }));
        assert_eq!(job.status, ProviderGenerationStatus::Completed);
        assert_eq!(job.metadata["file_id"], "f-1");
        // video_url 在 parse_job 阶段未填充;由 get_video_generation 调
        // /v1/files/retrieve 后补全。
        assert!(job.video_url.is_none());
    }

    #[test]
    fn parse_job_fail_records_error_message() {
        let job = adapter().parse_job(json!({
            "task_id": "t",
            "status": "Fail",
            "base_resp": {"status_code": 1, "status_msg": "content moderation failed"}
        }));
        assert_eq!(job.status, ProviderGenerationStatus::Failed);
        assert_eq!(job.error, Some("content moderation failed".into()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn create_request_never_emits_callback_url_field() {
        // PRD 非目标:不实现 webhook。即使 provider_options 给了 callback_url,
        // 也必须不出现在请求体里。
        let request = VideoGenerationRequest {
            content: vec![VideoContentItem::Text { text: "x".into() }],
            generation_config: Default::default(),
            execution_config: Default::default(),
            provider_options: json!({"callback_url": "https://attacker/listen"}),
        };
        let body = adapter().build_create_request(&request).await.unwrap();
        assert!(
            body.get("callback_url").is_none(),
            "callback_url must not be forwarded to Minimax (PRD non-goal); body = {body}"
        );
    }
}
