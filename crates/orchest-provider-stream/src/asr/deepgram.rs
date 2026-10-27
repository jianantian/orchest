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

/// Whether this dialect honours `SessionInput::Finalize`: it sends Deepgram's
/// native `Finalize` control frame, which flushes buffered audio without
/// closing the stream (<https://developers.deepgram.com/docs/finalize>).
pub const CLIENT_FINALIZE: bool = true;

/// The `Finalize` control text frame.
pub const FINALIZE_FRAME: &str = r#"{"type":"Finalize"}"#;

/// The `CloseStream` control text frame.
pub const CLOSE_STREAM_FRAME: &str = r#"{"type":"CloseStream"}"#;

/// A decoded Deepgram inbound message.
#[derive(Debug, PartialEq)]
pub enum DeepgramMessage {
    Result(DeepgramResult),
    /// `UtteranceEnd`: a gap after the last finalized word — a Turn signal
    /// (<https://developers.deepgram.com/docs/utterance-end>).
    UtteranceEnd,
    Error(String),
    /// Metadata / SpeechStarted / KeepAlive / unknown, or an `UtteranceEnd`
    /// with `last_word_end == -1` (already finalized) — no content.
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
    /// Deepgram `from_finalize`: this result answers a client `Finalize`.
    pub from_finalize: bool,
    pub confidence: Option<f64>,
    /// Deepgram `start`: seconds offset of this speech segment in the stream.
    /// Interim updates of one utterance share the same `start`, so it doubles
    /// as the segment identity (`seg{start_ms}`).
    pub start: f64,
}

/// Parse one Deepgram text message. `Results` → [`DeepgramResult`];
/// `UtteranceEnd` → [`DeepgramMessage::UtteranceEnd`]; `Error` → its
/// description; everything else is [`DeepgramMessage::Ignored`].
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
        "UtteranceEnd" => {
            let already_final = value
                .get("last_word_end")
                .and_then(serde_json::Value::as_f64)
                .is_some_and(|end| end < 0.0);
            Ok(if already_final {
                DeepgramMessage::Ignored
            } else {
                DeepgramMessage::UtteranceEnd
            })
        }
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
        from_finalize: message.from_finalize,
        confidence: alternative.and_then(|a| a.confidence),
        start: message.start,
    })
}

/// Stateful projector of Deepgram messages onto unified events.
///
/// A non-empty transcript becomes a `Transcript` (`Committed` once `is_final`,
/// else `Provisional`) on the segment named by its native `start` offset
/// (`seg{start_ms}`, `Snapshot`). The Turn (`EndOfSpeech`) comes from
/// `speech_final`, `UtteranceEnd` or a `from_finalize` result, and points at the
/// latest Stable not yet ended by a Turn; the three signals are de-duplicated on
/// that segment, so one Stable gets one Turn. A `from_finalize` result with
/// nothing pending still answers the client's `Finalize` with a Turn whose
/// segment is `None`.
#[derive(Debug, Default)]
pub struct DeepgramMapper {
    /// The latest Stable segment that has not been ended by a Turn yet.
    pending: Option<SegmentRef>,
}

impl DeepgramMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map(&mut self, message: DeepgramMessage) -> Vec<StreamEvent> {
        match message {
            DeepgramMessage::Result(result) => self.map_result(&result),
            DeepgramMessage::UtteranceEnd => self.turn(false).into_iter().collect(),
            DeepgramMessage::Error(message) => vec![StreamEvent::Error {
                error: ProtocolError::new(ErrorCode::ProviderTaskFailed, message),
                fatal: true,
            }],
            DeepgramMessage::Ignored => Vec::new(),
        }
    }

    fn map_result(&mut self, result: &DeepgramResult) -> Vec<StreamEvent> {
        let segment = SegmentRef {
            segment_id: Some(format!("seg{}", (result.start * 1000.0) as u64)),
            update_kind: TranscriptUpdateKind::Snapshot,
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
                segment: Some(segment.clone()),
            });
            if result.is_final {
                self.pending = Some(segment);
            }
        }
        if result.speech_final || result.from_finalize {
            events.extend(self.turn(result.from_finalize));
        }
        events
    }

    /// The Turn ending the pending Stable, if any; `always` answers a
    /// `Finalize` even when nothing is pending.
    fn turn(&mut self, always: bool) -> Option<StreamEvent> {
        let segment = self.pending.take();
        (segment.is_some() || always).then_some(StreamEvent::Lifecycle(
            LifecycleEvent::EndOfSpeech { segment },
        ))
    }
}

#[derive(Debug, Deserialize)]
struct ResultsMessage {
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    #[serde(default)]
    from_finalize: bool,
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
/// **binary** frames; `Finalize` sends the `Finalize` **text** control frame and
/// keeps the stream open; on input end a `CloseStream` control frame is sent.
/// Inbound **text** messages project via [`DeepgramMapper`]. Ends on a fatal
/// error, or on transport EOF (deepgram closes after `CloseStream`), which emits
/// `SessionClosed`.
pub async fn run_deepgram_stream<T: ByteDuplex>(
    mut transport: T,
    mut input: mpsc::Receiver<SessionInput>,
    events: mpsc::Sender<StreamEvent>,
) {
    let mut mapper = DeepgramMapper::new();
    let mut input_open = true;
    loop {
        tokio::select! {
            maybe_input = input.recv(), if input_open => match maybe_input {
                Some(SessionInput::Audio(bytes)) => {
                    if transport.send(WsFrame::Binary(bytes.to_vec())).await.is_err() {
                        break;
                    }
                }
                Some(SessionInput::Finalize) => {
                    if transport.send(WsFrame::Text(FINALIZE_FRAME.to_string())).await.is_err() {
                        break;
                    }
                }
                None | Some(SessionInput::Interrupt) => {
                    input_open = false;
                    let _ = transport
                        .send(WsFrame::Text(CLOSE_STREAM_FRAME.to_string()))
                        .await;
                }
                Some(_) => {}
            },
            maybe_frame = transport.recv() => match maybe_frame.as_ref().and_then(WsFrame::as_text) {
                Some(text) => match parse_message(text) {
                    Ok(message) => {
                        let is_error = matches!(message, DeepgramMessage::Error(_));
                        for event in mapper.map(message) {
                            if events.send(event).await.is_err() {
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
                    Some(_) => continue, // binary frame (unexpected) — ignore
                    None => {
                        let _ = events
                            .send(StreamEvent::Lifecycle(LifecycleEvent::SessionClosed))
                            .await;
                        break;
                    }
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
        .with_ext(super::asr_ext(CLIENT_FINALIZE))
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
            .with_ext(super::asr_ext(CLIENT_FINALIZE))
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
            "{}?model={}&encoding=linear16&sample_rate=16000&interim_results=true&utterance_end_ms=1000&smart_format=true",
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
            crate::fixture::live(WsDuplex::new(ws_stream)),
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

    fn result(transcript: &str, is_final: bool, speech_final: bool, start: f64) -> DeepgramMessage {
        parse_message(&results(transcript, is_final, speech_final, start)).unwrap()
    }

    fn finalize_result(transcript: &str, start: f64) -> DeepgramMessage {
        parse_message(
            &json!({
                "type": "Results",
                "is_final": true,
                "speech_final": false,
                "from_finalize": true,
                "start": start,
                "channel": {"alternatives": [{"transcript": transcript}]}
            })
            .to_string(),
        )
        .unwrap()
    }

    fn turn_segment(event: &StreamEvent) -> Option<Option<&str>> {
        match event {
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { segment }) => Some(
                segment
                    .as_ref()
                    .and_then(|segment| segment.segment_id.as_deref()),
            ),
            _ => None,
        }
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
        assert_eq!(
            turn_segment(&events_rx.recv().await.unwrap()),
            Some(Some("seg1250"))
        );

        // closing input flushes a CloseStream control text frame
        drop(input_tx);
        assert_eq!(
            out_rx.recv().await.unwrap(),
            WsFrame::Text(CLOSE_STREAM_FRAME.to_string())
        );
        drop(in_tx); // server closes -> SessionClosed, loop ends
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Lifecycle(LifecycleEvent::SessionClosed)
        ));
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn finalize_sends_native_frame_and_keeps_the_stream_open() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let _handle = tokio::spawn(run_deepgram_stream(transport, input_rx, events_tx));

        input_tx.send(SessionInput::Finalize).await.unwrap();
        assert_eq!(
            out_rx.recv().await.unwrap(),
            WsFrame::Text(FINALIZE_FRAME.to_string())
        );
        in_tx
            .send(WsFrame::Text(
                json!({
                    "type": "Results",
                    "is_final": true,
                    "from_finalize": true,
                    "start": 0.5,
                    "channel": {"alternatives": [{"transcript": "hi"}]}
                })
                .to_string(),
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
        assert_eq!(
            turn_segment(&events_rx.recv().await.unwrap()),
            Some(Some("seg500"))
        );
        // still open: more audio is accepted
        input_tx
            .send(SessionInput::Audio(bytes::Bytes::from_static(b"pcm")))
            .await
            .unwrap();
        assert!(matches!(out_rx.recv().await.unwrap(), WsFrame::Binary(_)));
    }

    #[test]
    fn parses_interim_and_final_results() {
        assert_eq!(
            result("hello", false, false, 0.5),
            DeepgramMessage::Result(DeepgramResult {
                transcript: "hello".into(),
                is_final: false,
                speech_final: false,
                from_finalize: false,
                confidence: Some(0.9),
                start: 0.5,
            })
        );
        match result("hello world", true, true, 0.5) {
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
        let message = parse_message(&raw).unwrap();
        match &message {
            DeepgramMessage::Result(r) => assert_eq!(r.start, 0.0),
            other => panic!("expected Result, got {other:?}"),
        }
        let events = DeepgramMapper::new().map(message);
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
        for ty in ["Metadata", "SpeechStarted", "KeepAlive"] {
            let raw = json!({"type": ty}).to_string();
            assert_eq!(parse_message(&raw).unwrap(), DeepgramMessage::Ignored);
        }
    }

    #[test]
    fn utterance_end_parses_unless_already_final() {
        let raw = json!({"type": "UtteranceEnd", "channel": [0, 1], "last_word_end": 2.395});
        assert_eq!(
            parse_message(&raw.to_string()).unwrap(),
            DeepgramMessage::UtteranceEnd
        );
        let stale = json!({"type": "UtteranceEnd", "channel": [0, 1], "last_word_end": -1});
        assert_eq!(
            parse_message(&stale.to_string()).unwrap(),
            DeepgramMessage::Ignored
        );
    }

    #[test]
    fn interim_is_a_provisional_snapshot() {
        let events = DeepgramMapper::new().map(result("hi", false, false, 1.25));
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
    fn interim_final_and_turn_share_the_segment() {
        let mut mapper = DeepgramMapper::new();
        let interim = mapper.map(result("do", false, false, 2.5));
        let r#final = mapper.map(result("done", true, true, 2.5));
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
        assert_eq!(turn_segment(&r#final[1]), Some(Some("seg2500")));
    }

    #[test]
    fn utterance_end_is_a_turn_for_the_latest_stable() {
        let mut mapper = DeepgramMapper::new();
        assert_eq!(mapper.map(result("one", true, false, 0.0)).len(), 1);
        assert_eq!(mapper.map(result("two", true, false, 1.0)).len(), 1);
        let turn = mapper.map(DeepgramMessage::UtteranceEnd);
        assert_eq!(turn.len(), 1);
        assert_eq!(turn_segment(&turn[0]), Some(Some("seg1000")));
    }

    #[test]
    fn speech_final_and_utterance_end_give_one_turn() {
        let mut mapper = DeepgramMapper::new();
        let r#final = mapper.map(result("done", true, true, 3.0));
        assert_eq!(r#final.len(), 2);
        assert!(mapper.map(DeepgramMessage::UtteranceEnd).is_empty());
    }

    #[test]
    fn utterance_end_without_a_pending_stable_is_ignored() {
        let mut mapper = DeepgramMapper::new();
        assert!(mapper.map(DeepgramMessage::UtteranceEnd).is_empty());
        mapper.map(result("draft", false, false, 0.0));
        assert!(mapper.map(DeepgramMessage::UtteranceEnd).is_empty());
    }

    #[test]
    fn from_finalize_always_answers_with_a_turn() {
        let mut mapper = DeepgramMapper::new();
        let with_text = mapper.map(finalize_result("hi", 0.5));
        assert_eq!(turn_segment(&with_text[1]), Some(Some("seg500")));
        let empty = mapper.map(finalize_result("", 1.0));
        assert_eq!(empty.len(), 1);
        assert_eq!(turn_segment(&empty[0]), Some(None));
    }

    #[test]
    fn empty_transcript_emits_nothing() {
        assert!(DeepgramMapper::new()
            .map(result("", true, false, 0.0))
            .is_empty());
    }

    #[test]
    fn descriptor_declares_client_finalize() {
        match entry_descriptor().ext {
            orchest_protocol::CapabilityExt::Asr(value) => {
                assert_eq!(value[crate::asr::CLIENT_FINALIZE_KEY], json!(true));
            }
            other => panic!("expected Asr ext, got {other:?}"),
        }
    }
}
