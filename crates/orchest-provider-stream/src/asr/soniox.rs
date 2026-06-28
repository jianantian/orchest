//! Soniox streaming-ASR token codec (Issue 006). Ported from
//! `agent-runtime-asr-providers`'s `providers/soniox/mod.rs` parsing, over the
//! spine [`ProtocolError`] / [`StreamEvent`].
//!
//! Soniox streams **tokens**: each carries `text` and an `is_final` flag, so one
//! message mixes a committed prefix (final tokens) and a revisable tail
//! (non-final tokens). `finished` marks end of stream. Audio out is binary; this
//! module owns the inbound JSON → unified-event projection (the live `Asr` impl
//! wraps it once the transport carries both binary audio and text frames).

use async_trait::async_trait;
use orchest_protocol::{
    Asr, Capability, CapabilityDescriptor, ErrorCode, EventStream, Language, LifecycleEvent,
    Modality, ProtocolError, RealtimeHandle, SessionInput, StreamEvent, StreamingTranscribeRequest,
    TranscribeRequest, TranscribeResult, TranscriptStability,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::ws::{connect_async, tungstenite};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::transport::{ByteDuplex, WsDuplex, WsFrame};

const DEFAULT_WS_URL: &str = "wss://stt-rt.soniox.com/transcribe-websocket";

/// A decoded Soniox inbound message.
#[derive(Debug, PartialEq)]
pub enum SonioxMessage {
    Tokens {
        /// Concatenated `is_final` tokens (committed).
        final_text: String,
        /// Concatenated non-final tokens (revisable).
        provisional_text: String,
        /// End of stream.
        finished: bool,
    },
    Error(String),
    /// No tokens and not finished — nothing to surface.
    Ignored,
}

/// Parse one Soniox text message into [`SonioxMessage`]. An `error_code` yields
/// `Error`; a message with no tokens that is not `finished` is `Ignored`.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_message(text: &str) -> Result<SonioxMessage, ProtocolError> {
    let message: RawMessage = serde_json::from_str(text).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("failed to parse Soniox message: {e}"),
        )
    })?;
    if let Some(error_code) = message.error_code {
        return Ok(SonioxMessage::Error(format!(
            "soniox error {error_code}: {}",
            message
                .error_message
                .unwrap_or_else(|| "stream error".into())
        )));
    }
    if message.tokens.is_empty() && !message.finished {
        return Ok(SonioxMessage::Ignored);
    }
    let mut final_text = String::new();
    let mut provisional_text = String::new();
    for token in message.tokens {
        if token.is_final {
            final_text.push_str(&token.text);
        } else {
            provisional_text.push_str(&token.text);
        }
    }
    Ok(SonioxMessage::Tokens {
        final_text,
        provisional_text,
        finished: message.finished,
    })
}

/// Project a Soniox message onto unified events: committed tokens → a
/// `Committed` `Transcript`, the revisable tail → a `Provisional` `Transcript`,
/// and `finished` → `EndOfSpeech`. An `Error` becomes a single fatal `Error`.
pub fn map_message(message: SonioxMessage) -> Vec<StreamEvent> {
    match message {
        SonioxMessage::Tokens {
            final_text,
            provisional_text,
            finished,
        } => {
            let mut events = Vec::new();
            if !final_text.is_empty() {
                events.push(StreamEvent::Transcript {
                    text: final_text,
                    stability: TranscriptStability::Committed,
                    segment: None,
                });
            }
            if !provisional_text.is_empty() {
                events.push(StreamEvent::Transcript {
                    text: provisional_text,
                    stability: TranscriptStability::Provisional,
                    segment: None,
                });
            }
            if finished {
                events.push(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
                    segment: None,
                }));
            }
            events
        }
        SonioxMessage::Error(message) => vec![StreamEvent::Error {
            error: ProtocolError::new(ErrorCode::ProviderTaskFailed, message),
            fatal: true,
        }],
        SonioxMessage::Ignored => Vec::new(),
    }
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    #[serde(default)]
    tokens: Vec<Token>,
    #[serde(default)]
    finished: bool,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Token {
    text: String,
    #[serde(default)]
    is_final: bool,
}

// ---------------------------------------------------------------------------
// Streaming loop + the spine `Asr` impl
// ---------------------------------------------------------------------------

/// Soniox's first frame is a text config carrying the api_key/model; audio then
/// streams as binary. Build that config frame, merging the request's `options`.
fn build_config_frame(api_key: &str, model: &str, options: &Value) -> String {
    let mut value = serde_json::json!({
        "api_key": api_key,
        "model": model,
        "audio_format": "pcm_s16le",
        "sample_rate": 16000,
        "num_channels": 1,
    });
    if let Some(overrides) = options.as_object() {
        for (key, v) in overrides {
            value[key] = v.clone();
        }
    }
    value.to_string()
}

/// Drive one soniox streaming session over `transport`: send the `config_frame`
/// (text) first, then client audio as **binary**; an empty binary frame signals
/// end of audio. Inbound **text** token messages project via [`map_message`].
/// Ends on `finished`, a fatal error, or transport EOF.
pub async fn run_soniox_stream<T: ByteDuplex>(
    mut transport: T,
    config_frame: String,
    mut input: mpsc::Receiver<SessionInput>,
    events: mpsc::Sender<StreamEvent>,
) {
    if transport.send(WsFrame::Text(config_frame)).await.is_err() {
        return;
    }
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
                    let _ = transport.send(WsFrame::Binary(Vec::new())).await; // end-of-audio
                }
                Some(_) => {}
            },
            maybe_frame = transport.recv() => match maybe_frame.as_ref().and_then(WsFrame::as_text) {
                Some(text) => match parse_message(text) {
                    Ok(message) => {
                        let terminal = matches!(
                            &message,
                            SonioxMessage::Tokens { finished: true, .. } | SonioxMessage::Error(_)
                        );
                        for event in map_message(message) {
                            if events.send(event).await.is_err() {
                                return;
                            }
                        }
                        if terminal {
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
                    None => break,
                },
            },
        }
    }
}

/// Soniox streaming-ASR configuration.
#[derive(Debug, Clone)]
pub struct SonioxAsrConfig {
    pub model: String,
    pub ws_url: String,
    pub api_key: String,
}

/// The soniox streaming ASR provider as the spine [`Asr`].
pub struct SonioxAsr {
    config: SonioxAsrConfig,
}

impl SonioxAsr {
    pub fn new(config: SonioxAsrConfig) -> Self {
        Self { config }
    }
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split(['/', '?']).next())
        .unwrap_or("stt-rt.soniox.com")
}

/// The static descriptor the registry filters on for the soniox dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("soniox", "stt-rt-v5", Capability::Asr)
        .streaming(true)
        .duplex(true)
        .with_input_modalities([Modality::Audio])
        .with_output_modalities([Modality::Text])
}

/// Build a [`SonioxAsr`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<SonioxAsr, ProtocolError> {
    let api_key = cfg
        .api_key
        .clone()
        .ok_or_else(|| ProtocolError::new(ErrorCode::MissingApiKey, "soniox requires api_key"))?;
    let model = if cfg.model.is_empty() {
        "stt-rt-v5".to_string()
    } else {
        cfg.model.clone()
    };
    let ws_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_WS_URL.to_string());
    Ok(SonioxAsr::new(SonioxAsrConfig {
        model,
        ws_url,
        api_key,
    }))
}

#[async_trait]
impl Asr for SonioxAsr {
    fn provider_name(&self) -> &str {
        "soniox"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("soniox", self.config.model.clone(), Capability::Asr)
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
            "soniox ASR is streaming-only; use start_stream",
        ))
    }

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError> {
        if !self.config.ws_url.starts_with("wss://") {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "WebSocket URL must use wss:// for secure credential transport",
            ));
        }
        let config_frame =
            build_config_frame(&self.config.api_key, &self.config.model, &request.options);
        let ws_request = tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
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
                format!("Soniox WebSocket connection failed: {e}"),
            )
        })?;

        let (input_tx, input_rx) = mpsc::channel(32);
        let (events_tx, events) = EventStream::channel(64);
        tokio::spawn(run_soniox_stream(
            WsDuplex::new(ws_stream),
            config_frame,
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

    fn tokens_msg(tokens: &[(&str, bool)], finished: bool) -> String {
        let tokens: Vec<_> = tokens
            .iter()
            .map(|(t, f)| json!({"text": t, "is_final": f}))
            .collect();
        json!({"tokens": tokens, "finished": finished}).to_string()
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
    async fn run_soniox_stream_sends_config_audio_and_finishes() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let config = build_config_frame("k", "stt-rt-v5", &json!({}));
        let handle = tokio::spawn(run_soniox_stream(transport, config, input_rx, events_tx));

        // first frame is the text config carrying the api_key
        match out_rx.recv().await.unwrap() {
            WsFrame::Text(t) => assert!(t.contains("api_key")),
            other => panic!("expected config text frame, got {other:?}"),
        }

        // audio in -> binary
        input_tx
            .send(SessionInput::Audio(bytes::Bytes::from_static(b"pcm")))
            .await
            .unwrap();
        assert!(matches!(out_rx.recv().await.unwrap(), WsFrame::Binary(_)));

        // a finished token message -> Committed transcript + EndOfSpeech, then ends
        in_tx
            .send(WsFrame::Text(tokens_msg(&[("done", true)], true)))
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
        handle.await.unwrap(); // terminated on `finished`
    }

    #[test]
    fn parses_mixed_final_and_provisional_tokens() {
        let raw = tokens_msg(&[("hello ", true), ("wor", false)], false);
        assert_eq!(
            parse_message(&raw).unwrap(),
            SonioxMessage::Tokens {
                final_text: "hello ".into(),
                provisional_text: "wor".into(),
                finished: false,
            }
        );
    }

    #[test]
    fn empty_unfinished_message_is_ignored() {
        assert_eq!(
            parse_message(&json!({"tokens": []}).to_string()).unwrap(),
            SonioxMessage::Ignored
        );
    }

    #[test]
    fn error_code_becomes_error() {
        let raw = json!({"error_code": "401", "error_message": "bad key"}).to_string();
        match parse_message(&raw).unwrap() {
            SonioxMessage::Error(m) => assert!(m.contains("401") && m.contains("bad key")),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn map_emits_committed_provisional_and_end_of_speech() {
        let events = map_message(SonioxMessage::Tokens {
            final_text: "hello".into(),
            provisional_text: "world".into(),
            finished: true,
        });
        assert!(matches!(
            events[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }
        ));
        assert!(matches!(
            events[1],
            StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                ..
            }
        ));
        assert!(matches!(
            events[2],
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { .. })
        ));
    }

    #[test]
    fn map_error_is_fatal() {
        let events = map_message(SonioxMessage::Error("boom".into()));
        assert!(matches!(events[0], StreamEvent::Error { fatal: true, .. }));
    }
}
