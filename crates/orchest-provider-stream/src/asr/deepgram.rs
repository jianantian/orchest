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
    /// A message without `type` that carries `from_finalize: true`: the
    /// documented acknowledgement of a client `Finalize`
    /// (<https://developers.deepgram.com/docs/finalize>).
    FinalizeAck,
    Error(super::UpstreamError),
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
    /// Parsed for diagnostics only; segment identity is the Turn counter
    /// (`final{n}` / `tail{n}`).
    pub start: f64,
}

/// Parse one Deepgram text message. `Results` → [`DeepgramResult`];
/// `UtteranceEnd` → [`DeepgramMessage::UtteranceEnd`]; `Error` → its
/// description; an untyped `{"from_finalize": true}` →
/// [`DeepgramMessage::FinalizeAck`]; everything else is
/// [`DeepgramMessage::Ignored`].
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
        "Error" => Ok(DeepgramMessage::Error(super::UpstreamError::new(
            value
                .get("variant")
                .or_else(|| value.get("err_code"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            value
                .get("description")
                .or_else(|| value.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Deepgram stream error"),
        ))),
        "" if value
            .get("from_finalize")
            .and_then(serde_json::Value::as_bool)
            == Some(true) =>
        {
            Ok(DeepgramMessage::FinalizeAck)
        }
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

/// Prefix of the committed segment of each Turn (`final{n}`): a long
/// utterance can arrive as several `is_final` results before `speech_final`
/// (<https://developers.deepgram.com/docs/understand-endpointing-interim-results>),
/// so they `Append` to one segment.
pub const FINAL_SEGMENT_PREFIX: &str = "final";

/// Prefix of the revisable interim of each Turn (`tail{n}`): each interim is a
/// full snapshot of the not-yet-final part, replacing the previous one.
pub const TAIL_SEGMENT_PREFIX: &str = "tail";

/// Stateful projector of Deepgram messages onto unified events.
///
/// Interim results → a `Snapshot` `Provisional` `Transcript` on `tail{n}`;
/// `is_final` results → an `Append` `Committed` `Transcript` on `final{n}`,
/// space-separated, so every `is_final` piece of an utterance lands in the
/// segment its Turn ends. The Turn (`EndOfSpeech` on `final{n}`) comes from
/// `speech_final`, `UtteranceEnd` or a `from_finalize` result, whichever is
/// first, and starts Turn `n + 1`. Only a `from_finalize` result (or the
/// untyped finalize acknowledgement) answers the client's `Finalize`; with
/// nothing committed its Turn has segment `None`. A `speech_final` /
/// `UtteranceEnd` Turn is a VAD Turn and answers nothing.
#[derive(Debug, Default)]
pub struct DeepgramMapper {
    turn: u64,
    /// Whether `final{turn}` has received any Stable text.
    has_stable: bool,
    /// Whether the last mapped message answered a client `Finalize`.
    answered_finalize: bool,
}

impl DeepgramMapper {
    pub fn new() -> Self {
        Self::default()
    }

    fn segment(&self, prefix: &str, update_kind: TranscriptUpdateKind) -> Option<SegmentRef> {
        Some(SegmentRef {
            segment_id: Some(format!("{prefix}{}", self.turn)),
            update_kind,
        })
    }

    /// Whether the message mapped last was the upstream answer to a client
    /// `Finalize`; reading it clears it.
    pub fn take_finalize_answer(&mut self) -> bool {
        std::mem::take(&mut self.answered_finalize)
    }

    pub fn map(&mut self, message: DeepgramMessage) -> Vec<StreamEvent> {
        self.answered_finalize = false;
        match message {
            DeepgramMessage::Result(result) => {
                self.answered_finalize = result.from_finalize;
                self.map_result(&result)
            }
            DeepgramMessage::UtteranceEnd => self.turn(false).into_iter().collect(),
            DeepgramMessage::FinalizeAck => {
                self.answered_finalize = true;
                self.turn(true).into_iter().collect()
            }
            DeepgramMessage::Error(error) => vec![error.into_event("deepgram")],
            DeepgramMessage::Ignored => Vec::new(),
        }
    }

    fn map_result(&mut self, result: &DeepgramResult) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if !result.transcript.is_empty() {
            if result.is_final {
                let text = if self.has_stable {
                    format!(" {}", result.transcript)
                } else {
                    result.transcript.clone()
                };
                events.push(StreamEvent::Transcript {
                    text,
                    stability: TranscriptStability::Committed,
                    segment: self.segment(FINAL_SEGMENT_PREFIX, TranscriptUpdateKind::Append),
                });
                self.has_stable = true;
            } else {
                events.push(StreamEvent::Transcript {
                    text: result.transcript.clone(),
                    stability: TranscriptStability::Provisional,
                    segment: self.segment(TAIL_SEGMENT_PREFIX, TranscriptUpdateKind::Snapshot),
                });
            }
        }
        if result.speech_final || result.from_finalize {
            events.extend(self.turn(result.from_finalize));
        }
        events
    }

    /// The Turn ending `final{turn}`, if it has Stable text; `always` answers
    /// a `Finalize` even when nothing was committed.
    fn turn(&mut self, always: bool) -> Option<StreamEvent> {
        if !self.has_stable && !always {
            return None;
        }
        let segment = self
            .has_stable
            .then(|| self.segment(FINAL_SEGMENT_PREFIX, TranscriptUpdateKind::Append))
            .flatten();
        if self.has_stable {
            self.turn += 1;
            self.has_stable = false;
        }
        Some(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment,
        }))
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
    let mut finalize = super::FinalizeWatch::default();
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
                    finalize.sent();
                }
                None | Some(SessionInput::Interrupt) => {
                    input_open = false;
                    let _ = transport
                        .send(WsFrame::Text(CLOSE_STREAM_FRAME.to_string()))
                        .await;
                }
                Some(_) => {}
            },
            () = finalize.expired(), if finalize.is_pending() => {
                if events.send(finalize.expire("deepgram")).await.is_err() {
                    return;
                }
            }
            maybe_frame = transport.recv() => match maybe_frame.as_ref().and_then(WsFrame::as_text) {
                Some(text) => match parse_message(text) {
                    Ok(message) => {
                        let is_error = matches!(message, DeepgramMessage::Error(_));
                        let mapped = mapper.map(message);
                        let mut mapped = finalize.settle(mapper.take_finalize_answer(), mapped);
                        if is_error {
                            mapped = finalize.upstream_failed(mapped);
                        }
                        for event in mapped {
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
            "{}?model={}&encoding=linear16&sample_rate=16000&interim_results=true&vad_events=true&utterance_end_ms=1000&endpointing=300&smart_format=true",
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
            Some(Some("final0"))
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
            Some(Some("final0"))
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
            } if segment_id.as_deref() == Some("tail0")
        ));
    }

    #[test]
    fn error_message_is_extracted() {
        let raw = json!({"type": "Error", "description": "bad audio"}).to_string();
        assert_eq!(
            parse_message(&raw).unwrap(),
            DeepgramMessage::Error(super::super::UpstreamError::new(None, "bad audio"))
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
            } if segment_id.as_deref() == Some("tail0")
        ));
    }

    fn transcript(event: &StreamEvent) -> (&str, TranscriptStability, Option<&str>) {
        match event {
            StreamEvent::Transcript {
                text,
                stability,
                segment,
            } => (
                text.as_str(),
                *stability,
                segment.as_ref().and_then(|s| s.segment_id.as_deref()),
            ),
            other => panic!("expected Transcript, got {other:?}"),
        }
    }

    #[test]
    fn interim_final_and_turn_follow_the_turn_segments() {
        let mut mapper = DeepgramMapper::new();
        let interim = mapper.map(result("do", false, false, 2.5));
        let r#final = mapper.map(result("done", true, true, 2.5));
        assert_eq!(
            transcript(&interim[0]),
            ("do", TranscriptStability::Provisional, Some("tail0"))
        );
        assert_eq!(
            transcript(&r#final[0]),
            ("done", TranscriptStability::Committed, Some("final0"))
        );
        assert_eq!(turn_segment(&r#final[1]), Some(Some("final0")));
        let next = mapper.map(result("again", false, false, 4.0));
        assert_eq!(transcript(&next[0]).2, Some("tail1"));
    }

    /// Deepgram's own endpointing example: one utterance in two `is_final`
    /// results with different `start`s, then `speech_final`. Both pieces
    /// append to the segment the single Turn ends.
    #[test]
    fn a_long_utterance_appends_every_final_to_one_turn() {
        let mut mapper = DeepgramMapper::new();
        let first = mapper.map(result(
            "yeah so my credit card number is two two",
            true,
            false,
            0.0,
        ));
        let second = mapper.map(result("two two three three three three", true, true, 3.26));
        assert_eq!(first.len(), 1);
        assert_eq!(
            transcript(&first[0]),
            (
                "yeah so my credit card number is two two",
                TranscriptStability::Committed,
                Some("final0")
            )
        );
        assert!(matches!(
            &first[0],
            StreamEvent::Transcript {
                segment: Some(SegmentRef {
                    update_kind: TranscriptUpdateKind::Append,
                    ..
                }),
                ..
            }
        ));
        assert_eq!(
            transcript(&second[0]),
            (
                " two two three three three three",
                TranscriptStability::Committed,
                Some("final0")
            )
        );
        assert_eq!(turn_segment(&second[1]), Some(Some("final0")));
    }

    #[test]
    fn utterance_end_is_a_turn_for_the_pending_stable() {
        let mut mapper = DeepgramMapper::new();
        assert_eq!(mapper.map(result("one", true, false, 0.0)).len(), 1);
        assert_eq!(mapper.map(result("two", true, false, 1.0)).len(), 1);
        let turn = mapper.map(DeepgramMessage::UtteranceEnd);
        assert_eq!(turn.len(), 1);
        assert_eq!(turn_segment(&turn[0]), Some(Some("final0")));
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
        assert_eq!(turn_segment(&with_text[1]), Some(Some("final0")));
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

    #[tokio::test(start_paused = true)]
    async fn an_unanswered_finalize_times_out_and_keeps_the_stream_open() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (_in_tx, in_rx) = mpsc::channel(8);
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
        crate::asr::assert_finalize_error(&events_rx.recv().await.unwrap(), ErrorCode::Timeout);
        input_tx
            .send(SessionInput::Audio(bytes::Bytes::from_static(b"pcm")))
            .await
            .unwrap();
        assert!(matches!(out_rx.recv().await.unwrap(), WsFrame::Binary(_)));
    }

    fn results_frame(transcript: &str, speech_final: bool, from_finalize: bool) -> WsFrame {
        WsFrame::Text(
            json!({
                "type": "Results",
                "is_final": true,
                "speech_final": speech_final,
                "from_finalize": from_finalize,
                "start": 0.0,
                "channel": {"alternatives": [{"transcript": transcript}]}
            })
            .to_string(),
        )
    }

    #[tokio::test(start_paused = true)]
    async fn a_vad_turn_does_not_answer_finalize() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(16);
        let _handle = tokio::spawn(run_deepgram_stream(transport, input_rx, events_tx));

        input_tx.send(SessionInput::Finalize).await.unwrap();
        out_rx.recv().await.unwrap();
        // an ordinary endpoint arrives first, then the real finalize answer
        in_tx
            .send(results_frame("hello world", true, false))
            .await
            .unwrap();
        in_tx
            .send(results_frame("how are you", false, true))
            .await
            .unwrap();
        let mut turns = Vec::new();
        for _ in 0..4 {
            let event = events_rx.recv().await.unwrap();
            if let Some(segment) = turn_segment(&event) {
                turns.push(segment.map(str::to_string));
            }
        }
        assert_eq!(
            turns,
            [Some("final0".to_string()), Some("final1".to_string())]
        );
        // the Finalize was answered by final1, so it never times out
        tokio::time::sleep(crate::asr::FINALIZE_TIMEOUT * 2).await;
        assert!(events_rx.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn a_late_answer_after_the_timeout_is_not_a_second_result() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(16);
        let _handle = tokio::spawn(run_deepgram_stream(transport, input_rx, events_tx));

        input_tx.send(SessionInput::Finalize).await.unwrap();
        out_rx.recv().await.unwrap();
        crate::asr::assert_finalize_error(&events_rx.recv().await.unwrap(), ErrorCode::Timeout);
        // a late, empty answer adds nothing
        in_tx
            .send(WsFrame::Text(json!({"from_finalize": true}).to_string()))
            .await
            .unwrap();
        // a late answer with text still ends its Stable, as an ordinary Turn
        in_tx.send(results_frame("hi", false, true)).await.unwrap();
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }
        ));
        assert_eq!(
            turn_segment(&events_rx.recv().await.unwrap()),
            Some(Some("final0"))
        );
    }

    #[test]
    fn the_untyped_finalize_ack_is_recognized_and_answers_finalize() {
        assert_eq!(
            parse_message(r#"{"from_finalize": true}"#).unwrap(),
            DeepgramMessage::FinalizeAck
        );
        let mut mapper = DeepgramMapper::new();
        let events = mapper.map(DeepgramMessage::FinalizeAck);
        assert!(mapper.take_finalize_answer());
        assert_eq!(turn_segment(&events[0]), Some(None));
        // a VAD Turn is not an answer
        mapper.map(result("hi", true, true, 0.0));
        assert!(!mapper.take_finalize_answer());
    }

    #[test]
    fn either_order_of_finalize_ack_and_results_gives_one_answer() {
        let ack = || DeepgramMessage::FinalizeAck;
        let run = |messages: Vec<DeepgramMessage>| {
            let mut mapper = DeepgramMapper::new();
            let mut watch = crate::asr::FinalizeWatch::default();
            watch.sent();
            let mut events = Vec::new();
            for message in messages {
                let mapped = mapper.map(message);
                events.extend(watch.settle(mapper.take_finalize_answer(), mapped));
            }
            assert!(!watch.is_pending(), "the Finalize was answered");
            events
                .iter()
                .filter_map(|event| turn_segment(event).map(|s| s.map(str::to_string)))
                .collect::<Vec<_>>()
        };
        // an endpoint Turn first, then the ack: the ack's segment-less Turn answers
        assert_eq!(
            run(vec![result("hello world", true, true, 0.0), ack()]),
            [Some("final0".to_string()), None]
        );
        // the ack first, then the flushed text: the ack answers, and the text
        // still gets its own Turn
        assert_eq!(
            run(vec![ack(), finalize_result("how are you", 1.5)]),
            [None, Some("final0".to_string())]
        );
        // the flushed text first, then the ack: one Turn, no empty second one
        assert_eq!(
            run(vec![finalize_result("how are you", 1.5), ack()]),
            [Some("final0".to_string())]
        );
    }
}
