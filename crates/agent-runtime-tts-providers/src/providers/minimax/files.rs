//! Minimax `/v1/files/upload` (multipart) and `/v1/files/retrieve` thin
//! wrapper. Used by issue 005 Voice Clone (`prompt_audio`) and by the
//! async TTS path (`text_file_id` long text input).
//!
//! Reference: `docs/external/minimax/voice_clone/voice-upload.md` for the
//! multipart payload; spec §4e for the call sites.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::error::{TtsError, TtsErrorCode};

/// `purpose` field on `/v1/files/upload`. Maps to the 3 documented Minimax
/// values; we keep them small and explicit so callers can't typo them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilePurpose {
    VoiceClone,
    PromptAudio,
    T2aAsyncInput,
}

impl FilePurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            FilePurpose::VoiceClone => "voice_clone",
            FilePurpose::PromptAudio => "prompt_audio",
            FilePurpose::T2aAsyncInput => "t2a_async_input",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct UploadResponse {
    file: UploadedFile,
}

#[derive(Debug, Clone, Deserialize)]
struct UploadedFile {
    file_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct RetrieveResponse {
    file: RetrievedFile,
}

#[derive(Debug, Clone, Deserialize)]
struct RetrievedFile {
    #[serde(default)]
    download_url: Option<String>,
}

/// Upload bytes to Minimax `/v1/files/upload` as multipart/form-data and
/// return the assigned `file_id`. Errors map to `InvalidRequest`
/// (per spec §4e — NOT `InvalidInput`, which doesn't exist).
#[allow(clippy::too_many_arguments)] // justified: spec §4e enumerates client+base_url+api_key+purpose+bytes+filename+mime explicitly; an UploadConfig struct adds boilerplate without helping callers.
pub async fn upload_file(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    purpose: FilePurpose,
    bytes: Bytes,
    filename: &str,
    mime_type: &str,
) -> Result<u64, TtsError> {
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(filename.to_string())
        .mime_str(mime_type)
        .map_err(|err| {
            TtsError::new(
                TtsErrorCode::InvalidRequest,
                format!("invalid mime type for upload: {err}"),
            )
        })?;
    let form = reqwest::multipart::Form::new()
        .text("purpose", purpose.as_str())
        .part("file", part);

    let url = format!("{}/v1/files/upload", base_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|err| {
            TtsError::new(
                TtsErrorCode::ProviderHttpError,
                format!("Minimax /v1/files/upload request failed: {err}"),
            )
        })?;

    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(TtsError::new(
            TtsErrorCode::ProviderHttpError,
            format!("Minimax /v1/files/upload HTTP {status}: {body_text}"),
        )
        .with_upstream(Some(status.as_u16()), None, None, None));
    }

    let parsed: UploadResponse = serde_json::from_str(&body_text).map_err(|err| {
        TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("Minimax /v1/files/upload bad response: {err}; body: {body_text}"),
        )
    })?;
    Ok(parsed.file.file_id)
}

/// Thin wrapper around `/v1/files/retrieve` — returns the short-lived
/// `download_url` for a previously uploaded or generated file. Per spec
/// §3.3 decision A this helper is duplicated inside the tts crate; aigc
/// crate has an analogous private helper.
pub async fn retrieve_download_url(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    file_id: u64,
) -> Result<String, TtsError> {
    let url = format!(
        "{}/v1/files/retrieve?file_id={}",
        base_url.trim_end_matches('/'),
        file_id
    );
    let response = client
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|err| {
            TtsError::new(
                TtsErrorCode::ProviderHttpError,
                format!("Minimax /v1/files/retrieve request failed: {err}"),
            )
        })?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(TtsError::new(
            TtsErrorCode::ProviderHttpError,
            format!("Minimax /v1/files/retrieve HTTP {status}: {body_text}"),
        )
        .with_upstream(Some(status.as_u16()), None, None, None));
    }
    let parsed: RetrieveResponse = serde_json::from_str(&body_text).map_err(|err| {
        TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("Minimax /v1/files/retrieve bad response: {err}; body: {body_text}"),
        )
    })?;
    parsed.file.download_url.ok_or_else(|| {
        TtsError::new(
            TtsErrorCode::InvalidRequest,
            "Minimax /v1/files/retrieve response missing file.download_url",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_purpose_string_mapping() {
        assert_eq!(FilePurpose::VoiceClone.as_str(), "voice_clone");
        assert_eq!(FilePurpose::PromptAudio.as_str(), "prompt_audio");
        assert_eq!(FilePurpose::T2aAsyncInput.as_str(), "t2a_async_input");
    }

    #[test]
    fn upload_response_parses() {
        let raw = r#"{"file":{"file_id":12345,"bytes":100,"purpose":"voice_clone"},"base_resp":{"status_code":0,"status_msg":"success"}}"#;
        let parsed: UploadResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.file.file_id, 12345);
    }

    #[test]
    fn retrieve_response_parses_download_url() {
        let raw = r#"{"file":{"file_id":1,"download_url":"https://x"},"base_resp":{"status_code":0,"status_msg":""}}"#;
        let parsed: RetrieveResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.file.download_url.as_deref(), Some("https://x"));
    }

    #[test]
    fn retrieve_response_missing_url_parses_as_none() {
        let raw = r#"{"file":{"file_id":1}}"#;
        let parsed: RetrieveResponse = serde_json::from_str(raw).unwrap();
        assert!(parsed.file.download_url.is_none());
    }
}
