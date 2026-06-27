//! Volcengine openspeech **omni** full-duplex dialogue as the spine
//! [`RealtimeSession`] (Issue 006, absorbing `agent-runtime-realtime-providers`).
//!
//! The omni session interleaves microphone audio in, and audio + transcript +
//! model-text out, with mid-stream tool use — all over one openspeech WS
//! connection. The spine shape splits this into a **send side** ([`SessionInput`]
//! → one command channel) and a **pulled receive side** ([`EventStream`] of
//! unified [`StreamEvent`]s). Keeping the two on independent channels is what
//! makes a mid-stream `ToolResult` never block outgoing audio — the omni
//! acceptance ruler (design §6.1).
//!
//! This module owns the network-independent core: the server-event → unified
//! `StreamEvent` mapping (ported from the v0.9.11 `map_realtime_server_event`),
//! the `RealtimeSession` surface, and an in-memory session for the ruler test.
//! The live WS transport (handshake + frame loop over [`crate::openspeech`]) is
//! layered on in a later slice; the factory wiring waits on the async-connect
//! question and is not registered through the wall yet.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;
use tokio::sync::mpsc;

use orchest_protocol::{
    AudioFormat, Capability, CapabilityDescriptor, CapabilitySource, ErrorCode, EventStream,
    Modality, ProtocolError, RealtimeSession, SessionInput, StreamEvent, TranscriptStability,
};

// openspeech omni server event ids (the v0.9.11 evidence set).
const EV_AUDIO_OUTPUT: u16 = 352;
const EV_ASR_RESPONSE: u16 = 451;
const EV_MODEL_TEXT: u16 = 550;
const EV_SESSION_ERROR_A: u16 = 51;
const EV_SESSION_ERROR_B: u16 = 153;

/// A client→server omni frame, produced from [`SessionInput`]. The live transport
/// serializes these onto openspeech wire frames; the in-memory session exposes
/// them for assertions.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientFrame {
    Audio(Bytes),
    Text(String),
    ToolResult { tool_use_id: String, content: Value },
    Interrupt,
}

impl From<SessionInput> for ClientFrame {
    fn from(input: SessionInput) -> Self {
        match input {
            SessionInput::Audio(data) => ClientFrame::Audio(data),
            SessionInput::Text(text) => ClientFrame::Text(text),
            SessionInput::ToolResult {
                tool_use_id,
                content,
            } => ClientFrame::ToolResult {
                tool_use_id,
                content,
            },
            SessionInput::Interrupt => ClientFrame::Interrupt,
        }
    }
}

/// Map one openspeech omni server event to a unified [`StreamEvent`]. Returns
/// `None` for pure lifecycle/metadata frames that carry no content (the session
/// loop may still act on those). Audio output is `pcm_s16le` @ 24 kHz.
pub fn map_server_event(
    event_id: u16,
    payload: &Value,
    audio: Option<Bytes>,
) -> Option<StreamEvent> {
    match event_id {
        EV_AUDIO_OUTPUT => Some(StreamEvent::AudioDelta {
            data: audio.unwrap_or_default(),
            format: AudioFormat::Pcm16Le,
            sequence: 0,
        }),
        EV_ASR_RESPONSE => {
            let first = payload
                .get("results")
                .and_then(Value::as_array)
                .and_then(|results| results.first());
            let text = first
                .and_then(|r| r.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let is_interim = first
                .and_then(|r| r.get("is_interim"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(StreamEvent::Transcript {
                text,
                stability: if is_interim {
                    TranscriptStability::Provisional
                } else {
                    TranscriptStability::Committed
                },
                segment: None,
            })
        }
        EV_MODEL_TEXT => Some(StreamEvent::Text {
            delta: payload
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        EV_SESSION_ERROR_A | EV_SESSION_ERROR_B => Some(StreamEvent::Error {
            error: ProtocolError::new(
                ErrorCode::ProviderStreamError,
                payload
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("omni provider error")
                    .to_string(),
            ),
            fatal: false,
        }),
        _ => None,
    }
}

/// The queryable descriptor for the omni capability: bidirectional, interruptible,
/// audio in / audio out.
pub fn omni_descriptor(
    provider: impl Into<std::borrow::Cow<'static, str>>,
    model: impl Into<std::borrow::Cow<'static, str>>,
) -> CapabilityDescriptor {
    CapabilityDescriptor::new(provider, model, Capability::Realtime)
        .streaming(true)
        .duplex(true)
        .interruptible(true)
        .with_input_modalities([Modality::Audio, Modality::Text])
        .with_output_modalities([Modality::Audio, Modality::Text])
        .with_source(CapabilitySource::Static)
}

/// An omni full-duplex session. `send` feeds the one command channel (never
/// blocks on the reader); `events` is pulled concurrently.
pub struct OmniSession {
    session_id: String,
    commands: mpsc::Sender<ClientFrame>,
    events: EventStream,
    closed: AtomicBool,
}

impl OmniSession {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Build a network-independent session plus a server handle that injects
    /// server events and observes client frames — the substrate of the ruler.
    pub fn in_memory(session_id: impl Into<String>) -> (Self, OmniServerHandle) {
        let (events_tx, events) = EventStream::channel(64);
        let (commands_tx, commands_rx) = mpsc::channel(64);
        (
            Self {
                session_id: session_id.into(),
                commands: commands_tx,
                events,
                closed: AtomicBool::new(false),
            },
            OmniServerHandle {
                events_tx: Some(events_tx),
                commands_rx,
            },
        )
    }
}

#[async_trait]
impl RealtimeSession for OmniSession {
    async fn send(&self, input: SessionInput) -> Result<(), ProtocolError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "omni session is closed",
            ));
        }
        self.commands.send(input.into()).await.map_err(|_| {
            ProtocolError::new(ErrorCode::ProviderStreamError, "omni command loop closed")
        })
    }

    fn events(&mut self) -> &mut EventStream {
        &mut self.events
    }

    async fn close(&mut self) -> Result<(), ProtocolError> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// Test/driver handle paired with an in-memory [`OmniSession`]: injects mapped
/// server events into the session's event stream and observes client frames.
pub struct OmniServerHandle {
    events_tx: Option<mpsc::Sender<StreamEvent>>,
    commands_rx: mpsc::Receiver<ClientFrame>,
}

impl OmniServerHandle {
    /// Map a raw openspeech server event and push it onto the session's stream.
    pub async fn emit(&self, event_id: u16, payload: &Value, audio: Option<Bytes>) {
        if let (Some(tx), Some(event)) =
            (&self.events_tx, map_server_event(event_id, payload, audio))
        {
            let _ = tx.send(event).await;
        }
    }

    /// Drop the event sender so the session's `events()` stream terminates.
    pub fn close_events(&mut self) {
        self.events_tx = None;
    }

    /// Observe the next client frame the session sent, if any.
    pub async fn next_client_frame(&mut self) -> Option<ClientFrame> {
        self.commands_rx.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_audio_transcript_and_model_text() {
        let audio = map_server_event(
            EV_AUDIO_OUTPUT,
            &json!({}),
            Some(Bytes::from_static(b"pcm")),
        );
        assert!(matches!(
            audio,
            Some(StreamEvent::AudioDelta {
                format: AudioFormat::Pcm16Le,
                ..
            })
        ));

        let transcript = map_server_event(
            EV_ASR_RESPONSE,
            &json!({"results": [{"text": "hi", "is_interim": true}]}),
            None,
        );
        assert!(matches!(
            transcript,
            Some(StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                ..
            })
        ));

        let text = map_server_event(EV_MODEL_TEXT, &json!({"content": "world"}), None);
        assert_eq!(
            text,
            Some(StreamEvent::Text {
                delta: "world".into()
            })
        );

        // Pure lifecycle ids carry no content event.
        assert_eq!(map_server_event(150, &json!({}), None), None);
    }

    /// The omni acceptance ruler against a fake session: audio in, audio + text
    /// out, a mid-stream tool result — and audio output that arrives *after* the
    /// tool use, proving the tool result never blocked the audio stream.
    #[tokio::test]
    async fn omni_ruler_audio_never_blocks_across_tool_use() {
        let (mut session, mut server) = OmniSession::in_memory("sess-1");

        // audio out + interim transcript before the tool turn
        server
            .emit(
                EV_AUDIO_OUTPUT,
                &json!({}),
                Some(Bytes::from_static(b"aud0")),
            )
            .await;
        server
            .emit(
                EV_ASR_RESPONSE,
                &json!({"results": [{"text": "hello", "is_interim": true}]}),
                None,
            )
            .await;

        // audio in, then a mid-stream tool result — both must return immediately
        session
            .send(SessionInput::Audio(Bytes::from_static(b"mic")))
            .await
            .unwrap();
        session
            .send(SessionInput::ToolResult {
                tool_use_id: "t1".into(),
                content: json!({"ok": true}),
            })
            .await
            .unwrap();

        // audio + model text continue AFTER the tool use
        server
            .emit(
                EV_AUDIO_OUTPUT,
                &json!({}),
                Some(Bytes::from_static(b"aud1")),
            )
            .await;
        server
            .emit(EV_MODEL_TEXT, &json!({"content": "world"}), None)
            .await;
        server.close_events(); // end the event stream so the reader terminates

        let mut audio = Vec::new();
        let mut text = String::new();
        let mut transcript = String::new();
        while let Some(ev) = session.events().next().await {
            match ev {
                StreamEvent::AudioDelta { data, .. } => audio.push(data),
                StreamEvent::Text { delta } => text.push_str(&delta),
                StreamEvent::Transcript { text: t, .. } => transcript = t,
                _ => {}
            }
        }
        drop(session); // release the command sender so client frames terminate

        // audio out arrived both before and after the mid-stream tool use
        assert_eq!(audio.len(), 2, "audio must flow across the tool turn");
        assert_eq!(audio[0], Bytes::from_static(b"aud0"));
        assert_eq!(audio[1], Bytes::from_static(b"aud1"));
        assert_eq!(text, "world");
        assert_eq!(transcript, "hello");

        // the session forwarded the audio-in and the tool result as client frames
        let mut frames = Vec::new();
        while let Some(f) = server.next_client_frame().await {
            frames.push(f);
        }
        assert_eq!(frames.len(), 2);
        assert!(matches!(frames[0], ClientFrame::Audio(_)));
        assert!(matches!(frames[1], ClientFrame::ToolResult { .. }));
    }
}
