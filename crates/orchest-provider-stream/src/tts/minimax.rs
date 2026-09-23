//! Minimax `t2a_v2` **WebSocket TTS** protocol primitives (Issue 006). Ported
//! from `agent-runtime-tts-providers`'s `providers/minimax/protocol.rs`, now over
//! the spine [`ProtocolError`] instead of a provider-local `TtsError`.
//!
//! The dialect frames text as `task_start` → `task_continue` → `task_finish`,
//! and receives `task_started` / `task_continued` / `task_finished` frames whose
//! `data.audio` is a **hex** string (or `null` on the opening frame). This module
//! owns the wire primitives; the `Tts` impl over [`crate::transport`] wraps them.
//!
//! Reference: `docs/external/minimax/tts_sync.md` (frames) and `tts_async.md`
//! §base_resp (error-code union).

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use orchest_protocol::{
    AudioFormat, Capability, CapabilityDescriptor, ErrorCode, EventStream, Modality, ProtocolError,
    RealtimeHandle, StreamEvent, SynthesizeRequest, SynthesizeResult, TokenUsage, Tts,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::ws::{connect_async, tungstenite};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::transport::{ByteDuplex, WsDuplex, WsFrame};
use crate::tts::require_explicit_voice;

const DEFAULT_WSS_URL: &str = "wss://api.minimaxi.com/ws/v1/t2a_v2";

#[derive(Debug, Clone, Deserialize)]
pub struct BaseResp {
    #[serde(default)]
    pub status_code: i64,
    #[serde(default)]
    pub status_msg: String,
}

impl BaseResp {
    pub fn is_ok(&self) -> bool {
        self.status_code == 0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskStartFrame<'a> {
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
pub struct TaskContinueFrame<'a> {
    pub event: &'static str,
    pub text: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskFinishFrame {
    pub event: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InboundFrame {
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
pub struct InboundData {
    #[serde(default)]
    pub audio: Option<String>,
    /// Per-frame status (1=in-progress, 2=final). The frame `event` discriminant
    /// + `is_final` cover terminal detection; kept as part of the protocol.
    #[serde(default)]
    pub status: Option<i64>,
}

/// Decode the `data.audio` hex string. `None`/empty returns empty `Bytes` —
/// Minimax sends `data: null` on the opening `task_started` frame, which must
/// not panic.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn decode_hex_audio(audio: Option<&str>) -> Result<Bytes, ProtocolError> {
    let Some(s) = audio.filter(|s| !s.is_empty()) else {
        return Ok(Bytes::new());
    };
    let bytes = hex::decode(s).map_err(|err| {
        ProtocolError::new(
            ErrorCode::InvalidAudio,
            format!("Minimax hex audio decode failed: {err}"),
        )
    })?;
    Ok(Bytes::from(bytes))
}

/// Map a Minimax `base_resp` block to a [`ProtocolError`], or `None` on success
/// (`status_code == 0`). The raw upstream code is preserved in the message.
pub fn map_base_resp(resp: &BaseResp) -> Option<ProtocolError> {
    if resp.is_ok() {
        return None;
    }
    let code = resp.status_code;
    let (error_code, status) = match code {
        1001 | 2201 => (ErrorCode::Timeout, None),
        // Rate-limit family — surface as HTTP 429, raw code preserved.
        1002 | 1039 | 2205 => (ErrorCode::ProviderHttpError, Some(429u16)),
        1004 => (ErrorCode::InvalidApiKey, None),
        1042 | 2203 | 2204 | 2013 => (ErrorCode::InvalidRequest, None),
        2202 => (ErrorCode::ProviderStreamError, None),
        _ => (ErrorCode::ProviderTaskFailed, None),
    };
    let detail = if resp.status_msg.is_empty() {
        format!("Minimax error {code}")
    } else {
        format!("Minimax error {code}: {}", resp.status_msg)
    };
    let mut err = ProtocolError::new(error_code, detail);
    if let Some(status) = status {
        err = err.with_status(status);
    }
    Some(err)
}

pub fn minimax_audio_format(format: &AudioFormat) -> &'static str {
    match format {
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Wav | AudioFormat::WavPcm16Le => "wav",
        AudioFormat::Flac | AudioFormat::OggOpus | AudioFormat::Ogg => "flac",
        _ => "pcm",
    }
}

// ---------------------------------------------------------------------------
// Frame bodies + aggregation (the pure core of the t2a_v2 session)
// ---------------------------------------------------------------------------

/// `task_start` body: model + voice/audio settings. `voice_overrides` (from the
/// spine request's `options`) is merged into `voice_setting` (speed/pitch/vol/…).
pub fn build_task_start_body(
    model: &str,
    voice_id: &str,
    format: &AudioFormat,
    sample_rate: Option<u32>,
    voice_overrides: &Value,
) -> Value {
    let mut voice_setting = json!({ "voice_id": voice_id });
    if let Some(overrides) = voice_overrides.as_object() {
        for (key, value) in overrides {
            voice_setting[key] = value.clone();
        }
    }
    let mut audio_setting = json!({ "format": minimax_audio_format(format) });
    if let Some(sr) = sample_rate {
        audio_setting["sample_rate"] = json!(sr);
    }
    serde_json::to_value(TaskStartFrame {
        event: "task_start",
        model,
        voice_setting,
        audio_setting,
        language_boost: None,
        pronunciation_dict: None,
    })
    .expect("static frame serializes")
}

/// `task_continue` body: the text to synthesize.
pub fn build_task_continue_body(text: &str) -> Value {
    serde_json::to_value(TaskContinueFrame {
        event: "task_continue",
        text,
    })
    .expect("static frame serializes")
}

/// `task_finish` body.
pub fn build_task_finish_body() -> Value {
    serde_json::to_value(TaskFinishFrame {
        event: "task_finish",
    })
    .expect("static frame serializes")
}

/// Aggregate a sequence of inbound JSON frames into one audio buffer, stopping on
/// `task_finished` / `is_final`. Surfaces the first non-zero `base_resp` as a
/// [`ProtocolError`]. The pure core the live WSS runner feeds raw text frames.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn aggregate_frames<I: IntoIterator<Item = String>>(frames: I) -> Result<Bytes, ProtocolError> {
    let mut buf = BytesMut::new();
    for raw in frames {
        let frame: InboundFrame = serde_json::from_str(&raw).map_err(|err| {
            ProtocolError::new(
                ErrorCode::ProviderStreamError,
                format!("Minimax inbound frame parse failed: {err}"),
            )
        })?;
        if let Some(err) = map_base_resp(&frame.base_resp) {
            return Err(err);
        }
        let chunk = decode_hex_audio(frame.data.as_ref().and_then(|d| d.audio.as_deref()))?;
        buf.extend_from_slice(&chunk);
        if frame.event == "task_finished" || frame.is_final {
            break;
        }
    }
    Ok(buf.freeze())
}

// ---------------------------------------------------------------------------
// Streaming loop + the spine `Tts` impl
// ---------------------------------------------------------------------------

/// Drive one minimax t2a_v2 synthesis over `transport`: send `task_start` /
/// `task_continue(text)` / `task_finish` as **text** frames, then stream inbound
/// **text** frames — each `data.audio` (hex) becomes an `AudioDelta`, a non-zero
/// `base_resp` a fatal `Error`, and `task_finished` / `is_final` a terminal
/// `Done`.
pub async fn run_minimax_synthesis<T: ByteDuplex>(
    mut transport: T,
    start_body: Value,
    text: String,
    format: AudioFormat,
    events: mpsc::Sender<StreamEvent>,
) {
    for body in [
        start_body,
        build_task_continue_body(&text),
        build_task_finish_body(),
    ] {
        if transport
            .send(WsFrame::Text(body.to_string()))
            .await
            .is_err()
        {
            return;
        }
    }
    while let Some(frame) = transport.recv().await {
        let Some(raw) = frame.as_text() else {
            continue;
        };
        let inbound: InboundFrame = match serde_json::from_str(raw) {
            Ok(frame) => frame,
            Err(e) => {
                let _ = events
                    .send(StreamEvent::Error {
                        error: ProtocolError::new(
                            ErrorCode::ProviderStreamError,
                            format!("minimax frame parse: {e}"),
                        ),
                        fatal: true,
                    })
                    .await;
                break;
            }
        };
        if let Some(error) = map_base_resp(&inbound.base_resp) {
            let _ = events.send(StreamEvent::Error { error, fatal: true }).await;
            break;
        }
        let audio = inbound.data.as_ref().and_then(|d| d.audio.as_deref());
        match decode_hex_audio(audio) {
            Ok(bytes) if !bytes.is_empty() => {
                if events
                    .send(StreamEvent::AudioDelta {
                        data: bytes,
                        format,
                        sequence: 0,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Ok(_) => {}
            Err(error) => {
                let _ = events.send(StreamEvent::Error { error, fatal: true }).await;
                break;
            }
        }
        if inbound.event == "task_finished" || inbound.is_final {
            let _ = events
                .send(StreamEvent::Done {
                    usage: TokenUsage::default(),
                })
                .await;
            break;
        }
    }
}

/// Minimax WebSocket TTS configuration (the `t2a_v2` endpoint).
#[derive(Debug, Clone)]
pub struct MinimaxTtsConfig {
    pub model: String,
    pub ws_url: String,
    pub api_key: String,
}

/// The minimax streaming TTS provider as the spine [`Tts`].
pub struct MinimaxTts {
    config: MinimaxTtsConfig,
}

impl MinimaxTts {
    pub fn new(config: MinimaxTtsConfig) -> Self {
        Self { config }
    }

    async fn open_stream(&self, request: SynthesizeRequest) -> Result<EventStream, ProtocolError> {
        if !self.config.ws_url.starts_with("wss://") {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "WebSocket URL must use wss:// for secure credential transport",
            ));
        }
        let voice = require_explicit_voice(&request)?;
        let start_body = build_task_start_body(
            &self.config.model,
            voice,
            &request.format,
            None,
            &request.options,
        );
        let ws_request = tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Host", host_of(&self.config.ws_url))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| {
                ProtocolError::new(ErrorCode::InvalidRequest, format!("build ws request: {e}"))
            })?;

        let (ws_stream, _response) = connect_async(ws_request).await.map_err(|e| {
            ProtocolError::new(
                ErrorCode::ProviderStreamError,
                format!("Minimax WebSocket connection failed: {e}"),
            )
        })?;

        let (events_tx, events) = EventStream::channel(64);
        tokio::spawn(run_minimax_synthesis(
            WsDuplex::new(ws_stream),
            start_body,
            request.text,
            request.format,
            events_tx,
        ));
        Ok(events)
    }
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split(['/', '?']).next())
        .unwrap_or("api.minimaxi.com")
}

/// The static descriptor the registry filters on for the minimax TTS dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("minimax", "speech-2.8-hd", Capability::Tts)
        .streaming(true)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Audio])
}

/// Build a [`MinimaxTts`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<MinimaxTts, ProtocolError> {
    let api_key = cfg
        .api_key
        .clone()
        .ok_or_else(|| ProtocolError::new(ErrorCode::MissingApiKey, "minimax requires api_key"))?;
    let model = if cfg.model.is_empty() {
        "speech-2.8-hd".to_string()
    } else {
        cfg.model.clone()
    };
    let ws_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_WSS_URL.to_string());
    Ok(MinimaxTts::new(MinimaxTtsConfig {
        model,
        ws_url,
        api_key,
    }))
}

#[async_trait]
impl Tts for MinimaxTts {
    fn provider_name(&self) -> &str {
        "minimax"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("minimax", self.config.model.clone(), Capability::Tts)
            .streaming(true)
            .with_input_modalities([Modality::Text])
            .with_output_modalities([Modality::Audio])
    }

    async fn synthesize(
        &self,
        request: SynthesizeRequest,
    ) -> Result<SynthesizeResult, ProtocolError> {
        let format = request.format;
        let mut stream = self.open_stream(request).await?;
        let mut audio = Vec::new();
        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::AudioDelta { data, .. } => audio.extend_from_slice(&data),
                StreamEvent::Error { error, .. } => return Err(error),
                StreamEvent::Done { .. } => break,
                _ => {}
            }
        }
        Ok(SynthesizeResult {
            audio: Bytes::from(audio),
            format,
            diagnostic_metadata: Value::Null,
        })
    }

    async fn stream_synthesize(
        &self,
        request: SynthesizeRequest,
    ) -> Result<EventStream, ProtocolError> {
        self.open_stream(request).await
    }

    async fn start_duplex_stream(&self) -> Result<RealtimeHandle, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "minimax TTS duplex stream is not wired on the spine",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hex_audio_handles_none_and_empty() {
        assert!(decode_hex_audio(None).unwrap().is_empty());
        assert!(decode_hex_audio(Some("")).unwrap().is_empty());
    }

    #[test]
    fn decode_hex_audio_round_trips() {
        let raw = [0x01u8, 0xab, 0xcd, 0xef];
        assert_eq!(
            decode_hex_audio(Some(&hex::encode(raw))).unwrap().as_ref(),
            raw
        );
    }

    #[test]
    fn decode_hex_audio_invalid_is_error() {
        assert_eq!(
            decode_hex_audio(Some("zzz")).unwrap_err().code,
            ErrorCode::InvalidAudio
        );
    }

    #[test]
    fn map_base_resp_zero_is_none() {
        assert!(map_base_resp(&BaseResp {
            status_code: 0,
            status_msg: "success".into()
        })
        .is_none());
    }

    #[test]
    fn map_base_resp_classifies_codes() {
        let cases = [
            (1001i64, ErrorCode::Timeout, None),
            (2201, ErrorCode::Timeout, None),
            (1002, ErrorCode::ProviderHttpError, Some(429u16)),
            (1004, ErrorCode::InvalidApiKey, None),
            (2203, ErrorCode::InvalidRequest, None),
            (2202, ErrorCode::ProviderStreamError, None),
            (9999, ErrorCode::ProviderTaskFailed, None),
        ];
        for (code, expect, status) in cases {
            let err = map_base_resp(&BaseResp {
                status_code: code,
                status_msg: "x".into(),
            })
            .expect("error");
            assert_eq!(err.code, expect, "code {code}");
            assert_eq!(err.status, status, "code {code}");
            assert!(err.message.contains(&code.to_string()));
        }
    }

    #[test]
    fn inbound_frame_parses_null_data() {
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
        assert_eq!(
            decode_hex_audio(data.audio.as_deref()).unwrap().as_ref(),
            &[0x01, 0xab]
        );
    }

    #[test]
    fn audio_format_mapping() {
        assert_eq!(minimax_audio_format(&AudioFormat::Mp3), "mp3");
        assert_eq!(minimax_audio_format(&AudioFormat::Pcm16Le), "pcm");
        assert_eq!(minimax_audio_format(&AudioFormat::WavPcm16Le), "wav");
        assert_eq!(minimax_audio_format(&AudioFormat::Flac), "flac");
    }

    struct ChannelDuplex {
        out: mpsc::Sender<WsFrame>,
        inbound: mpsc::Receiver<WsFrame>,
    }

    #[async_trait]
    impl ByteDuplex for ChannelDuplex {
        async fn send(&mut self, frame: WsFrame) -> Result<(), ProtocolError> {
            self.out
                .send(frame)
                .await
                .map_err(|_| ProtocolError::new(ErrorCode::ProviderStreamError, "closed"))
        }
        async fn recv(&mut self) -> Option<WsFrame> {
            self.inbound.recv().await
        }
    }

    #[tokio::test]
    async fn run_minimax_synthesis_streams_audio_then_done() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let start = build_task_start_body("m", "v", &AudioFormat::Mp3, None, &json!({}));
        let handle = tokio::spawn(run_minimax_synthesis(
            transport,
            start,
            "hi".to_string(),
            AudioFormat::Mp3,
            events_tx,
        ));

        // the loop sends task_start / task_continue / task_finish as text frames
        for expected in ["task_start", "task_continue", "task_finish"] {
            match out_rx.recv().await.unwrap() {
                WsFrame::Text(t) => assert!(t.contains(expected), "missing {expected}"),
                other => panic!("expected text frame, got {other:?}"),
            }
        }

        // server streams a hex audio chunk, then task_finished
        in_tx
            .send(WsFrame::Text(frame(
                "task_continued",
                Some("01ab"),
                0,
                false,
            )))
            .await
            .unwrap();
        in_tx
            .send(WsFrame::Text(frame("task_finished", None, 0, true)))
            .await
            .unwrap();
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::AudioDelta { .. }
        ));
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Done { .. }
        ));
        handle.await.unwrap();
    }

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
    fn task_start_body_merges_overrides_and_format() {
        let v = build_task_start_body(
            "speech-2.8-hd",
            "xiaoming",
            &AudioFormat::Mp3,
            Some(16000),
            &json!({"speed": 1.5, "emotion": "happy"}),
        );
        assert_eq!(v["event"], "task_start");
        assert_eq!(v["model"], "speech-2.8-hd");
        assert_eq!(v["voice_setting"]["voice_id"], "xiaoming");
        assert!((v["voice_setting"]["speed"].as_f64().unwrap() - 1.5).abs() < 1e-3);
        assert_eq!(v["voice_setting"]["emotion"], "happy");
        assert_eq!(v["audio_setting"]["format"], "mp3");
        assert_eq!(v["audio_setting"]["sample_rate"], 16000);
    }

    #[test]
    fn continue_and_finish_bodies() {
        assert_eq!(build_task_continue_body("hi")["text"], "hi");
        assert_eq!(build_task_finish_body()["event"], "task_finish");
    }

    #[test]
    fn aggregate_frames_concatenates_audio() {
        let frames = vec![
            frame("task_started", None, 0, false),
            frame("task_continued", Some("01ab"), 0, false),
            frame("task_continued", Some("cdef"), 0, false),
            frame("task_finished", None, 0, true),
        ];
        assert_eq!(
            aggregate_frames(frames).unwrap().as_ref(),
            &[0x01u8, 0xab, 0xcd, 0xef]
        );
    }

    #[test]
    fn aggregate_frames_tolerates_null_data() {
        let frames = vec![
            frame("task_started", None, 0, false),
            frame("task_finished", None, 0, true),
        ];
        assert!(aggregate_frames(frames).unwrap().is_empty());
    }

    #[test]
    fn aggregate_frames_propagates_base_resp_error() {
        let err = aggregate_frames(vec![frame("task_started", None, 1004, false)]).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidApiKey);
        assert!(err.message.contains("1004"));
    }

    #[tokio::test]
    async fn rejects_missing_voice_before_network() {
        let tts = MinimaxTts::new(MinimaxTtsConfig {
            model: "speech-2.8-hd".into(),
            ws_url: "wss://example.invalid/tts".into(),
            api_key: "test-key".into(),
        });
        let err = tts
            .synthesize(SynthesizeRequest {
                text: "hi".into(),
                voice: None,
                format: AudioFormat::Mp3,
                options: Value::Null,
            })
            .await
            .expect_err("missing voice must fail before dialing the provider");
        assert_eq!(err.code, ErrorCode::InvalidRequest);
        assert!(
            err.message.contains("voice"),
            "expected voice-focused message, got {}",
            err.message
        );
    }
}
