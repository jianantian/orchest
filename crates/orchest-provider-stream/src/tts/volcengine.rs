//! Volcengine openspeech **TTS** event-framing codec, on the shared
//! [`crate::openspeech`] core (Issue 006). Ported from
//! `agent-runtime-tts-providers`'s `providers/volcengine/protocol.rs` — same
//! event-tagged frame layout, now over the unified header/constants and the
//! spine [`ProtocolError`] instead of a provider-local protocol + `TtsError`.
//!
//! TTS uses the `FLAG_WITH_EVENT` framing: after the 4-byte header come an
//! `event` (i32), then — for session/task frames — a length-prefixed
//! `session_id`, then the length-prefixed payload (or raw audio).

use async_trait::async_trait;
use bytes::Bytes;
use orchest_protocol::{
    AudioFormat, Capability, CapabilityDescriptor, ErrorCode, EventStream, Modality, ProtocolError,
    RealtimeHandle, StreamEvent, SynthesizeRequest, SynthesizeResult, TokenUsage, Tts,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::ws::{connect_async, tungstenite};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::openspeech::{
    build_header, parse_header, COMP_NONE, FLAG_WITH_EVENT, MSG_AUDIO_ONLY_REQUEST,
    MSG_AUDIO_ONLY_RESPONSE, MSG_ERROR_RESPONSE, MSG_FULL_CLIENT_REQUEST, MSG_FULL_SERVER_RESPONSE,
    SER_JSON, SER_NONE,
};
use crate::transport::{ByteDuplex, WsDuplex, WsFrame};

const DEFAULT_UNIDIRECTIONAL_WS_URL: &str =
    "wss://openspeech.bytedance.com/api/v3/tts/unidirectional/stream";

// openspeech TTS event numbers.
pub const EVENT_START_CONNECTION: i32 = 1;
pub const EVENT_CONNECTION_STARTED: i32 = 50;
pub const EVENT_CONNECTION_FAILED: i32 = 51;
pub const EVENT_CONNECTION_FINISHED: i32 = 52;
pub const EVENT_START_SESSION: i32 = 100;
pub const EVENT_CANCEL_SESSION: i32 = 101;
pub const EVENT_FINISH_SESSION: i32 = 102;
pub const EVENT_SESSION_STARTED: i32 = 150;
pub const EVENT_SESSION_FINISHED: i32 = 152;
pub const EVENT_SESSION_FAILED: i32 = 153;
pub const EVENT_TASK_REQUEST: i32 = 200;
pub const EVENT_TTS_RESPONSE: i32 = 352;

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn stream_err(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::ProviderStreamError, message)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn serialize(payload: &Value) -> Result<Vec<u8>, ProtocolError> {
    serde_json::to_vec(payload)
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidRequest, format!("serialize: {e}")))
}

#[derive(Debug)]
pub enum VolcengineFrame {
    Meta {
        event: i32,
        session_id: String,
        payload: Value,
    },
    Audio {
        event: i32,
        session_id: String,
        data: Vec<u8>,
    },
    Error {
        code: u32,
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Frame building
// ---------------------------------------------------------------------------

/// Connection-level frame (StartConnection / FinishConnection): event, no session_id.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn build_connect_frame(event: i32, payload: &Value) -> Result<Vec<u8>, ProtocolError> {
    let payload = serialize(payload)?;
    let mut out = build_header(
        MSG_FULL_CLIENT_REQUEST,
        FLAG_WITH_EVENT,
        SER_JSON,
        COMP_NONE,
    )
    .to_vec();
    out.extend_from_slice(&event.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Session / task frame (StartSession, FinishSession, TaskRequest): event +
/// session_id + payload.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn build_meta_frame(
    event: i32,
    session_id: &str,
    payload: &Value,
) -> Result<Vec<u8>, ProtocolError> {
    let payload = serialize(payload)?;
    let session = session_id.as_bytes();
    let mut out = build_header(
        MSG_FULL_CLIENT_REQUEST,
        FLAG_WITH_EVENT,
        SER_JSON,
        COMP_NONE,
    )
    .to_vec();
    out.extend_from_slice(&event.to_be_bytes());
    out.extend_from_slice(&(session.len() as u32).to_be_bytes());
    out.extend_from_slice(session);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Client audio frame (`MSG_AUDIO_ONLY_REQUEST`): event + session_id + raw audio.
/// The bidirectional (omni) client direction; the unidirectional TTS path never
/// sends audio, but both dialects share this openspeech framing.
pub fn build_audio_frame(event: i32, session_id: &str, audio: &[u8]) -> Vec<u8> {
    let session = session_id.as_bytes();
    let mut out =
        build_header(MSG_AUDIO_ONLY_REQUEST, FLAG_WITH_EVENT, SER_NONE, COMP_NONE).to_vec();
    out.extend_from_slice(&event.to_be_bytes());
    out.extend_from_slice(&(session.len() as u32).to_be_bytes());
    out.extend_from_slice(session);
    out.extend_from_slice(&(audio.len() as u32).to_be_bytes());
    out.extend_from_slice(audio);
    out
}

// ---------------------------------------------------------------------------
// Frame parsing
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_frame(data: &[u8]) -> Result<VolcengineFrame, ProtocolError> {
    if data.len() < 8 {
        return Err(stream_err("Volcengine frame too short"));
    }
    let header = parse_header(data)?;
    match header.msg_type {
        MSG_FULL_SERVER_RESPONSE => {
            let (event, session_id, payload_bytes) = parse_event_session_payload(data)?;
            let payload = serde_json::from_slice(&payload_bytes)
                .map_err(|e| stream_err(format!("parse Volcengine meta JSON: {e}")))?;
            Ok(VolcengineFrame::Meta {
                event,
                session_id,
                payload,
            })
        }
        MSG_AUDIO_ONLY_RESPONSE => {
            let (event, session_id, data) = parse_event_session_payload(data)?;
            Ok(VolcengineFrame::Audio {
                event,
                session_id,
                data,
            })
        }
        MSG_ERROR_RESPONSE => parse_error(data),
        other => Err(stream_err(format!(
            "unexpected Volcengine frame type {other}"
        ))),
    }
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_event_session_payload(data: &[u8]) -> Result<(i32, String, Vec<u8>), ProtocolError> {
    if data.len() < 12 {
        return Err(stream_err("Volcengine event frame too short"));
    }
    let event = i32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let session_len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let session_start = 12;
    let payload_len_start = session_start + session_len;
    if data.len() < payload_len_start + 4 {
        return Err(stream_err("Volcengine frame missing payload length"));
    }
    let session_id = String::from_utf8_lossy(&data[session_start..payload_len_start]).to_string();
    let payload_len = u32::from_be_bytes([
        data[payload_len_start],
        data[payload_len_start + 1],
        data[payload_len_start + 2],
        data[payload_len_start + 3],
    ]) as usize;
    let payload_start = payload_len_start + 4;
    if data.len() < payload_start + payload_len {
        return Err(stream_err("Volcengine frame payload truncated"));
    }
    Ok((
        event,
        session_id,
        data[payload_start..payload_start + payload_len].to_vec(),
    ))
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_error(data: &[u8]) -> Result<VolcengineFrame, ProtocolError> {
    if data.len() < 12 {
        return Err(stream_err("Volcengine error frame too short"));
    }
    let code = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let message = if data.len() >= 12 + len {
        String::from_utf8_lossy(&data[12..12 + len]).to_string()
    } else {
        String::new()
    };
    Ok(VolcengineFrame::Error { code, message })
}

// ---------------------------------------------------------------------------
// Frame → unified event mapping
// ---------------------------------------------------------------------------

/// Project a decoded TTS frame onto a unified [`StreamEvent`]: synthesized audio
/// (`pcm_s16le` @ 24 kHz) becomes `AudioDelta`; an error frame a fatal `Error`;
/// session/lifecycle `Meta` frames carry no content event.
pub fn map_frame(frame: VolcengineFrame) -> Option<StreamEvent> {
    match frame {
        VolcengineFrame::Audio { data, .. } => Some(StreamEvent::AudioDelta {
            data: Bytes::from(data),
            format: AudioFormat::Pcm16Le,
            sequence: 0,
        }),
        VolcengineFrame::Error { code, message } => Some(StreamEvent::Error {
            error: ProtocolError::new(
                ErrorCode::ProviderTaskFailed,
                format!("Volcengine TTS error {code}: {message}"),
            ),
            fatal: true,
        }),
        VolcengineFrame::Meta { .. } => None,
    }
}

// ---------------------------------------------------------------------------
// Unidirectional synthesis loop (transport-agnostic; live WS is a thin adapter)
// ---------------------------------------------------------------------------

/// Drive one unidirectional TTS synthesis over `transport`: send the prebuilt
/// client `request_frame` (text + voice config), then stream server frames —
/// audio chunks become `AudioDelta`, the terminal `SESSION_FINISHED` emits a
/// final `Done`, and an error frame a fatal `Error`. Generic over [`ByteDuplex`]
/// so the synthesis behavior is testable without a network (the live
/// `tokio-tungstenite` WebSocket is the same thin adapter the ASR path uses).
pub async fn run_tts_synthesis<T: ByteDuplex>(
    mut transport: T,
    request_frame: Vec<u8>,
    events: mpsc::Sender<StreamEvent>,
) {
    if transport
        .send(WsFrame::Binary(request_frame))
        .await
        .is_err()
    {
        return;
    }
    while let Some(frame) = transport.recv().await {
        let Some(bytes) = frame.as_binary() else {
            continue; // openspeech TTS server frames are binary
        };
        match parse_frame(bytes) {
            Ok(frame) => {
                let finished = matches!(
                    &frame,
                    VolcengineFrame::Meta { event, .. } if *event == EVENT_SESSION_FINISHED
                );
                if let Some(event) = map_frame(frame) {
                    if events.send(event).await.is_err() {
                        return;
                    }
                }
                if finished {
                    let _ = events
                        .send(StreamEvent::Done {
                            usage: TokenUsage::default(),
                        })
                        .await;
                    break;
                }
            }
            Err(error) => {
                let _ = events.send(StreamEvent::Error { error, fatal: true }).await;
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Client send frame + payload (unidirectional synthesize)
// ---------------------------------------------------------------------------

/// Unidirectional client send frame: a plain `MSG_FULL_CLIENT_REQUEST` (no event
/// flag), length-prefixed JSON payload. (Duplex uses the event-framing builders.)
pub fn build_send_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = build_header(MSG_FULL_CLIENT_REQUEST, 0, SER_JSON, COMP_NONE).to_vec();
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn audio_format_name(format: &AudioFormat) -> &'static str {
    match format {
        AudioFormat::Mp3 => "mp3",
        AudioFormat::OggOpus | AudioFormat::Ogg => "ogg_opus",
        AudioFormat::Wav | AudioFormat::WavPcm16Le => "wav",
        _ => "pcm",
    }
}

fn build_synthesis_payload(request: &SynthesizeRequest) -> Value {
    let mut req_params = serde_json::json!({
        "text": request.text,
        "speaker": request.voice.clone().unwrap_or_default(),
        "audio_params": {
            "format": audio_format_name(&request.format),
            "sample_rate": 24000,
        },
    });
    if let Some(extra) = request.options.as_object() {
        for (key, value) in extra {
            req_params[key] = value.clone();
        }
    }
    serde_json::json!({ "user": {"uid": "orchest-sdk"}, "req_params": req_params })
}

// ---------------------------------------------------------------------------
// The spine `Tts` impl
// ---------------------------------------------------------------------------

/// Volcengine streaming-TTS configuration (the openspeech unidirectional endpoint).
#[derive(Debug, Clone)]
pub struct VolcengineTtsConfig {
    pub model: String,
    pub ws_url: String,
    pub api_key: String,
    pub access_key: Option<String>,
    pub resource_id: String,
}

/// The Volcengine streaming TTS provider as the spine [`Tts`]. Like the ASR
/// provider, construction is synchronous; the WS handshake is deferred to the
/// synthesize calls.
pub struct VolcengineTts {
    config: VolcengineTtsConfig,
}

impl VolcengineTts {
    pub fn new(config: VolcengineTtsConfig) -> Self {
        Self { config }
    }

    /// Connect, send the unidirectional request frame, and spawn the synthesis
    /// loop, returning the pulled [`EventStream`] of audio/lifecycle events.
    async fn open_stream(&self, request: SynthesizeRequest) -> Result<EventStream, ProtocolError> {
        if !self.config.ws_url.starts_with("wss://") {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "WebSocket URL must use wss:// for secure credential transport",
            ));
        }
        let payload = serde_json::to_vec(&build_synthesis_payload(&request)).map_err(|e| {
            ProtocolError::new(ErrorCode::InvalidRequest, format!("serialize: {e}"))
        })?;
        let request_frame = build_send_frame(&payload);

        let connect_id = uuid::Uuid::new_v4().to_string();
        let request_id = uuid::Uuid::new_v4().to_string();
        let mut builder = tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
            .header("X-Api-Key", &self.config.api_key);
        if let Some(access_key) = &self.config.access_key {
            builder = builder.header("X-Api-Access-Key", access_key);
        }
        let ws_request = builder
            .header("X-Api-Resource-Id", &self.config.resource_id)
            .header("X-Api-Connect-Id", &connect_id)
            .header("X-Api-Request-Id", &request_id)
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

        let (ws_stream, _response) = connect_async(ws_request)
            .await
            .map_err(|e| stream_err(format!("WebSocket connection failed: {e}")))?;

        let (events_tx, events) = EventStream::channel(64);
        tokio::spawn(run_tts_synthesis(
            WsDuplex::new(ws_stream),
            request_frame,
            events_tx,
        ));
        Ok(events)
    }
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split('/').next())
        .unwrap_or("openspeech.bytedance.com")
}

/// The static descriptor the registry filters on for the Volcengine TTS dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("volcengine", "tts", Capability::Tts)
        .streaming(true)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Audio])
}

/// Build a [`VolcengineTts`] from a registry [`ProviderConfig`]: `api_url` is the
/// `wss://` endpoint (defaulting to the unidirectional stream), `api_key` the
/// secret, and `options.{access_key,resource_id}` the dialect knobs (resource_id
/// defaults to the model name).
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<VolcengineTts, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "volcengine TTS requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        "tts".to_string()
    } else {
        cfg.model.clone()
    };
    let ws_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_UNIDIRECTIONAL_WS_URL.to_string());
    let access_key = cfg
        .options
        .get("access_key")
        .and_then(Value::as_str)
        .map(String::from);
    let resource_id = cfg
        .options
        .get("resource_id")
        .and_then(Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| model.clone());
    Ok(VolcengineTts::new(VolcengineTtsConfig {
        model,
        ws_url,
        api_key,
        access_key,
        resource_id,
    }))
}

#[async_trait]
impl Tts for VolcengineTts {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("volcengine", self.config.model.clone(), Capability::Tts)
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
        // The bidirectional event-framing handshake (build_connect_frame /
        // build_meta_frame) lands in a later slice; one-shot synthesis is the
        // primary path and is fully wired above.
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "volcengine TTS duplex stream is not yet wired on the spine",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn connect_frame_header_and_event() {
        let frame = build_connect_frame(EVENT_START_CONNECTION, &json!({})).unwrap();
        let header = parse_header(&frame).unwrap();
        assert_eq!(header.msg_type, MSG_FULL_CLIENT_REQUEST);
        assert_eq!(header.flags, FLAG_WITH_EVENT);
        let event = i32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]);
        assert_eq!(event, EVENT_START_CONNECTION);
    }

    fn server_meta_frame(event: i32, session_id: &str, payload: &Value) -> Vec<u8> {
        // The server echoes meta as a FULL_SERVER_RESPONSE (what parse_frame
        // decodes); build_meta_frame produces the CLIENT side and is checked
        // separately via parse_header.
        let payload = serde_json::to_vec(payload).unwrap();
        let mut out = build_header(
            MSG_FULL_SERVER_RESPONSE,
            FLAG_WITH_EVENT,
            SER_JSON,
            COMP_NONE,
        )
        .to_vec();
        out.extend_from_slice(&event.to_be_bytes());
        out.extend_from_slice(&(session_id.len() as u32).to_be_bytes());
        out.extend_from_slice(session_id.as_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&payload);
        out
    }

    #[test]
    fn client_meta_frame_is_well_formed() {
        let frame =
            build_meta_frame(EVENT_START_SESSION, "sess-42", &json!({"speaker": "vv"})).unwrap();
        let header = parse_header(&frame).unwrap();
        assert_eq!(header.msg_type, MSG_FULL_CLIENT_REQUEST);
        assert_eq!(header.flags, FLAG_WITH_EVENT);
        assert_eq!(
            i32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]),
            EVENT_START_SESSION
        );
    }

    #[test]
    fn server_meta_frame_roundtrips_through_parse() {
        let frame = server_meta_frame(EVENT_SESSION_STARTED, "sess-42", &json!({"speaker": "vv"}));
        match parse_frame(&frame).unwrap() {
            VolcengineFrame::Meta {
                event,
                session_id,
                payload,
            } => {
                assert_eq!(event, EVENT_SESSION_STARTED);
                assert_eq!(session_id, "sess-42");
                assert_eq!(payload["speaker"], "vv");
            }
            _ => panic!("expected Meta"),
        }
    }

    fn audio_frame(session_id: &str, audio: &[u8]) -> Vec<u8> {
        // Build a server audio-only response frame the way the wire does.
        let mut out = build_header(
            MSG_AUDIO_ONLY_RESPONSE,
            FLAG_WITH_EVENT,
            SER_JSON,
            COMP_NONE,
        )
        .to_vec();
        out.extend_from_slice(&EVENT_TTS_RESPONSE.to_be_bytes());
        out.extend_from_slice(&(session_id.len() as u32).to_be_bytes());
        out.extend_from_slice(session_id.as_bytes());
        out.extend_from_slice(&(audio.len() as u32).to_be_bytes());
        out.extend_from_slice(audio);
        out
    }

    #[test]
    fn parse_and_map_audio_response() {
        let frame = audio_frame("sess-1", b"pcm-bytes");
        match parse_frame(&frame).unwrap() {
            VolcengineFrame::Audio {
                event,
                session_id,
                data,
            } => {
                assert_eq!(event, EVENT_TTS_RESPONSE);
                assert_eq!(session_id, "sess-1");
                assert_eq!(data, b"pcm-bytes");
            }
            _ => panic!("expected Audio"),
        }
        let mapped = map_frame(parse_frame(&frame).unwrap());
        assert!(matches!(
            mapped,
            Some(StreamEvent::AudioDelta {
                format: AudioFormat::Pcm16Le,
                ..
            })
        ));
    }

    #[test]
    fn parse_and_map_error() {
        let mut frame =
            build_header(MSG_ERROR_RESPONSE, FLAG_WITH_EVENT, SER_JSON, COMP_NONE).to_vec();
        frame.extend_from_slice(&3000u32.to_be_bytes());
        let msg = b"quota exceeded";
        frame.extend_from_slice(&(msg.len() as u32).to_be_bytes());
        frame.extend_from_slice(msg);
        match map_frame(parse_frame(&frame).unwrap()) {
            Some(StreamEvent::Error { fatal: true, error }) => {
                assert!(error.message.contains("quota exceeded"));
            }
            _ => panic!("expected fatal Error"),
        }
    }

    #[test]
    fn meta_frame_maps_to_no_event() {
        let frame = server_meta_frame(EVENT_SESSION_FINISHED, "s", &json!({}));
        assert!(map_frame(parse_frame(&frame).unwrap()).is_none());
    }

    struct ChannelDuplex {
        out: mpsc::Sender<WsFrame>,
        inbound: mpsc::Receiver<WsFrame>,
    }

    #[async_trait]
    impl ByteDuplex for ChannelDuplex {
        async fn send(&mut self, frame: WsFrame) -> Result<(), ProtocolError> {
            self.out.send(frame).await.map_err(|_| stream_err("closed"))
        }
        async fn recv(&mut self) -> Option<WsFrame> {
            self.inbound.recv().await
        }
    }

    #[tokio::test]
    async fn run_tts_synthesis_streams_audio_then_done() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (events_tx, mut events_rx) = mpsc::channel(8);

        let handle = tokio::spawn(run_tts_synthesis(transport, vec![0u8; 8], events_tx));

        // The synthesis sends the request frame, which the server observes.
        assert!(out_rx.recv().await.is_some());
        // Server streams one audio chunk, then signals SESSION_FINISHED.
        in_tx
            .send(WsFrame::Binary(audio_frame("s", b"pcm")))
            .await
            .unwrap();
        in_tx
            .send(WsFrame::Binary(server_meta_frame(
                EVENT_SESSION_FINISHED,
                "s",
                &json!({}),
            )))
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
        handle.await.unwrap(); // synthesis terminated on SESSION_FINISHED
    }
}
