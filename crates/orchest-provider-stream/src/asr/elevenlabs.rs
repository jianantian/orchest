//! ElevenLabs realtime speech-to-text on the spine (Issue 006). Ported from
//! `agent-runtime-asr-providers`'s `providers/elevenlabs`, over the spine
//! [`ProtocolError`] / [`StreamEvent`].
//!
//! ElevenLabs is an **all-text** WebSocket dialect: client audio is a JSON
//! `input_audio_chunk` carrying base64 PCM; the server replies with
//! `partial_transcript` / `committed_transcript[_with_timestamps]` /
//! `session_started` / `error` JSON messages.

use async_trait::async_trait;
use base64::Engine;
use orchest_protocol::{
    Asr, Capability, CapabilityDescriptor, ErrorCode, EventStream, Language, Modality,
    ProtocolError, RealtimeHandle, SegmentRef, SessionInput, StreamEvent,
    StreamingTranscribeRequest, TranscribeRequest, TranscribeResult, TranscriptStability,
    TranscriptUpdateKind,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::ws::{connect_async, tungstenite};
use serde_json::json;
use tokio::sync::mpsc;

use crate::transport::{ByteDuplex, WsDuplex, WsFrame};

const DEFAULT_WS_URL: &str = "wss://api.elevenlabs.io/v1/speech-to-text/stream";

/// A decoded ElevenLabs inbound message.
#[derive(Debug, PartialEq)]
pub enum ElevenLabsMessage {
    Partial(String),
    Committed(String),
    Error(String),
    /// `session_started` / unknown — no content.
    Ignored,
}

/// First non-empty string among `keys` in `value`.
fn first_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| value.get(k).and_then(|v| v.as_str()))
        .map(ToString::to_string)
}

/// Parse one ElevenLabs text message.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_message(text: &str) -> Result<ElevenLabsMessage, ProtocolError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("failed to parse ElevenLabs message: {e}"),
        )
    })?;
    let message_type = value
        .get("message_type")
        .or_else(|| value.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(match message_type {
        "partial_transcript" => ElevenLabsMessage::Partial(
            first_string(&value, &["text", "transcript"]).unwrap_or_default(),
        ),
        "committed_transcript" | "committed_transcript_with_timestamps" => {
            ElevenLabsMessage::Committed(
                first_string(&value, &["text", "transcript"]).unwrap_or_default(),
            )
        }
        "error" | "auth_error" | "quota_exceeded" => ElevenLabsMessage::Error(
            first_string(&value, &["message", "error", "detail"])
                .unwrap_or_else(|| "ElevenLabs stream error".into()),
        ),
        _ => ElevenLabsMessage::Ignored,
    })
}

/// Stateful projector of ElevenLabs messages onto unified events. The wire has
/// no segment identity, so a synthesized `s{n}` id (`Snapshot`) is shared by a
/// partial and its committed transcript; the counter advances on each commit.
#[derive(Debug, Default)]
pub struct ElevenLabsMapper {
    current: u64,
}

impl ElevenLabsMapper {
    pub fn new() -> Self {
        Self::default()
    }

    fn segment(&self) -> Option<SegmentRef> {
        Some(SegmentRef {
            segment_id: Some(format!("s{}", self.current)),
            update_kind: TranscriptUpdateKind::Snapshot,
        })
    }

    pub fn map(&mut self, message: ElevenLabsMessage) -> Vec<StreamEvent> {
        match message {
            ElevenLabsMessage::Partial(text) if !text.is_empty() => {
                vec![StreamEvent::Transcript {
                    text,
                    stability: TranscriptStability::Provisional,
                    segment: self.segment(),
                }]
            }
            ElevenLabsMessage::Committed(text) if !text.is_empty() => {
                let events = vec![StreamEvent::Transcript {
                    text,
                    stability: TranscriptStability::Committed,
                    segment: self.segment(),
                }];
                self.current += 1;
                events
            }
            ElevenLabsMessage::Error(message) => vec![StreamEvent::Error {
                error: ProtocolError::new(ErrorCode::ProviderTaskFailed, message),
                fatal: true,
            }],
            _ => Vec::new(),
        }
    }
}

/// Build the `input_audio_chunk` text frame carrying base64 PCM. `commit` flushes
/// the final segment (sent with empty audio on input end).
pub fn build_audio_message(data: &[u8], commit: bool, sample_rate: u32) -> String {
    json!({
        "message_type": "input_audio_chunk",
        "audio_base_64": base64::engine::general_purpose::STANDARD.encode(data),
        "sample_rate": sample_rate,
        "commit": commit,
    })
    .to_string()
}

/// Drive one ElevenLabs session: client audio is sent as `input_audio_chunk`
/// **text** frames (base64); input end sends a `commit` frame. Inbound **text**
/// transcripts project via [`ElevenLabsMapper`]. Ends on a fatal error or EOF.
pub async fn run_elevenlabs_stream<T: ByteDuplex>(
    mut transport: T,
    sample_rate: u32,
    mut input: mpsc::Receiver<SessionInput>,
    events: mpsc::Sender<StreamEvent>,
) {
    let mut mapper = ElevenLabsMapper::new();
    let mut input_open = true;
    loop {
        tokio::select! {
            maybe_input = input.recv(), if input_open => match maybe_input {
                Some(SessionInput::Audio(bytes)) => {
                    let frame = build_audio_message(&bytes, false, sample_rate);
                    if transport.send(WsFrame::Text(frame)).await.is_err() {
                        break;
                    }
                }
                None | Some(SessionInput::Interrupt) => {
                    input_open = false;
                    let _ = transport
                        .send(WsFrame::Text(build_audio_message(&[], true, sample_rate)))
                        .await;
                }
                Some(_) => {}
            },
            maybe_frame = transport.recv() => match maybe_frame.as_ref().and_then(WsFrame::as_text) {
                Some(text) => match parse_message(text) {
                    Ok(message) => {
                        let is_error = matches!(message, ElevenLabsMessage::Error(_));
                        for unified in mapper.map(message) {
                            if events.send(unified).await.is_err() {
                                return;
                            }
                        }
                        if is_error {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = events.send(StreamEvent::Error { error, fatal: true }).await;
                        break;
                    }
                },
                None => match maybe_frame {
                    Some(_) => continue,
                    None => break,
                },
            },
        }
    }
}

/// ElevenLabs streaming-ASR configuration.
#[derive(Debug, Clone)]
pub struct ElevenLabsAsrConfig {
    pub model: String,
    pub ws_url: String,
    pub api_key: String,
}

/// The ElevenLabs realtime ASR provider as the spine [`Asr`].
pub struct ElevenLabsAsr {
    config: ElevenLabsAsrConfig,
}

impl ElevenLabsAsr {
    pub fn new(config: ElevenLabsAsrConfig) -> Self {
        Self { config }
    }
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split(['/', '?']).next())
        .unwrap_or("api.elevenlabs.io")
}

/// The static descriptor the registry filters on for the elevenlabs dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("elevenlabs", "scribe-v2-realtime", Capability::Asr)
        .streaming(true)
        .duplex(true)
        .with_input_modalities([Modality::Audio])
        .with_output_modalities([Modality::Text])
}

/// Build an [`ElevenLabsAsr`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<ElevenLabsAsr, ProtocolError> {
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "elevenlabs requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        "scribe-v2-realtime".to_string()
    } else {
        cfg.model.clone()
    };
    let ws_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_WS_URL.to_string());
    Ok(ElevenLabsAsr::new(ElevenLabsAsrConfig {
        model,
        ws_url,
        api_key,
    }))
}

#[async_trait]
impl Asr for ElevenLabsAsr {
    fn provider_name(&self) -> &str {
        "elevenlabs"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("elevenlabs", self.config.model.clone(), Capability::Asr)
            .streaming(true)
            .duplex(true)
            .with_input_modalities([Modality::Audio])
            .with_output_modalities([Modality::Text])
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(
        &self,
        _request: TranscribeRequest,
    ) -> Result<TranscribeResult, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "elevenlabs ASR is streaming-only; use start_stream",
        ))
    }

    async fn start_stream(
        &self,
        _request: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError> {
        if !self.config.ws_url.starts_with("wss://") {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "WebSocket URL must use wss:// for secure credential transport",
            ));
        }
        let ws_request = tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
            .header("xi-api-key", &self.config.api_key)
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
                format!("ElevenLabs WebSocket connection failed: {e}"),
            )
        })?;

        let (input_tx, input_rx) = mpsc::channel(32);
        let (events_tx, events) = EventStream::channel(64);
        tokio::spawn(run_elevenlabs_stream(
            WsDuplex::new(ws_stream),
            16_000,
            input_rx,
            events_tx,
        ));
        Ok(RealtimeHandle {
            input: input_tx,
            events,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_partial_committed_and_error() {
        assert_eq!(
            parse_message(&json!({"message_type": "partial_transcript", "text": "hi"}).to_string())
                .unwrap(),
            ElevenLabsMessage::Partial("hi".into())
        );
        assert_eq!(
            parse_message(
                &json!({"message_type": "committed_transcript", "text": "done"}).to_string()
            )
            .unwrap(),
            ElevenLabsMessage::Committed("done".into())
        );
        match parse_message(
            &json!({"message_type": "auth_error", "message": "bad key"}).to_string(),
        )
        .unwrap()
        {
            ElevenLabsMessage::Error(m) => assert!(m.contains("bad key")),
            other => panic!("expected Error, got {other:?}"),
        }
        assert_eq!(
            parse_message(&json!({"message_type": "session_started"}).to_string()).unwrap(),
            ElevenLabsMessage::Ignored
        );
    }

    #[test]
    fn maps_stability_and_errors() {
        let mut mapper = ElevenLabsMapper::new();
        assert!(matches!(
            mapper
                .map(ElevenLabsMessage::Partial("x".into()))
                .as_slice(),
            [StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                ..
            }]
        ));
        assert!(matches!(
            mapper
                .map(ElevenLabsMessage::Committed("x".into()))
                .as_slice(),
            [StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }]
        ));
        assert!(matches!(
            mapper.map(ElevenLabsMessage::Error("e".into())).as_slice(),
            [StreamEvent::Error { fatal: true, .. }]
        ));
    }

    #[test]
    fn mapper_shares_counter_id_and_advances_on_commit() {
        let mut mapper = ElevenLabsMapper::new();
        let ids: Vec<String> = [
            ElevenLabsMessage::Partial("he".into()),
            ElevenLabsMessage::Committed("hello".into()),
            ElevenLabsMessage::Partial("wo".into()),
        ]
        .into_iter()
        .map(|m| match mapper.map(m).into_iter().next() {
            Some(StreamEvent::Transcript {
                segment:
                    Some(SegmentRef {
                        segment_id: Some(id),
                        update_kind: TranscriptUpdateKind::Snapshot,
                    }),
                ..
            }) => id,
            other => panic!("expected Transcript with segment, got {other:?}"),
        })
        .collect();
        // A partial and its committed share the id; the next partial advances it.
        assert_eq!(ids, ["s0", "s0", "s1"]);
    }

    #[test]
    fn audio_message_is_base64_json() {
        let msg = build_audio_message(b"abc", false, 16000);
        let value: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(value["message_type"], "input_audio_chunk");
        assert_eq!(
            value["audio_base_64"],
            base64::engine::general_purpose::STANDARD.encode(b"abc")
        );
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
    async fn run_elevenlabs_stream_sends_audio_text_and_maps_committed() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let handle = tokio::spawn(run_elevenlabs_stream(transport, 16000, input_rx, events_tx));

        // audio in -> an input_audio_chunk text frame
        input_tx
            .send(SessionInput::Audio(bytes::Bytes::from_static(b"pcm")))
            .await
            .unwrap();
        match out_rx.recv().await.unwrap() {
            WsFrame::Text(t) => assert!(t.contains("input_audio_chunk")),
            other => panic!("expected audio text frame, got {other:?}"),
        }

        // a committed transcript -> Committed Transcript
        in_tx
            .send(WsFrame::Text(
                json!({"message_type": "committed_transcript", "text": "done"}).to_string(),
            ))
            .await
            .unwrap();
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }
        ));

        drop(in_tx); // server closes -> loop ends
        handle.await.unwrap();
    }
}
