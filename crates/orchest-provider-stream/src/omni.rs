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
//! This module owns: the server-event → unified `StreamEvent` mapping (ported
//! from the v0.9.11 `map_realtime_server_event`), the `RealtimeSession` surface +
//! an in-memory session for the ruler test, the live transport
//! ([`run_omni_session`]: handshake + duplex loop over [`crate::openspeech`], via
//! the Volcengine event-frame codec), and the wall factory. The factory is sync
//! but connect is async, so [`OmniSession::spawn_live`] spawns a connect-then-run
//! task and surfaces a connect failure as a fatal `Error` on `events()`.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;
use tokio::sync::mpsc;

use orchest_protocol::{
    AudioFormat, Capability, CapabilityDescriptor, CapabilitySource, ErrorCode, EventStream,
    Modality, ProtocolError, RealtimeSession, SessionInput, StreamEvent, TranscriptStability,
};

use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::ws::{connect_async, tungstenite};

use crate::transport::{ByteDuplex, WsDuplex, WsFrame};
use crate::tts::volcengine::{
    build_audio_frame, build_connect_frame, build_meta_frame, parse_frame, VolcengineFrame,
    EVENT_CONNECTION_STARTED, EVENT_FINISH_SESSION, EVENT_SESSION_FINISHED, EVENT_SESSION_STARTED,
    EVENT_START_CONNECTION, EVENT_START_SESSION, EVENT_TASK_REQUEST,
};

/// Default Volcengine openspeech realtime-dialogue endpoint.
const DEFAULT_OMNI_WS_URL: &str = "wss://openspeech.bytedance.com/api/v3/realtime/dialogue";

/// Client `FinishConnection` event (no TTS analog; the unidirectional path never
/// closes a live connection).
const EV_FINISH_CONNECTION: i32 = 2;
/// Client barge-in / interrupt event (omni-only).
const EV_CLIENT_INTERRUPT: i32 = 515;

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
            fatal: true,
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

// ---------------------------------------------------------------------------
// Live transport: handshake + full-duplex loop over the openspeech codec
// ---------------------------------------------------------------------------

/// Encode one [`ClientFrame`] onto an openspeech wire frame for `session_id`.
/// `Audio` is a raw `TaskRequest` audio frame; `Interrupt` a client-interrupt
/// meta frame; `Text` / `ToolResult` ride `TaskRequest` as a JSON meta payload.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn encode_client_frame(frame: ClientFrame, session_id: &str) -> Result<Vec<u8>, ProtocolError> {
    Ok(match frame {
        ClientFrame::Audio(bytes) => build_audio_frame(EVENT_TASK_REQUEST, session_id, &bytes),
        ClientFrame::Interrupt => {
            build_meta_frame(EV_CLIENT_INTERRUPT, session_id, &serde_json::json!({}))?
        }
        ClientFrame::Text(text) => build_meta_frame(
            EVENT_TASK_REQUEST,
            session_id,
            &serde_json::json!({ "text": text }),
        )?,
        ClientFrame::ToolResult {
            tool_use_id,
            content,
        } => build_meta_frame(
            EVENT_TASK_REQUEST,
            session_id,
            &serde_json::json!({ "tool_use_id": tool_use_id, "content": content }),
        )?,
    })
}

/// Project a decoded openspeech frame onto a unified event, plus whether it is a
/// terminal frame (`SessionFinished` / a fatal error).
fn map_volc_frame(frame: VolcengineFrame) -> (Option<StreamEvent>, bool) {
    match frame {
        VolcengineFrame::Audio { event, data, .. } => (
            map_server_event(event as u16, &Value::Null, Some(Bytes::from(data))),
            false,
        ),
        VolcengineFrame::Meta { event, payload, .. } => {
            // SESSION_FINISHED is a clean terminal; a session/connection failure
            // (mapped to a fatal Error above) is also terminal — don't linger in
            // the duplex loop waiting for a close a failure won't cleanly send.
            let terminal = event == EVENT_SESSION_FINISHED
                || event == EV_SESSION_ERROR_A as i32
                || event == EV_SESSION_ERROR_B as i32;
            (map_server_event(event as u16, &payload, None), terminal)
        }
        VolcengineFrame::Error { code, message } => (
            Some(StreamEvent::Error {
                error: ProtocolError::new(
                    ErrorCode::ProviderStreamError,
                    format!("omni provider error {code}: {message}"),
                ),
                fatal: true,
            }),
            true,
        ),
    }
}

/// Read frames until the `expected` lifecycle event arrives (`true`), forwarding
/// any stray content events meanwhile. Returns `false` on a terminal/error frame
/// or transport EOF before the ack.
async fn await_lifecycle<T: ByteDuplex>(
    transport: &mut T,
    expected: i32,
    events: &mpsc::Sender<StreamEvent>,
) -> bool {
    while let Some(frame) = transport.recv().await {
        let WsFrame::Binary(bytes) = frame else {
            continue;
        };
        let Ok(decoded) = parse_frame(&bytes) else {
            continue;
        };
        if let VolcengineFrame::Meta { event, .. } = &decoded {
            if *event == expected {
                return true;
            }
        }
        let (mapped, terminal) = map_volc_frame(decoded);
        if let Some(event) = mapped {
            let _ = events.send(event).await;
        }
        if terminal {
            return false;
        }
    }
    false
}

/// Drive one omni full-duplex session over `transport`: the openspeech handshake
/// (`StartConnection` → `ConnectionStarted` → `StartSession` → `SessionStarted`),
/// then a duplex loop — [`ClientFrame`]s in, server frames projected to
/// [`StreamEvent`]s out, ending on `SessionFinished` / a fatal error / EOF. Input
/// end flushes `FinishSession` + `FinishConnection`. Generic over [`ByteDuplex`]
/// so the whole session is testable without a network.
pub async fn run_omni_session<T: ByteDuplex>(
    mut transport: T,
    session_id: String,
    session_config: Value,
    mut input: mpsc::Receiver<ClientFrame>,
    events: mpsc::Sender<StreamEvent>,
) {
    let Ok(start_connection) = build_connect_frame(EVENT_START_CONNECTION, &serde_json::json!({}))
    else {
        return;
    };
    if transport
        .send(WsFrame::Binary(start_connection))
        .await
        .is_err()
        || !await_lifecycle(&mut transport, EVENT_CONNECTION_STARTED, &events).await
    {
        return;
    }
    let Ok(start_session) = build_meta_frame(EVENT_START_SESSION, &session_id, &session_config)
    else {
        return;
    };
    if transport
        .send(WsFrame::Binary(start_session))
        .await
        .is_err()
        || !await_lifecycle(&mut transport, EVENT_SESSION_STARTED, &events).await
    {
        return;
    }

    let mut input_open = true;
    loop {
        tokio::select! {
            command = input.recv(), if input_open => match command {
                Some(frame) => {
                    if let Ok(bytes) = encode_client_frame(frame, &session_id) {
                        if transport.send(WsFrame::Binary(bytes)).await.is_err() {
                            break;
                        }
                    }
                }
                None => {
                    input_open = false;
                    if let Ok(finish_session) =
                        build_meta_frame(EVENT_FINISH_SESSION, &session_id, &serde_json::json!({}))
                    {
                        let _ = transport.send(WsFrame::Binary(finish_session)).await;
                    }
                    if let Ok(finish_connection) =
                        build_connect_frame(EV_FINISH_CONNECTION, &serde_json::json!({}))
                    {
                        let _ = transport.send(WsFrame::Binary(finish_connection)).await;
                    }
                }
            },
            frame = transport.recv() => match frame {
                Some(WsFrame::Binary(bytes)) => {
                    if let Ok(decoded) = parse_frame(&bytes) {
                        let (mapped, terminal) = map_volc_frame(decoded);
                        if let Some(event) = mapped {
                            if events.send(event).await.is_err() {
                                return;
                            }
                        }
                        if terminal {
                            break;
                        }
                    }
                }
                Some(_) => {}
                None => break,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Live connect + the sync wall factory
// ---------------------------------------------------------------------------

/// Volcengine openspeech omni (realtime dialogue) configuration. Auth is the four
/// `X-Api-*` credentials; `start_session_payload` carries the asr/dialog/tts setup.
#[derive(Debug, Clone)]
pub struct OmniConfig {
    pub model: String,
    pub speaker: String,
    pub ws_url: String,
    pub app_id: String,
    pub access_key: String,
    pub resource_id: String,
    pub app_key: String,
}

impl OmniConfig {
    /// The `StartSession` payload (asr input / dialog bot / tts output blocks).
    fn start_session_payload(&self) -> Value {
        serde_json::json!({
            "asr": { "audio_info": { "format": "pcm_s16le", "sample_rate": 16000, "channel": 1 } },
            "dialog": {
                "bot_name": "Doubao",
                "dialog_id": "",
                "extra": { "input_mod": "audio_file", "model": self.model, "strict_audit": true }
            },
            "tts": {
                "speaker": self.speaker,
                "audio_config": { "channel": 1, "format": "pcm_s16le", "sample_rate": 24000 }
            }
        })
    }
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split(['/', '?']).next())
        .unwrap_or("openspeech.bytedance.com")
}

/// Connect the omni WebSocket with the four `X-Api-*` headers.
async fn connect_omni(config: &OmniConfig) -> Result<impl ByteDuplex, ProtocolError> {
    let ws_request = tungstenite::http::Request::builder()
        .uri(&config.ws_url)
        .header("X-Api-App-ID", &config.app_id)
        .header("X-Api-Access-Key", &config.access_key)
        .header("X-Api-Resource-Id", &config.resource_id)
        .header("X-Api-App-Key", &config.app_key)
        .header("X-Api-Connect-Id", uuid::Uuid::new_v4().to_string())
        .header("Host", host_of(&config.ws_url))
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
            format!("Volcengine omni WebSocket connection failed: {e}"),
        )
    })?;
    Ok(WsDuplex::new(ws_stream))
}

impl OmniSession {
    /// Build a **live** session. The wall factory is sync but connect is async, so
    /// this spawns a connect-then-run task and returns immediately; a connect
    /// failure surfaces as a fatal `Error` on `events()` (the `RealtimeSession`
    /// surface has no async constructor to defer to). The command channel buffers
    /// any `send` issued before the handshake completes.
    pub fn spawn_live(config: OmniConfig) -> Self {
        let session_id = uuid::Uuid::new_v4().simple().to_string();
        let (commands_tx, commands_rx) = mpsc::channel(32);
        let (events_tx, events) = EventStream::channel(64);
        let task_session_id = session_id.clone();
        tokio::spawn(async move {
            match connect_omni(&config).await {
                Ok(transport) => {
                    run_omni_session(
                        transport,
                        task_session_id,
                        config.start_session_payload(),
                        commands_rx,
                        events_tx,
                    )
                    .await;
                }
                Err(error) => {
                    let _ = events_tx
                        .send(StreamEvent::Error { error, fatal: true })
                        .await;
                }
            }
        });
        Self {
            session_id,
            commands: commands_tx,
            events,
            closed: AtomicBool::new(false),
        }
    }
}

/// The static descriptor the registry filters on for the omni dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    omni_descriptor("volcengine", "1.2.1.1")
}

/// Build a live omni [`OmniSession`] from a registry [`ProviderConfig`]: the
/// access key is `api_key`; `app_id` / `app_key` / `resource_id` / `speaker` ride
/// `options` (with the Volcengine defaults).
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<OmniSession, ProtocolError> {
    let access_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(
            ErrorCode::MissingApiKey,
            "volcengine omni requires api_key (the access key)",
        )
    })?;
    let opt = |key: &str| {
        cfg.options
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let app_id = opt("app_id").ok_or_else(|| {
        ProtocolError::new(
            ErrorCode::InvalidRequest,
            "volcengine omni requires options.app_id",
        )
    })?;
    let config = OmniConfig {
        model: if cfg.model.is_empty() {
            "1.2.1.1".to_string()
        } else {
            cfg.model.clone()
        },
        speaker: opt("speaker").unwrap_or_else(|| "zh_female_vv_jupiter_bigtts".to_string()),
        ws_url: cfg
            .api_url
            .clone()
            .unwrap_or_else(|| DEFAULT_OMNI_WS_URL.to_string()),
        app_id,
        access_key,
        resource_id: opt("resource_id").unwrap_or_else(|| "volc.speech.dialog".to_string()),
        app_key: opt("app_key").unwrap_or_else(|| "PlgvMymc7f3tQnJ6".to_string()),
    };
    Ok(OmniSession::spawn_live(config))
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

    /// A crossed pair of in-memory duplexes: what one sends, the other receives.
    fn duplex_pair() -> (ChannelDuplex, ChannelDuplex) {
        let (a_tx, a_rx) = mpsc::channel(32);
        let (b_tx, b_rx) = mpsc::channel(32);
        (
            ChannelDuplex {
                out: a_tx,
                inbound: b_rx,
            },
            ChannelDuplex {
                out: b_tx,
                inbound: a_rx,
            },
        )
    }

    /// Build a server→client audio (`MSG_AUDIO_ONLY_RESPONSE`) openspeech frame.
    fn server_audio_frame(event: i32, session: &str, audio: &[u8]) -> Vec<u8> {
        use crate::openspeech::{
            build_header, COMP_NONE, FLAG_WITH_EVENT, MSG_AUDIO_ONLY_RESPONSE, SER_NONE,
        };
        let session = session.as_bytes();
        let mut out = build_header(
            MSG_AUDIO_ONLY_RESPONSE,
            FLAG_WITH_EVENT,
            SER_NONE,
            COMP_NONE,
        )
        .to_vec();
        out.extend_from_slice(&event.to_be_bytes());
        out.extend_from_slice(&(session.len() as u32).to_be_bytes());
        out.extend_from_slice(session);
        out.extend_from_slice(&(audio.len() as u32).to_be_bytes());
        out.extend_from_slice(audio);
        out
    }

    /// Build a server→client meta (`MSG_FULL_SERVER_RESPONSE`) openspeech frame —
    /// what `parse_frame` decodes (the client builders produce request frames).
    fn server_meta_frame(event: i32, session: &str, payload: &serde_json::Value) -> Vec<u8> {
        use crate::openspeech::{
            build_header, COMP_NONE, FLAG_WITH_EVENT, MSG_FULL_SERVER_RESPONSE, SER_JSON,
        };
        let body = serde_json::to_vec(payload).unwrap();
        let session = session.as_bytes();
        let mut out = build_header(
            MSG_FULL_SERVER_RESPONSE,
            FLAG_WITH_EVENT,
            SER_JSON,
            COMP_NONE,
        )
        .to_vec();
        out.extend_from_slice(&event.to_be_bytes());
        out.extend_from_slice(&(session.len() as u32).to_be_bytes());
        out.extend_from_slice(session);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// Assert the next client frame the server receives carries `expected` event.
    async fn expect_client_event(server: &mut ChannelDuplex, expected: i32) {
        match server.recv().await.expect("a client frame") {
            WsFrame::Binary(bytes) => {
                let event = i32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                assert_eq!(event, expected, "unexpected client event");
            }
            other => panic!("expected a binary frame, got {other:?}"),
        }
    }

    /// The live session over an in-memory openspeech peer: the handshake completes,
    /// server audio + model text project to unified events, a mic frame is encoded
    /// as a `TaskRequest`, and `SessionFinished` ends the stream.
    #[tokio::test]
    async fn run_omni_session_handshakes_then_streams_duplex() {
        let (client, mut server) = duplex_pair();
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let handle = tokio::spawn(run_omni_session(
            client,
            "sess".to_string(),
            json!({}),
            input_rx,
            events_tx,
        ));

        // handshake: StartConnection -> ConnectionStarted, StartSession -> SessionStarted
        expect_client_event(&mut server, EVENT_START_CONNECTION).await;
        server
            .send(WsFrame::Binary(server_meta_frame(
                EVENT_CONNECTION_STARTED,
                "sess",
                &json!({}),
            )))
            .await
            .unwrap();
        expect_client_event(&mut server, EVENT_START_SESSION).await;
        server
            .send(WsFrame::Binary(server_meta_frame(
                EVENT_SESSION_STARTED,
                "sess",
                &json!({}),
            )))
            .await
            .unwrap();

        // server emits an audio chunk (352) and a model-text delta (550)
        server
            .send(WsFrame::Binary(server_audio_frame(
                EV_AUDIO_OUTPUT as i32,
                "sess",
                b"aud",
            )))
            .await
            .unwrap();
        server
            .send(WsFrame::Binary(server_meta_frame(
                EV_MODEL_TEXT as i32,
                "sess",
                &json!({"content": "hi"}),
            )))
            .await
            .unwrap();

        // a mic frame in is encoded as a TaskRequest(200) audio frame
        input_tx
            .send(ClientFrame::Audio(Bytes::from_static(b"mic")))
            .await
            .unwrap();
        expect_client_event(&mut server, EVENT_TASK_REQUEST).await;

        // server finishes the session -> the event stream terminates
        server
            .send(WsFrame::Binary(server_meta_frame(
                EVENT_SESSION_FINISHED,
                "sess",
                &json!({}),
            )))
            .await
            .unwrap();

        let mut audio = Vec::new();
        let mut text = String::new();
        while let Some(event) = events_rx.recv().await {
            match event {
                StreamEvent::AudioDelta { data, .. } => audio.push(data),
                StreamEvent::Text { delta } => text.push_str(&delta),
                _ => {}
            }
        }
        assert_eq!(audio, vec![Bytes::from_static(b"aud")]);
        assert_eq!(text, "hi");
        handle.await.unwrap();
    }

    #[test]
    fn session_failure_events_map_to_fatal_error() {
        for id in [EV_SESSION_ERROR_A, EV_SESSION_ERROR_B] {
            assert!(
                matches!(
                    map_server_event(id, &json!({ "error": "boom" }), None),
                    Some(StreamEvent::Error { fatal: true, .. })
                ),
                "event {id} must map to a fatal error",
            );
        }
    }

    /// A session-failure meta frame (event 153) mid-call must surface a *fatal*
    /// error and *terminate* the duplex loop — not linger (the old fatal:false,
    /// non-terminal behavior) until the server drops the socket.
    #[tokio::test]
    async fn run_omni_session_terminates_on_session_failure() {
        let (client, mut server) = duplex_pair();
        let (_input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let handle = tokio::spawn(run_omni_session(
            client,
            "sess".to_string(),
            json!({}),
            input_rx,
            events_tx,
        ));

        // handshake
        expect_client_event(&mut server, EVENT_START_CONNECTION).await;
        server
            .send(WsFrame::Binary(server_meta_frame(
                EVENT_CONNECTION_STARTED,
                "sess",
                &json!({}),
            )))
            .await
            .unwrap();
        expect_client_event(&mut server, EVENT_START_SESSION).await;
        server
            .send(WsFrame::Binary(server_meta_frame(
                EVENT_SESSION_STARTED,
                "sess",
                &json!({}),
            )))
            .await
            .unwrap();

        // The server reports a session failure (event 153).
        server
            .send(WsFrame::Binary(server_meta_frame(
                EV_SESSION_ERROR_B as i32,
                "sess",
                &json!({ "error": "boom" }),
            )))
            .await
            .unwrap();

        let mut saw_fatal = false;
        while let Some(event) = events_rx.recv().await {
            if let StreamEvent::Error { fatal, .. } = event {
                saw_fatal = fatal;
            }
        }
        assert!(saw_fatal, "session failure must surface a fatal error");
        handle.await.unwrap(); // the run loop terminated (did not hang)
    }
}
