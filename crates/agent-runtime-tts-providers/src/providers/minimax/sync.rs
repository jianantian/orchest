//! Minimax synchronous WSS TTS client primitives.
//!
//! The 6-frame session (task_start → task_started → task_continue → task_continued
//! → task_finish → task_finished) is split into testable pure functions plus a
//! thin live-WSS runner. Unit tests drive the pure functions with static frame
//! fixtures; live testing exercises [`run_sync_session`].

use bytes::{Bytes, BytesMut};
use serde_json::{json, Value};

use crate::error::{TtsError, TtsErrorCode};
use crate::types::{AudioFormat, SpeechControls, VoiceSelection};

use super::protocol::{
    decode_hex_audio, map_base_resp, minimax_audio_format, InboundFrame, TaskContinueFrame,
    TaskFinishFrame, TaskStartFrame,
};

#[derive(Debug, Clone)]
pub(crate) struct SyncOutcome {
    pub audio: Bytes,
    /// Minimax server-side trace id from the `trace_id` field of any frame.
    /// Currently not surfaced through `SynthesizeResult` — telemetry is
    /// driven by the caller's `request.trace_id`. Kept here so a future
    /// PR can correlate Minimax-side logs without re-plumbing the frame loop.
    #[allow(dead_code)] // justified: held for future telemetry correlation
    pub trace_id: Option<String>,
    pub extra_info: Option<Value>,
}

pub(crate) fn build_task_start_body(
    model: &str,
    voice: &VoiceSelection,
    output_format: &AudioFormat,
    output_sample_rate: Option<u32>,
    controls: &SpeechControls,
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
    if let Some(emotion) = &controls.emotion {
        voice_setting["emotion"] = json!(emotion);
    }
    let mut audio_setting = json!({ "format": minimax_audio_format(output_format) });
    if let Some(sr) = output_sample_rate {
        audio_setting["sample_rate"] = json!(sr);
    }
    let frame = TaskStartFrame {
        event: "task_start",
        model,
        voice_setting,
        audio_setting,
        language_boost: None,
        pronunciation_dict: None,
    };
    serde_json::to_value(frame).expect("static frame serializes")
}

pub(crate) fn build_task_continue_body(text: &str) -> Value {
    serde_json::to_value(TaskContinueFrame {
        event: "task_continue",
        text,
    })
    .expect("static frame serializes")
}

pub(crate) fn build_task_finish_body() -> Value {
    serde_json::to_value(TaskFinishFrame {
        event: "task_finish",
    })
    .expect("static frame serializes")
}

/// Parse a sequence of inbound JSON frames into a single audio buffer.
/// Stops on `task_finished` or `is_final=true`. Returns the first non-zero
/// `base_resp` mapped via [`map_base_resp`].
pub(crate) fn aggregate_frames<I: IntoIterator<Item = String>>(
    frames: I,
) -> Result<SyncOutcome, TtsError> {
    let mut buf = BytesMut::new();
    let mut trace_id: Option<String> = None;
    let mut extra_info: Option<Value> = None;
    for raw in frames {
        let frame: InboundFrame = serde_json::from_str(&raw).map_err(|err| {
            TtsError::new(
                TtsErrorCode::ProviderStreamError,
                format!("Minimax inbound frame parse failed: {err}; raw: {raw}"),
            )
        })?;
        if let Some(err) = map_base_resp(&frame.base_resp) {
            return Err(err);
        }
        if trace_id.is_none() {
            trace_id = frame.trace_id.clone();
        }
        if let Some(extra) = &frame.extra_info {
            extra_info = Some(extra.clone());
        }
        let audio_hex = frame.data.as_ref().and_then(|d| d.audio.as_deref());
        let chunk = decode_hex_audio(audio_hex)?;
        if !chunk.is_empty() {
            buf.extend_from_slice(&chunk);
        }
        if frame.event == "task_finished" || frame.is_final {
            break;
        }
    }
    Ok(SyncOutcome {
        audio: buf.freeze(),
        trace_id,
        extra_info,
    })
}

/// Split into per-frame `AudioChunk` payloads — for `stream_synthesize`.
#[cfg(test)]
pub(crate) fn split_chunks<I: IntoIterator<Item = String>>(
    frames: I,
) -> Result<Vec<Bytes>, TtsError> {
    let mut chunks = Vec::new();
    for raw in frames {
        let frame: InboundFrame = serde_json::from_str(&raw).map_err(|err| {
            TtsError::new(
                TtsErrorCode::ProviderStreamError,
                format!("frame parse failed: {err}"),
            )
        })?;
        if let Some(err) = map_base_resp(&frame.base_resp) {
            return Err(err);
        }
        let audio_hex = frame.data.as_ref().and_then(|d| d.audio.as_deref());
        let chunk = decode_hex_audio(audio_hex)?;
        if !chunk.is_empty() {
            chunks.push(chunk);
        }
        if frame.event == "task_finished" || frame.is_final {
            break;
        }
    }
    Ok(chunks)
}

/// Live WSS session — exercised by live tests only. Connects, drives the
/// 6-frame protocol once, returns aggregated audio.
pub(crate) async fn run_sync_session(
    ws_url: &str,
    api_key: &str,
    start_body: Value,
    text: &str,
) -> Result<SyncOutcome, TtsError> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{self, Message};

    let host = tungstenite::http::Uri::try_from(ws_url)
        .ok()
        .and_then(|u| u.authority().map(|a| a.as_str().to_string()))
        .unwrap_or_default();
    let ws_request = tungstenite::http::Request::builder()
        .uri(ws_url)
        .header("authorization", format!("Bearer {api_key}"))
        .header("host", host)
        .header("upgrade", "websocket")
        .header("connection", "upgrade")
        .header(
            "sec-websocket-key",
            tungstenite::handshake::client::generate_key(),
        )
        .header("sec-websocket-version", "13")
        .body(())
        .map_err(|err| {
            TtsError::new(
                TtsErrorCode::InvalidRequest,
                format!("invalid Minimax WSS URL: {err}"),
            )
        })?;
    let (mut socket, _) = tokio_tungstenite::connect_async(ws_request)
        .await
        .map_err(|err| {
            TtsError::new(
                TtsErrorCode::ProviderHttpError,
                format!("Minimax WSS connect failed: {err}"),
            )
        })?;

    for body in [
        start_body,
        build_task_continue_body(text),
        build_task_finish_body(),
    ] {
        socket
            .send(Message::Text(body.to_string()))
            .await
            .map_err(|err| {
                TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    format!("Minimax WSS send failed: {err}"),
                )
            })?;
    }

    let mut raw_frames: Vec<String> = Vec::new();
    while let Some(msg) = socket.next().await {
        match msg {
            Ok(Message::Text(t)) => {
                let is_terminal =
                    t.contains("\"task_finished\"") || t.contains("\"is_final\":true");
                raw_frames.push(t);
                if is_terminal {
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(err) => {
                return Err(TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    format!("Minimax WSS read failed: {err}"),
                ));
            }
        }
    }
    aggregate_frames(raw_frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(event: &str, audio_hex: Option<&str>, status_code: i64, is_final: bool) -> String {
        let data = match audio_hex {
            Some(h) => json!({"audio": h, "status": 1}),
            None => Value::Null,
        };
        json!({
            "event": event,
            "data": data,
            "base_resp": {
                "status_code": status_code,
                "status_msg": if status_code == 0 { "success" } else { "err" }
            },
            "is_final": is_final,
        })
        .to_string()
    }

    #[test]
    fn task_start_body_minimal() {
        let v = build_task_start_body(
            "speech-2.8-hd",
            &VoiceSelection::by_id("xiaoming"),
            &AudioFormat::Mp3,
            None,
            &SpeechControls::default(),
        );
        assert_eq!(v["event"], "task_start");
        assert_eq!(v["model"], "speech-2.8-hd");
        assert_eq!(v["voice_setting"]["voice_id"], "xiaoming");
        assert_eq!(v["audio_setting"]["format"], "mp3");
        assert!(v["voice_setting"].get("speed").is_none());
    }

    #[test]
    fn task_start_body_passes_non_default_controls() {
        let controls = SpeechControls {
            speed: 1.5,
            pitch: 3.0,
            volume: 0.8,
            instruction: None,
            emotion: Some("happy".into()),
            style: None,
        };
        let v = build_task_start_body(
            "speech-2.8-hd",
            &VoiceSelection::by_id("v"),
            &AudioFormat::Pcm16Le,
            Some(16000),
            &controls,
        );
        assert!((v["voice_setting"]["speed"].as_f64().unwrap() - 1.5).abs() < 1e-3);
        assert_eq!(v["voice_setting"]["pitch"], 3);
        assert!((v["voice_setting"]["vol"].as_f64().unwrap() - 0.8).abs() < 1e-3);
        assert_eq!(v["voice_setting"]["emotion"], "happy");
        assert_eq!(v["audio_setting"]["format"], "pcm");
        assert_eq!(v["audio_setting"]["sample_rate"], 16000);
    }

    #[test]
    fn aggregate_frames_concatenates_audio() {
        let frames = vec![
            frame("task_started", None, 0, false),
            frame("task_continued", Some("01ab"), 0, false),
            frame("task_continued", Some("cdef"), 0, false),
            frame("task_finished", None, 0, true),
        ];
        let outcome = aggregate_frames(frames).unwrap();
        assert_eq!(outcome.audio.as_ref(), &[0x01u8, 0xab, 0xcd, 0xef]);
    }

    #[test]
    fn aggregate_frames_tolerates_null_data() {
        // Initial frame has data=null — must not panic.
        let frames = vec![
            frame("task_started", None, 0, false),
            frame("task_finished", None, 0, true),
        ];
        let outcome = aggregate_frames(frames).unwrap();
        assert!(outcome.audio.is_empty());
    }

    #[test]
    fn aggregate_frames_propagates_base_resp_error() {
        let frames = vec![frame("task_started", None, 1004, false)];
        let err = aggregate_frames(frames).unwrap_err();
        assert_eq!(err.code, TtsErrorCode::InvalidApiKey);
        assert_eq!(err.upstream_code.as_deref(), Some("1004"));
    }

    #[test]
    fn split_chunks_returns_per_frame_payloads() {
        let frames = vec![
            frame("task_started", None, 0, false),
            frame("task_continued", Some("0102"), 0, false),
            frame("task_continued", Some("03"), 0, false),
            frame("task_finished", None, 0, true),
        ];
        let chunks = split_chunks(frames).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].as_ref(), &[0x01u8, 0x02]);
        assert_eq!(chunks[1].as_ref(), &[0x03u8]);
    }
}
