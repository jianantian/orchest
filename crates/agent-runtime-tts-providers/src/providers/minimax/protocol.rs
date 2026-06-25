//! Minimax `t2a_v2` WebSocket protocol primitives:
//! - 6 frame types (`task_start` / `task_started` / `task_continue` /
//!   `task_continued` / `task_finish` / `task_finished`)
//! - hex audio decode (frames carry `data.audio` as a hex string; can be `null`)
//! - `base_resp` → [`TtsError`] mapping per spec §4c
//!
//! Reference: `docs/external/minimax/tts_sync.md` (frame schema) and
//! `tts_async.md` §base_resp (error code union).

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{TtsError, TtsErrorCode};
use crate::types::AudioFormat;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BaseResp {
    #[serde(default)]
    pub status_code: i64,
    #[serde(default)]
    pub status_msg: String,
}

impl BaseResp {
    pub(crate) fn is_ok(&self) -> bool {
        self.status_code == 0
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TaskStartFrame<'a> {
    pub event: &'static str,
    pub model: &'a str,
    pub voice_setting: Value,
    pub audio_setting: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_boost: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pronunciation_dict: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TaskContinueFrame<'a> {
    pub event: &'static str,
    pub text: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TaskFinishFrame {
    pub event: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InboundFrame {
    pub event: String,
    #[serde(default)]
    pub data: Option<InboundData>,
    #[serde(default)]
    pub extra_info: Option<Value>,
    pub base_resp: BaseResp,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default)]
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InboundData {
    #[serde(default)]
    pub audio: Option<String>,
    #[serde(default)]
    /// Per-frame status (1=in-progress, 2=final). Currently unread; the
    /// frame-level `event` discriminant + `is_final` flag cover terminal
    /// detection, but this field is part of the protocol and kept for
    /// future telemetry.
    #[allow(dead_code)] // justified: protocol field reserved for telemetry
    pub status: Option<i64>,
}

/// Decode the `data.audio` hex string. `None` or empty returns empty
/// `Bytes` — Minimax sends `data: null` on the initial `task_started` frame
/// and we MUST NOT panic.
pub(crate) fn decode_hex_audio(audio: Option<&str>) -> Result<Bytes, TtsError> {
    let Some(s) = audio else {
        return Ok(Bytes::new());
    };
    if s.is_empty() {
        return Ok(Bytes::new());
    }
    let bytes = hex::decode(s).map_err(|err| {
        TtsError::new(
            TtsErrorCode::InvalidAudio,
            format!("Minimax hex audio decode failed: {err}"),
        )
    })?;
    Ok(Bytes::from(bytes))
}

/// Map a Minimax `base_resp` block to a [`TtsError`] per spec §4c. Returns
/// `None` on success (status_code == 0). `upstream_code` always preserves
/// the raw Minimax code.
pub(crate) fn map_base_resp(resp: &BaseResp) -> Option<TtsError> {
    if resp.is_ok() {
        return None;
    }
    let code = resp.status_code;
    let upstream_code = Some(code.to_string());
    let upstream_message = if resp.status_msg.is_empty() {
        None
    } else {
        Some(resp.status_msg.clone())
    };

    let (tts_code, status) = match code {
        1001 | 2201 => (TtsErrorCode::Timeout, None),
        // Rate-limit family — surface as HTTP 429 + upstream_code preserved.
        // Spec §4c defers a dedicated RateLimited variant.
        1002 | 1039 | 2205 => (TtsErrorCode::ProviderHttpError, Some(429u16)),
        1004 => (TtsErrorCode::InvalidApiKey, None),
        1042 | 2203 | 2204 | 2013 => (TtsErrorCode::InvalidRequest, None),
        2202 => (TtsErrorCode::ProviderStreamError, None),
        // 1000 / unlisted codes fall through to ProviderTaskFailed.
        // This intentionally covers sync-only (1491/1578/1683/1823/1871/2882)
        // and async-only (1200/1573/2251) codes documented in spec §4c.
        _ => (TtsErrorCode::ProviderTaskFailed, None),
    };

    let msg = upstream_message
        .clone()
        .unwrap_or_else(|| format!("Minimax base_resp.status_code = {code}"));

    Some(TtsError::new(tts_code, msg).with_upstream(status, upstream_code, upstream_message, None))
}

pub(crate) fn minimax_audio_format(format: &AudioFormat) -> &'static str {
    match format {
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Pcm16Le => "pcm",
        AudioFormat::WavPcm16Le => "wav",
        AudioFormat::OggOpus => "flac",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hex_audio_handles_none() {
        assert!(decode_hex_audio(None).unwrap().is_empty());
    }

    #[test]
    fn decode_hex_audio_handles_empty_string() {
        assert!(decode_hex_audio(Some("")).unwrap().is_empty());
    }

    #[test]
    fn decode_hex_audio_round_trips() {
        let raw = [0x01u8, 0xab, 0xcd, 0xef];
        let h = hex::encode(raw);
        assert_eq!(decode_hex_audio(Some(&h)).unwrap().as_ref(), raw);
    }

    #[test]
    fn decode_hex_audio_invalid_returns_error() {
        let err = decode_hex_audio(Some("zzz")).unwrap_err();
        assert_eq!(err.code, TtsErrorCode::InvalidAudio);
    }

    #[test]
    fn map_base_resp_zero_is_none() {
        let resp = BaseResp {
            status_code: 0,
            status_msg: "success".into(),
        };
        assert!(map_base_resp(&resp).is_none());
    }

    #[test]
    fn map_base_resp_timeout_codes() {
        for code in [1001i64, 2201] {
            let resp = BaseResp {
                status_code: code,
                status_msg: format!("err {code}"),
            };
            let err = map_base_resp(&resp).expect("error");
            assert_eq!(err.code, TtsErrorCode::Timeout, "code {code}");
            assert_eq!(
                err.upstream_code.as_deref(),
                Some(code.to_string().as_str())
            );
        }
    }

    #[test]
    fn map_base_resp_rate_limit_codes_become_http_429() {
        for code in [1002i64, 1039, 2205] {
            let resp = BaseResp {
                status_code: code,
                status_msg: "rate".into(),
            };
            let err = map_base_resp(&resp).expect("error");
            assert_eq!(err.code, TtsErrorCode::ProviderHttpError);
            assert_eq!(err.status, Some(429));
            assert_eq!(
                err.upstream_code.as_deref(),
                Some(code.to_string().as_str())
            );
        }
    }

    #[test]
    fn map_base_resp_invalid_api_key() {
        let resp = BaseResp {
            status_code: 1004,
            status_msg: "auth".into(),
        };
        assert_eq!(
            map_base_resp(&resp).unwrap().code,
            TtsErrorCode::InvalidApiKey
        );
    }

    #[test]
    fn map_base_resp_invalid_request_codes() {
        for code in [1042i64, 2203, 2204, 2013] {
            let resp = BaseResp {
                status_code: code,
                status_msg: "bad".into(),
            };
            assert_eq!(
                map_base_resp(&resp).unwrap().code,
                TtsErrorCode::InvalidRequest,
                "code {code}"
            );
        }
    }

    #[test]
    fn map_base_resp_stream_error() {
        let resp = BaseResp {
            status_code: 2202,
            status_msg: "stream".into(),
        };
        assert_eq!(
            map_base_resp(&resp).unwrap().code,
            TtsErrorCode::ProviderStreamError
        );
    }

    #[test]
    fn map_base_resp_other_codes_fallback_to_task_failed() {
        for code in [
            1000i64, 1491, 1578, 1683, 1823, 1871, 2882, 1200, 1573, 2251, 9999,
        ] {
            let resp = BaseResp {
                status_code: code,
                status_msg: format!("other {code}"),
            };
            let err = map_base_resp(&resp).expect("error");
            assert_eq!(err.code, TtsErrorCode::ProviderTaskFailed, "code {code}");
            assert_eq!(
                err.upstream_code.as_deref(),
                Some(code.to_string().as_str())
            );
        }
    }

    #[test]
    fn inbound_frame_parses_null_data_without_panic() {
        // task_started frame has `data: null`.
        let raw = r#"{"event":"task_started","data":null,"base_resp":{"status_code":0,"status_msg":"success"}}"#;
        let frame: InboundFrame = serde_json::from_str(raw).unwrap();
        assert_eq!(frame.event, "task_started");
        assert!(frame.data.is_none());
    }

    #[test]
    fn inbound_frame_parses_continued_with_hex_audio() {
        let raw = r#"{"event":"task_continued","data":{"audio":"01ab","status":1},"base_resp":{"status_code":0,"status_msg":"success"}}"#;
        let frame: InboundFrame = serde_json::from_str(raw).unwrap();
        let data = frame.data.expect("continued has data");
        let bytes = decode_hex_audio(data.audio.as_deref()).unwrap();
        assert_eq!(bytes.as_ref(), &[0x01, 0xab]);
    }

    #[test]
    fn audio_format_mapping() {
        assert_eq!(minimax_audio_format(&AudioFormat::Mp3), "mp3");
        assert_eq!(minimax_audio_format(&AudioFormat::Pcm16Le), "pcm");
        assert_eq!(minimax_audio_format(&AudioFormat::WavPcm16Le), "wav");
    }
}
