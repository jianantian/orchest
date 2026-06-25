//! `MinimaxMusicAdapter` implementing [`super::MusicProvider`]. Endpoints:
//! `POST /v1/music_generation`, `POST /v1/lyrics_generation`,
//! `POST /v1/music_cover_preprocess`. Auth: `Authorization: Bearer`. All JSON.

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AigcError;

use super::{
    CoverAudioSource, CoverPreprocessRequest, CoverPreprocessResult, GenerateLyricsRequest,
    GenerateLyricsResult, GenerateMusicRequest, GenerateMusicResult, MusicAudio, MusicOutputFormat,
    MusicProvider, MusicStream,
};

const DEFAULT_API_URL: &str = "https://api.minimaxi.com";

#[derive(Debug, Clone)]
pub struct MinimaxMusicConfig {
    pub model: String,
    pub api_key: String,
    pub api_url: Option<String>,
    pub timeout: Option<Duration>,
}

pub struct MinimaxMusicAdapter {
    config: MinimaxMusicConfig,
    api_base_url: String,
}

impl std::fmt::Debug for MinimaxMusicAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MinimaxMusicAdapter")
            .field("model", &self.config.model)
            .field("api_base_url", &self.api_base_url)
            .finish()
    }
}

impl MinimaxMusicAdapter {
    pub fn from_config(config: MinimaxMusicConfig) -> Result<Self, AigcError> {
        if config.model.trim().is_empty() {
            return Err(AigcError::new("invalid_model", "model cannot be empty"));
        }
        if config.api_key.trim().is_empty() {
            return Err(AigcError::new("invalid_api_key", "API key cannot be empty"));
        }
        let api_base_url = config
            .api_url
            .clone()
            .unwrap_or_else(|| DEFAULT_API_URL.into())
            .trim_end_matches('/')
            .to_string();
        Ok(Self {
            config,
            api_base_url,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base_url, path)
    }

    async fn post_json(&self, path: &str, body: &Value) -> Result<String, AigcError> {
        let mut builder = crate::http::shared_client()
            .post(self.url(path))
            .bearer_auth(&self.config.api_key)
            .json(body);
        if let Some(t) = self.config.timeout {
            builder = builder.timeout(t);
        }
        let response = builder.send().await.map_err(|err| {
            AigcError::new("provider_request_failed", err.to_string()).provider("minimax")
        })?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AigcError {
                code: "provider_http_error".into(),
                message: format!("Minimax {path} HTTP {status}: {text}"),
                provider: Some("minimax".into()),
                status: Some(status.as_u16()),
                upstream_code: None,
                upstream_message: None,
                upstream_body: serde_json::from_str(&text).ok(),
            });
        }
        Ok(text)
    }
}

pub(crate) fn build_generate_request_body(req: &GenerateMusicRequest, stream: bool) -> Value {
    let mut body = json!({ "model": req.model });
    if let Some(p) = &req.prompt {
        body["prompt"] = json!(p);
    }
    if let Some(l) = &req.lyrics {
        body["lyrics"] = json!(l);
    }
    body["stream"] = json!(stream);
    // Stream mode forces output_format=hex (generation.md:116-124).
    let output_format = if stream {
        MusicOutputFormat::Hex
    } else {
        req.output_format
    };
    body["output_format"] = json!(match output_format {
        MusicOutputFormat::Url => "url",
        MusicOutputFormat::Hex => "hex",
    });
    if let Some(setting) = &req.audio_setting {
        let mut audio_setting = json!({});
        if let Some(sr) = setting.sample_rate {
            audio_setting["sample_rate"] = json!(sr);
        }
        if let Some(br) = setting.bitrate {
            audio_setting["bitrate"] = json!(br);
        }
        if let Some(fmt) = &setting.format {
            audio_setting["format"] = json!(fmt);
        }
        body["audio_setting"] = audio_setting;
    }
    if req.aigc_watermark {
        body["aigc_watermark"] = json!(true);
    }
    if req.lyrics_optimizer {
        body["lyrics_optimizer"] = json!(true);
    }
    if req.is_instrumental {
        body["is_instrumental"] = json!(true);
    }
    if let Some(cover) = &req.cover_audio {
        match cover {
            CoverAudioSource::Url { url } => body["audio_url"] = json!(url),
            CoverAudioSource::Base64 { data } => body["audio_base64"] = json!(data),
            CoverAudioSource::FeatureId { id } => body["cover_feature_id"] = json!(id),
        }
    }
    body
}

pub(crate) fn build_lyrics_request_body(req: &GenerateLyricsRequest) -> Value {
    let mut body = json!({ "mode": req.mode.as_str() });
    if let Some(p) = &req.prompt {
        body["prompt"] = json!(p);
    }
    if let Some(l) = &req.lyrics {
        body["lyrics"] = json!(l);
    }
    if let Some(t) = &req.title {
        body["title"] = json!(t);
    }
    body
}

pub(crate) fn build_cover_request_body(req: &CoverPreprocessRequest) -> Value {
    let mut body = json!({ "model": req.model });
    match &req.audio_source {
        CoverAudioSource::Url { url } => body["audio_url"] = json!(url),
        CoverAudioSource::Base64 { data } => body["audio_base64"] = json!(data),
        CoverAudioSource::FeatureId { .. } => {
            body["audio_url"] = json!("");
        }
    }
    body
}

#[derive(Debug, Deserialize)]
struct BaseResp {
    #[serde(default)]
    status_code: i64,
    #[serde(default)]
    status_msg: String,
}

fn check_base_resp(resp: &BaseResp, op: &str) -> Result<(), AigcError> {
    if resp.status_code == 0 {
        return Ok(());
    }
    let msg = if resp.status_msg.is_empty() {
        format!("Minimax {op} status_code={}", resp.status_code)
    } else {
        resp.status_msg.clone()
    };
    Err(AigcError {
        code: "provider_task_failed".into(),
        message: msg,
        provider: Some("minimax".into()),
        status: None,
        upstream_code: Some(resp.status_code.to_string()),
        upstream_message: None,
        upstream_body: None,
    })
}

#[derive(Debug, Deserialize)]
struct GenerateResponseRaw {
    #[serde(default)]
    data: Option<GenerateDataRaw>,
    #[serde(default)]
    trace_id: Option<String>,
    #[serde(default)]
    extra_info: Option<Value>,
    base_resp: BaseResp,
}

#[derive(Debug, Deserialize)]
struct GenerateDataRaw {
    #[serde(default)]
    audio: Option<String>,
}

pub(crate) fn parse_generate_response(
    body: &str,
    requested_format: MusicOutputFormat,
) -> Result<GenerateMusicResult, AigcError> {
    let raw: GenerateResponseRaw = serde_json::from_str(body).map_err(|err| {
        AigcError::new(
            "provider_response_parse_failed",
            format!("Minimax /v1/music_generation response parse failed: {err}; body: {body}"),
        )
        .provider("minimax")
    })?;
    check_base_resp(&raw.base_resp, "/v1/music_generation")?;
    let extra_info = raw.extra_info.unwrap_or(Value::Null);
    let audio_str = raw
        .data
        .and_then(|d| d.audio)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AigcError::new(
                "provider_task_failed",
                "Minimax /v1/music_generation response missing data.audio",
            )
            .provider("minimax")
        })?;
    let audio = match requested_format {
        MusicOutputFormat::Url => MusicAudio::Url(audio_str),
        MusicOutputFormat::Hex => {
            let bytes = hex::decode(&audio_str).map_err(|err| {
                AigcError::new(
                    "invalid_audio",
                    format!("Minimax music hex decode failed: {err}"),
                )
                .provider("minimax")
            })?;
            MusicAudio::Bytes(Bytes::from(bytes))
        }
    };
    Ok(GenerateMusicResult {
        audio,
        extra_info,
        trace_id: raw.trace_id,
    })
}

#[derive(Debug, Deserialize)]
struct LyricsResponseRaw {
    #[serde(default)]
    song_title: Option<String>,
    #[serde(default)]
    style_tags: Option<String>,
    #[serde(default)]
    lyrics: Option<String>,
    #[serde(default)]
    trace_id: Option<String>,
    base_resp: BaseResp,
}

pub(crate) fn parse_lyrics_response(body: &str) -> Result<GenerateLyricsResult, AigcError> {
    let raw: LyricsResponseRaw = serde_json::from_str(body).map_err(|err| {
        AigcError::new(
            "provider_response_parse_failed",
            format!("Minimax /v1/lyrics_generation response parse failed: {err}; body: {body}"),
        )
        .provider("minimax")
    })?;
    check_base_resp(&raw.base_resp, "/v1/lyrics_generation")?;
    Ok(GenerateLyricsResult {
        song_title: raw.song_title.unwrap_or_default(),
        style_tags: raw.style_tags.unwrap_or_default(),
        lyrics: raw.lyrics.unwrap_or_default(),
        trace_id: raw.trace_id,
    })
}

#[derive(Debug, Deserialize)]
struct CoverResponseRaw {
    #[serde(default)]
    cover_feature_id: Option<String>,
    #[serde(default)]
    formatted_lyrics: Option<String>,
    #[serde(default)]
    structure_result: Option<String>,
    #[serde(default)]
    audio_duration: Option<f64>,
    #[serde(default)]
    trace_id: Option<String>,
    base_resp: BaseResp,
}

pub(crate) fn parse_cover_response(body: &str) -> Result<CoverPreprocessResult, AigcError> {
    let raw: CoverResponseRaw = serde_json::from_str(body).map_err(|err| {
        AigcError::new(
            "provider_response_parse_failed",
            format!(
                "Minimax /v1/music_cover_preprocess response parse failed: {err}; body: {body}"
            ),
        )
        .provider("minimax")
    })?;
    check_base_resp(&raw.base_resp, "/v1/music_cover_preprocess")?;
    Ok(CoverPreprocessResult {
        cover_feature_id: raw.cover_feature_id.unwrap_or_default(),
        formatted_lyrics: raw.formatted_lyrics.unwrap_or_default(),
        structure_result: raw.structure_result.unwrap_or_default(),
        audio_duration: raw.audio_duration.unwrap_or(0.0),
        trace_id: raw.trace_id,
    })
}

#[async_trait]
impl MusicProvider for MinimaxMusicAdapter {
    fn provider_name(&self) -> &str {
        "minimax"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    async fn generate(&self, req: GenerateMusicRequest) -> Result<GenerateMusicResult, AigcError> {
        let body = build_generate_request_body(&req, false);
        let resp = self.post_json("/v1/music_generation", &body).await?;
        parse_generate_response(&resp, req.output_format)
    }

    async fn stream_generate(&self, _req: GenerateMusicRequest) -> Result<MusicStream, AigcError> {
        // `build_generate_request_body(_, true)` is unit-tested to force
        // `output_format=hex` when stream=true (spec §6c). The real SSE
        // chunk loop that maps each `data: {...}` event to a `MusicChunk`
        // is not implemented this iteration; returning an empty receiver
        // would silently hide the gap from callers. Live verification will
        // drive this path in a follow-up PR.
        Err(AigcError::new(
            "unsupported_operation",
            "Minimax music streaming is not implemented yet; use generate() with output_format=Url or Hex",
        )
        .provider("minimax"))
    }

    async fn generate_lyrics(
        &self,
        req: GenerateLyricsRequest,
    ) -> Result<GenerateLyricsResult, AigcError> {
        let body = build_lyrics_request_body(&req);
        let resp = self.post_json("/v1/lyrics_generation", &body).await?;
        parse_lyrics_response(&resp)
    }

    async fn preprocess_cover(
        &self,
        req: CoverPreprocessRequest,
    ) -> Result<CoverPreprocessResult, AigcError> {
        let body = build_cover_request_body(&req);
        let resp = self.post_json("/v1/music_cover_preprocess", &body).await?;
        parse_cover_response(&resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::{
        CoverPreprocessRequest, GenerateLyricsRequest, GenerateMusicRequest, LyricsMode,
        MusicAudioSetting,
    };

    fn base_generate_req(model: &str) -> GenerateMusicRequest {
        GenerateMusicRequest {
            model: model.into(),
            prompt: Some("独立民谣".into()),
            lyrics: Some("[verse] 街灯".into()),
            output_format: MusicOutputFormat::Hex,
            audio_setting: None,
            aigc_watermark: false,
            lyrics_optimizer: false,
            is_instrumental: false,
            cover_audio: None,
        }
    }

    #[test]
    fn generate_request_url_format() {
        let req = GenerateMusicRequest {
            output_format: MusicOutputFormat::Url,
            audio_setting: Some(MusicAudioSetting {
                sample_rate: Some(44100),
                bitrate: Some(256000),
                format: Some("mp3".into()),
            }),
            aigc_watermark: true,
            ..base_generate_req("music-2.6")
        };
        let body = build_generate_request_body(&req, false);
        assert_eq!(body["model"], "music-2.6");
        assert_eq!(body["prompt"], "独立民谣");
        assert_eq!(body["output_format"], "url");
        assert_eq!(body["stream"], false);
        assert_eq!(body["audio_setting"]["sample_rate"], 44100);
        assert_eq!(body["audio_setting"]["format"], "mp3");
        assert_eq!(body["aigc_watermark"], true);
    }

    #[test]
    fn stream_forces_hex_format() {
        let req = GenerateMusicRequest {
            output_format: MusicOutputFormat::Url,
            ..base_generate_req("music-2.6")
        };
        let body = build_generate_request_body(&req, true);
        assert_eq!(body["stream"], true);
        // Streaming MUST force hex even if caller requested url (docs:116-124).
        assert_eq!(body["output_format"], "hex");
    }

    #[test]
    fn generate_request_music_cover_with_url() {
        let req = GenerateMusicRequest {
            cover_audio: Some(CoverAudioSource::Url {
                url: "https://example/ref.mp3".into(),
            }),
            ..base_generate_req("music-cover")
        };
        let body = build_generate_request_body(&req, false);
        assert_eq!(body["model"], "music-cover");
        assert_eq!(body["audio_url"], "https://example/ref.mp3");
        assert!(body.get("audio_base64").is_none());
        assert!(body.get("cover_feature_id").is_none());
    }

    #[test]
    fn generate_request_music_cover_with_feature_id() {
        let req = GenerateMusicRequest {
            cover_audio: Some(CoverAudioSource::FeatureId {
                id: "feat-1".into(),
            }),
            ..base_generate_req("music-cover")
        };
        let body = build_generate_request_body(&req, false);
        assert_eq!(body["cover_feature_id"], "feat-1");
    }

    #[test]
    fn generate_request_music_2_6_instrumental_and_optimizer() {
        let req = GenerateMusicRequest {
            lyrics_optimizer: true,
            is_instrumental: true,
            lyrics: None,
            ..base_generate_req("music-2.6")
        };
        let body = build_generate_request_body(&req, false);
        assert_eq!(body["lyrics_optimizer"], true);
        assert_eq!(body["is_instrumental"], true);
        assert!(body.get("lyrics").is_none());
    }

    #[test]
    fn parse_generate_response_hex_decodes_bytes() {
        let body = r#"{
            "data": {"audio": "01ab", "status": 2},
            "trace_id": "t-1",
            "extra_info": {"music_duration": 25364},
            "base_resp": {"status_code": 0, "status_msg": "success"}
        }"#;
        let resp = parse_generate_response(body, MusicOutputFormat::Hex).unwrap();
        assert_eq!(resp.trace_id.as_deref(), Some("t-1"));
        match resp.audio {
            MusicAudio::Bytes(b) => assert_eq!(b.as_ref(), &[0x01, 0xab]),
            MusicAudio::Url(_) => panic!("expected Bytes"),
        }
        assert_eq!(resp.extra_info["music_duration"], 25364);
    }

    #[test]
    fn parse_generate_response_url_returns_url() {
        let body = r#"{
            "data": {"audio": "https://example/music.mp3", "status": 2},
            "base_resp": {"status_code": 0, "status_msg": "success"}
        }"#;
        let resp = parse_generate_response(body, MusicOutputFormat::Url).unwrap();
        match resp.audio {
            MusicAudio::Url(u) => assert_eq!(u, "https://example/music.mp3"),
            MusicAudio::Bytes(_) => panic!("expected Url"),
        }
    }

    #[test]
    fn parse_generate_response_propagates_base_resp_error() {
        let body = r#"{"base_resp":{"status_code":1004,"status_msg":"auth"}}"#;
        let err = parse_generate_response(body, MusicOutputFormat::Hex).unwrap_err();
        assert_eq!(err.code, "provider_task_failed");
        assert_eq!(err.upstream_code.as_deref(), Some("1004"));
    }

    #[test]
    fn parse_generate_response_missing_audio_returns_error() {
        let body =
            r#"{"data": {"status": 1},"base_resp":{"status_code":0,"status_msg":"success"}}"#;
        let err = parse_generate_response(body, MusicOutputFormat::Hex).unwrap_err();
        assert!(err.message.contains("data.audio"));
    }

    #[test]
    fn lyrics_request_write_full_song() {
        let req = GenerateLyricsRequest {
            mode: LyricsMode::WriteFullSong,
            prompt: Some("夏日".into()),
            lyrics: None,
            title: None,
        };
        let body = build_lyrics_request_body(&req);
        assert_eq!(body["mode"], "write_full_song");
        assert_eq!(body["prompt"], "夏日");
        assert!(body.get("lyrics").is_none());
    }

    #[test]
    fn lyrics_request_edit_mode() {
        let req = GenerateLyricsRequest {
            mode: LyricsMode::Edit,
            prompt: Some("续写".into()),
            lyrics: Some("[verse] 已有".into()),
            title: Some("夏日海风".into()),
        };
        let body = build_lyrics_request_body(&req);
        assert_eq!(body["mode"], "edit");
        assert_eq!(body["lyrics"], "[verse] 已有");
        assert_eq!(body["title"], "夏日海风");
    }

    #[test]
    fn parse_lyrics_response_extracts_fields() {
        let body = r#"{
            "song_title": "夏日海风",
            "style_tags": "Pop, Upbeat",
            "lyrics": "[Intro] La la la",
            "trace_id": "t-2",
            "base_resp": {"status_code": 0, "status_msg": "success"}
        }"#;
        let resp = parse_lyrics_response(body).unwrap();
        assert_eq!(resp.song_title, "夏日海风");
        assert_eq!(resp.style_tags, "Pop, Upbeat");
        assert!(resp.lyrics.contains("[Intro]"));
        assert_eq!(resp.trace_id.as_deref(), Some("t-2"));
    }

    #[test]
    fn cover_request_with_audio_url() {
        let req = CoverPreprocessRequest {
            model: "music-cover".into(),
            audio_source: CoverAudioSource::Url {
                url: "https://example/ref.mp3".into(),
            },
        };
        let body = build_cover_request_body(&req);
        assert_eq!(body["model"], "music-cover");
        assert_eq!(body["audio_url"], "https://example/ref.mp3");
    }

    #[test]
    fn parse_cover_response_extracts_feature_id() {
        let body = r#"{
            "cover_feature_id": "feat-abc-123",
            "formatted_lyrics": "[Verse] hi",
            "structure_result": "{\"sections\":[]}",
            "audio_duration": 123.5,
            "trace_id": "t-3",
            "base_resp": {"status_code": 0, "status_msg": "success"}
        }"#;
        let resp = parse_cover_response(body).unwrap();
        assert_eq!(resp.cover_feature_id, "feat-abc-123");
        assert!(resp.formatted_lyrics.contains("[Verse]"));
        assert!(resp.structure_result.contains("sections"));
        assert!((resp.audio_duration - 123.5).abs() < 1e-3);
    }
}
