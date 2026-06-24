//! Minimax async HTTP TTS: `POST /v1/t2a_async_v2`.
//!
//! The async endpoint returns a `task_id` / `task_token` / `file_id` triple.
//! `file_id` is the rendered audio's handle in Minimax's file store; the
//! adapter then surfaces it as `AudioData::Url` (callers may pre-download via
//! [`super::files::retrieve_download_url`] or hand the URL through to users
//! \u2014 spec §4d says "是否预下载留给上层").
//!
//! Reference: `docs/external/minimax/tts_async.md`.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{TtsError, TtsErrorCode};
use crate::types::{AudioFormat, SpeechControls, VoiceSelection};

use super::protocol::{map_base_resp, minimax_audio_format, BaseResp};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AsyncResponse {
    #[serde(default)]
    pub task_id: Option<u64>,
    #[serde(default)]
    pub task_token: Option<String>,
    #[serde(default)]
    pub file_id: Option<u64>,
    pub base_resp: BaseResp,
}

/// Build the JSON body for `POST /v1/t2a_async_v2`. `text_or_file_id` is an
/// `Either<&str, u64>` shaped as a 2-tuple: either inline text or a
/// previously uploaded `text_file_id` (spec §4d long-text path).
#[allow(clippy::too_many_arguments)] // justified: spec §4d enumerates all 7 fields explicitly; aggregating into a struct adds noise without informing the call sites.
pub(crate) fn build_async_request(
    model: &str,
    voice: &VoiceSelection,
    output_format: &AudioFormat,
    output_sample_rate: Option<u32>,
    controls: &SpeechControls,
    text: Option<&str>,
    text_file_id: Option<u64>,
) -> Value {
    let mut voice_setting = json!({ "voice_id": voice.id });
    if controls.speed != 1.0 {
        voice_setting["speed"] = json!(controls.speed);
    }
    if controls.pitch != 0.0 {
        voice_setting["pitch"] = json!(controls.pitch as i32);
    }
    if controls.volume != 1.0 {
        voice_setting["vol"] = json!(controls.volume);
    }
    let mut audio_setting = json!({ "format": minimax_audio_format(output_format) });
    if let Some(sr) = output_sample_rate {
        audio_setting["sample_rate"] = json!(sr);
    }
    let mut body = json!({
        "model": model,
        "voice_setting": voice_setting,
        "audio_setting": audio_setting,
    });
    if let Some(t) = text {
        body["text"] = json!(t);
    }
    if let Some(fid) = text_file_id {
        body["text_file_id"] = json!(fid);
    }
    body
}

/// Parse a `POST /v1/t2a_async_v2` JSON response, mapping `base_resp` errors
/// per spec §4c.
pub(crate) fn parse_async_response(body: &str) -> Result<AsyncResponse, TtsError> {
    let parsed: AsyncResponse = serde_json::from_str(body).map_err(|err| {
        TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("Minimax /v1/t2a_async_v2 response parse failed: {err}; body: {body}"),
        )
    })?;
    if let Some(err) = map_base_resp(&parsed.base_resp) {
        return Err(err);
    }
    Ok(parsed)
}

/// Live POST to `/v1/t2a_async_v2`. Unit tests stub HTTP — only live tests
/// exercise this path against the real Minimax server.
pub(crate) async fn run_async_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    body: Value,
) -> Result<AsyncResponse, TtsError> {
    let url = format!("{}/v1/t2a_async_v2", base_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|err| {
            TtsError::new(
                TtsErrorCode::ProviderHttpError,
                format!("Minimax /v1/t2a_async_v2 request failed: {err}"),
            )
        })?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(TtsError::new(
            TtsErrorCode::ProviderHttpError,
            format!("Minimax /v1/t2a_async_v2 HTTP {status}: {body_text}"),
        )
        .with_upstream(Some(status.as_u16()), None, None, None));
    }
    parse_async_response(&body_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn async_request_body_inline_text() {
        let body = build_async_request(
            "speech-2.8-hd",
            &VoiceSelection::by_id("voice-1"),
            &AudioFormat::Mp3,
            None,
            &SpeechControls::default(),
            Some("hello"),
            None,
        );
        assert_eq!(body["model"], "speech-2.8-hd");
        assert_eq!(body["text"], "hello");
        assert!(body.get("text_file_id").is_none());
        assert_eq!(body["voice_setting"]["voice_id"], "voice-1");
        assert_eq!(body["audio_setting"]["format"], "mp3");
    }

    #[test]
    fn async_request_body_text_file_id_path() {
        let body = build_async_request(
            "speech-2.8-hd",
            &VoiceSelection::by_id("v"),
            &AudioFormat::Mp3,
            Some(24000),
            &SpeechControls::default(),
            None,
            Some(12345),
        );
        assert_eq!(body["text_file_id"], 12345);
        assert!(body.get("text").is_none());
        assert_eq!(body["audio_setting"]["sample_rate"], 24000);
    }

    #[test]
    fn parse_async_response_extracts_ids() {
        let raw = r#"{"task_id":111,"task_token":"tk-1","file_id":222,"base_resp":{"status_code":0,"status_msg":"success"}}"#;
        let parsed = parse_async_response(raw).unwrap();
        assert_eq!(parsed.task_id, Some(111));
        assert_eq!(parsed.task_token.as_deref(), Some("tk-1"));
        assert_eq!(parsed.file_id, Some(222));
    }

    #[test]
    fn parse_async_response_propagates_base_resp_error() {
        let raw = r#"{"base_resp":{"status_code":1004,"status_msg":"auth"}}"#;
        let err = parse_async_response(raw).unwrap_err();
        assert_eq!(err.code, TtsErrorCode::InvalidApiKey);
        assert_eq!(err.upstream_code.as_deref(), Some("1004"));
    }

    #[test]
    fn parse_async_response_invalid_json_returns_invalid_request() {
        let err = parse_async_response("not-json").unwrap_err();
        assert_eq!(err.code, TtsErrorCode::InvalidRequest);
    }
}
