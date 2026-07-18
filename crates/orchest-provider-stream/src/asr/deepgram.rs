//! Deepgram streaming-ASR result codec (Issue 006). Ported from
//! `agent-runtime-asr-providers`'s `providers/deepgram/mod.rs` parsing, now over
//! the spine [`ProtocolError`] / [`StreamEvent`].
//!
//! Deepgram's `v1/listen` WebSocket takes audio as **binary** frames and a few
//! **text** control frames (`Finalize` / `CloseStream`), and returns `Results` /
//! `Error` / lifecycle messages as **text JSON**. This module owns the inbound
//! JSON → unified-event projection; the live `Asr` impl (which needs a transport
//! that carries both binary audio and text control) wraps it once the transport
//! is generalized.

use async_trait::async_trait;
use orchest_protocol::{
    Asr, Capability, CapabilityDescriptor, ErrorCode, EventStream, Language, LifecycleEvent,
    Modality, ProtocolError, RealtimeHandle, SegmentRef, SessionInput, StreamEvent,
    StreamingTranscribeRequest, TranscribeRequest, TranscribeResult, TranscriptStability,
    TranscriptUpdateKind,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::ws::{connect_async, tungstenite};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::transport::{ByteDuplex, WsDuplex, WsFrame};

const DEFAULT_WS_URL: &str = "wss://api.deepgram.com/v1/listen";

/// A decoded Deepgram inbound message.
#[derive(Debug, PartialEq)]
pub enum DeepgramMessage {
    Result(DeepgramResult),
    Error(String),
    /// Metadata / UtteranceEnd / SpeechStarted / KeepAlive / unknown — no content.
    Ignored,
}

/// The fields of a `Results` message the spine cares about.
#[derive(Debug, PartialEq)]
pub struct DeepgramResult {
    pub transcript: String,
    /// Deepgram `is_final`: this interim segment is now stable (committed).
    pub is_final: bool,
    /// Deepgram `speech_final`: an endpoint (end of utterance) was detected.
    pub speech_final: bool,
    pub confidence: Option<f64>,
    /// Deepgram `start`: seconds offset of this speech segment in the stream.
    /// Interim updates of one utterance share the same `start`, so it doubles
    /// as the segment identity (`seg{start_ms}`).
    pub start: f64,
}

/// Parse one Deepgram text message. `Results` → [`DeepgramResult`]; `Error` →
/// its description; everything else is [`DeepgramMessage::Ignored`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_message(text: &str) -> Result<DeepgramMessage, ProtocolError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("failed to parse Deepgram message: {e}"),
        )
    })?;
    match value.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "Results" => Ok(DeepgramMessage::Result(parse_results(value)?)),
        "Error" => Ok(DeepgramMessage::Error(
            value
                .get("description")
                .or_else(|| value.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Deepgram stream error")
                .to_string(),
        )),
        _ => Ok(DeepgramMessage::Ignored),
    }
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_results(value: serde_json::Value) -> Result<DeepgramResult, ProtocolError> {
    let message: ResultsMessage = serde_json::from_value(value).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("invalid Deepgram Results message: {e}"),
        )
    })?;
    let alternative = message.channel.alternatives.into_iter().next();
    Ok(DeepgramResult {
        transcript: alternative
            .as_ref()
            .map(|a| a.transcript.clone())
            .unwrap_or_default(),
        is_final: message.is_final,
        speech_final: message.speech_final,
        confidence: alternative.and_then(|a| a.confidence),
        start: message.start,
    })
}

/// Project a Deepgram result onto unified events: a non-empty transcript becomes
/// a `Transcript` (`Committed` once `is_final`, else `Provisional`); a detected
/// endpoint (`speech_final`) additionally emits `EndOfSpeech`. The segment
/// identity comes from the native `start` offset (`seg{start_ms}`, `Snapshot`):
/// interim updates and the final of one utterance share it.
pub fn map_result(result: &DeepgramResult) -> Vec<StreamEvent> {
    let segment = || {
        Some(SegmentRef {
            segment_id: Some(format!("seg{}", (result.start * 1000.0) as u64)),
            update_kind: TranscriptUpdateKind::Snapshot,
        })
    };
    let mut events = Vec::new();
    if !result.transcript.is_empty() {
        events.push(StreamEvent::Transcript {
            text: result.transcript.clone(),
            stability: if result.is_final {
                TranscriptStability::Committed
            } else {
                TranscriptStability::Provisional
            },
            segment: segment(),
        });
    }
    if result.speech_final {
        events.push(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment: segment(),
        }));
    }
    events
}

#[derive(Debug, Deserialize)]
struct ResultsMessage {
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    /// Seconds offset of the segment start in the audio stream.
    #[serde(default)]
    start: f64,
    channel: Channel,
}

#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(default)]
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    confidence: Option<f64>,
}

// ---------------------------------------------------------------------------
// Streaming loop + the spine `Asr` impl
// ---------------------------------------------------------------------------

/// Drive one deepgram streaming session over `transport`: client audio is sent as
/// **binary** frames; on input end a `CloseStream` **text** control frame is sent;
/// inbound **text** `Results` are projected via [`map_result`]. Ends on a fatal
/// error or transport EOF (deepgram closes after `CloseStream`).
pub async fn run_deepgram_stream<T: ByteDuplex>(
    mut transport: T,
    mut input: mpsc::Receiver<SessionInput>,
    events: mpsc::Sender<StreamEvent>,
) {
    let mut input_open = true;
    loop {
        tokio::select! {
            maybe_input = input.recv(), if input_open => match maybe_input {
                Some(SessionInput::Audio(bytes)) => {
                    if transport.send(WsFrame::Binary(bytes.to_vec())).await.is_err() {
                        break;
                    }
                }
                None | Some(SessionInput::Interrupt) => {
                    input_open = false;
                    let _ = transport
                        .send(WsFrame::Text(r#"{"type":"CloseStream"}"#.to_string()))
                        .await;
                }
                Some(_) => {}
            },
            maybe_frame = transport.recv() => match maybe_frame.as_ref().and_then(WsFrame::as_text) {
                Some(text) => match parse_message(text) {
                    Ok(DeepgramMessage::Result(result)) => {
                        for event in map_result(&result) {
                            if events.send(event).await.is_err() {
                                return;
                            }
                        }
                    }
                    Ok(DeepgramMessage::Error(message)) => {
                        let _ = events
                            .send(StreamEvent::Error {
                                error: ProtocolError::new(ErrorCode::ProviderTaskFailed, message),
                                fatal: true,
                            })
                            .await;
                        break;
                    }
                    Ok(DeepgramMessage::Ignored) => {}
                    Err(error) => {
                        let _ = events.send(StreamEvent::Error { error, fatal: true }).await;
                        break;
                    }
                },
                None => match maybe_frame {
                    Some(_) => continue, // binary frame (unexpected) — ignore
                    None => break,
                },
            },
        }
    }
}

/// Deepgram streaming-ASR configuration.
#[derive(Debug, Clone)]
pub struct DeepgramAsrConfig {
    pub model: String,
    pub ws_url: String,
    pub api_key: String,
}

/// The deepgram streaming ASR provider as the spine [`Asr`].
pub struct DeepgramAsr {
    config: DeepgramAsrConfig,
}

impl DeepgramAsr {
    pub fn new(config: DeepgramAsrConfig) -> Self {
        Self { config }
    }
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split(['/', '?']).next())
        .unwrap_or("api.deepgram.com")
}

/// The static descriptor the registry filters on for the deepgram dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("deepgram", "nova-3", Capability::Asr)
        .streaming(true)
        .duplex(true)
        .with_input_modalities([Modality::Audio])
        .with_output_modalities([Modality::Text])
}

/// Build a [`DeepgramAsr`] from a registry [`ProviderConfig`]: `api_url` is the
/// `wss://` endpoint (defaulting to `v1/listen`), `api_key` the secret.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<DeepgramAsr, ProtocolError> {
    let api_key = cfg
        .api_key
        .clone()
        .ok_or_else(|| ProtocolError::new(ErrorCode::MissingApiKey, "deepgram requires api_key"))?;
    let model = if cfg.model.is_empty() {
        "nova-3".to_string()
    } else {
        cfg.model.clone()
    };
    let ws_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_WS_URL.to_string());
    Ok(DeepgramAsr::new(DeepgramAsrConfig {
        model,
        ws_url,
        api_key,
    }))
}

#[async_trait]
impl Asr for DeepgramAsr {
    fn provider_name(&self) -> &str {
        "deepgram"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("deepgram", self.config.model.clone(), Capability::Asr)
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
            "deepgram ASR is streaming-only; use start_stream",
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
        let url = format!(
            "{}?model={}&encoding=linear16&sample_rate=16000&interim_results=true&smart_format=true",
            self.config.ws_url, self.config.model
        );
        let ws_request = tungstenite::http::Request::builder()
            .uri(&url)
            .header("Authorization", format!("Token {}", self.config.api_key))
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
                format!("Deepgram WebSocket connection failed: {e}"),
            )
        })?;

        let (input_tx, input_rx) = mpsc::channel(32);
        let (events_tx, events) = EventStream::channel(64);
        tokio::spawn(run_deepgram_stream(
            WsDuplex::new(ws_stream),
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
    use serde_json::json;

    fn results(transcript: &str, is_final: bool, speech_final: bool, start: f64) -> String {
        json!({
            "type": "Results",
            "is_final": is_final,
            "speech_final": speech_final,
            "start": start,
            "channel": {"alternatives": [{"transcript": transcript, "confidence": 0.9}]}
        })
        .to_string()
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
    async fn run_deepgram_stream_sends_audio_and_maps_results() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let handle = tokio::spawn(run_deepgram_stream(transport, input_rx, events_tx));

        // audio in -> a binary frame
        input_tx
            .send(SessionInput::Audio(bytes::Bytes::from_static(b"pcm")))
            .await
            .unwrap();
        assert!(matches!(out_rx.recv().await.unwrap(), WsFrame::Binary(_)));

        // a final result -> Committed transcript + EndOfSpeech
        in_tx
            .send(WsFrame::Text(results("done", true, true, 1.25)))
            .await
            .unwrap();
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }
        ));
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { .. })
        ));

        // closing input flushes a CloseStream control text frame
        drop(input_tx);
        assert_eq!(
            out_rx.recv().await.unwrap(),
            WsFrame::Text(r#"{"type":"CloseStream"}"#.to_string())
        );
        drop(in_tx); // server closes -> loop ends
        handle.await.unwrap();
    }

    #[test]
    fn parses_interim_and_final_results() {
        let interim = parse_message(&results("hello", false, false, 0.5)).unwrap();
        assert_eq!(
            interim,
            DeepgramMessage::Result(DeepgramResult {
                transcript: "hello".into(),
                is_final: false,
                speech_final: false,
                confidence: Some(0.9),
                start: 0.5,
            })
        );
        match parse_message(&results("hello world", true, true, 0.5)).unwrap() {
            DeepgramMessage::Result(r) => {
                assert!(r.is_final && r.speech_final);
            }
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn missing_start_defaults_to_zero() {
        let raw = json!({
            "type": "Results",
            "is_final": false,
            "channel": {"alternatives": [{"transcript": "hi"}]}
        })
        .to_string();
        match parse_message(&raw).unwrap() {
            DeepgramMessage::Result(r) => assert_eq!(r.start, 0.0),
            other => panic!("expected Result, got {other:?}"),
        }
        let events = map_result(&DeepgramResult {
            transcript: "hi".into(),
            is_final: false,
            speech_final: false,
            confidence: None,
            start: 0.0,
        });
        assert!(matches!(
            &events[0],
            StreamEvent::Transcript {
                segment: Some(SegmentRef { segment_id, .. }),
                ..
            } if segment_id.as_deref() == Some("seg0")
        ));
    }

    #[test]
    fn error_message_is_extracted() {
        let raw = json!({"type": "Error", "description": "bad audio"}).to_string();
        assert_eq!(
            parse_message(&raw).unwrap(),
            DeepgramMessage::Error("bad audio".into())
        );
    }

    #[test]
    fn lifecycle_messages_are_ignored() {
        for ty in ["Metadata", "UtteranceEnd", "SpeechStarted", "KeepAlive"] {
            let raw = json!({"type": ty}).to_string();
            assert_eq!(parse_message(&raw).unwrap(), DeepgramMessage::Ignored);
        }
    }

    #[test]
    fn map_result_emits_transcript_with_stability() {
        let events = map_result(&DeepgramResult {
            transcript: "hi".into(),
            is_final: false,
            speech_final: false,
            confidence: None,
            start: 1.25,
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                segment: Some(SegmentRef {
                    segment_id,
                    update_kind: TranscriptUpdateKind::Snapshot,
                }),
                ..
            } if segment_id.as_deref() == Some("seg1250")
        ));
    }

    #[test]
    fn map_result_shares_segment_id_across_interim_final_and_endpoint() {
        let interim = map_result(&DeepgramResult {
            transcript: "do".into(),
            is_final: false,
            speech_final: false,
            confidence: None,
            start: 2.5,
        });
        let r#final = map_result(&DeepgramResult {
            transcript: "done".into(),
            is_final: true,
            speech_final: true,
            confidence: Some(0.8),
            start: 2.5,
        });
        assert!(matches!(
            &interim[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                segment: Some(SegmentRef { segment_id, .. }),
                ..
            } if segment_id.as_deref() == Some("seg2500")
        ));
        assert!(matches!(
            &r#final[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                segment: Some(SegmentRef { segment_id, .. }),
                ..
            } if segment_id.as_deref() == Some("seg2500")
        ));
        assert!(matches!(
            &r#final[1],
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
                segment: Some(SegmentRef { segment_id, .. }),
            }) if segment_id.as_deref() == Some("seg2500")
        ));
    }

    #[test]
    fn empty_transcript_emits_nothing() {
        assert!(map_result(&DeepgramResult {
            transcript: String::new(),
            is_final: true,
            speech_final: false,
            confidence: None,
            start: 0.0,
        })
        .is_empty());
    }
}
