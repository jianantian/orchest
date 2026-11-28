//! Shared TTS request selection and pull-based SDK stream handling.

use bytes::Bytes;
use orchest_protocol::{
    ErrorCode, EventStream, LifecycleEvent, ProtocolError, StreamEvent, TtsInput, VoiceUsage,
};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

/// A pulled payload keeps the audio buffer outside the JSON metadata.
pub struct TtsEvent {
    pub metadata: Value,
    pub audio: Option<Bytes>,
}

pub struct TtsStream {
    read: Mutex<ReadState>,
    input: Mutex<Option<mpsc::Sender<TtsInput>>>,
    duplex: bool,
    stop: CancellationToken,
}

struct ReadState {
    events: Option<EventStream>,
    usage: Option<VoiceUsage>,
}

impl TtsStream {
    pub fn new(events: EventStream, input: Option<mpsc::Sender<TtsInput>>) -> Self {
        Self {
            read: Mutex::new(ReadState {
                events: Some(events),
                usage: None,
            }),
            duplex: input.is_some(),
            input: Mutex::new(input),
            stop: CancellationToken::new(),
        }
    }

    pub async fn next(&self) -> Result<Option<TtsEvent>, ProtocolError> {
        let mut read = self.read.lock().await;
        let Some(events) = read.events.as_mut() else {
            return Ok(None);
        };
        loop {
            let event = tokio::select! {
                biased;
                () = self.stop.cancelled() => {
                    read.events.take();
                    return Ok(None);
                }
                event = events.next() => event,
            };
            match event {
                None => {
                    read.usage = events.take_voice_usage().await;
                    read.events.take();
                    return Ok(None);
                }
                Some(StreamEvent::Error { error, fatal: true }) => {
                    // Built-in TTS drivers report fatal errors at termination.
                    // Keep the terminal usage available after the exception.
                    read.usage = tokio::select! {
                        () = self.stop.cancelled() => None,
                        usage = events.take_voice_usage() => usage,
                    };
                    read.events.take();
                    return Err(error);
                }
                Some(event) => {
                    if let Some(event) = project_event(event) {
                        return Ok(Some(event));
                    }
                }
            }
        }
    }

    pub async fn command(&self, action: &str, text: Option<String>) -> Result<(), ProtocolError> {
        let command = match (action, text) {
            ("append", Some(text)) => TtsInput::Append(text),
            ("flush", None) => TtsInput::Flush,
            ("cancel", None) => TtsInput::Cancel,
            ("close", None) => TtsInput::Close,
            _ => {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidRequest,
                    "invalid TTS command",
                ))
            }
        };
        self.send(command).await
    }

    async fn send(&self, command: TtsInput) -> Result<(), ProtocolError> {
        if !self.duplex {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedOperation,
                "text and controls require duplex TTS",
            ));
        }
        let closing = matches!(command, TtsInput::Close);
        let mut input = self.input.lock().await;
        let Some(sender) = input.as_ref() else {
            return if closing { Ok(()) } else { Err(input_closed()) };
        };
        let sent = tokio::select! {
            biased;
            () = self.stop.cancelled() => false,
            result = sender.send(command) => result.is_ok(),
        };
        if closing {
            input.take();
            return Ok(());
        }
        if sent {
            Ok(())
        } else {
            Err(input_closed())
        }
    }

    pub async fn usage(&self) -> Option<VoiceUsage> {
        self.read.lock().await.usage.clone()
    }

    pub fn cancel_pending(&self) {
        self.stop.cancel();
    }

    pub async fn abort(&self) {
        self.cancel_pending();
        self.read.lock().await.events.take();
        self.input.lock().await.take();
    }
}

fn input_closed() -> ProtocolError {
    ProtocolError::new(
        ErrorCode::ProviderStreamError,
        "TTS session input is closed",
    )
}

fn project_event(event: StreamEvent) -> Option<TtsEvent> {
    let metadata = match event {
        StreamEvent::AudioDelta {
            data,
            format,
            sequence,
            tts,
        } => {
            return Some(TtsEvent {
                metadata: json!({
                    "kind": "audio", "format": format, "sequence": sequence,
                    "context_id": tts.map(|context| context.context_id),
                    "flush_id": tts.and_then(|context| context.flush_id),
                }),
                audio: Some(data),
            });
        }
        StreamEvent::Tts(event) => serde_json::to_value(event).ok()?,
        StreamEvent::Lifecycle(LifecycleEvent::SessionClosed) => json!({"kind": "session_closed"}),
        StreamEvent::Done { .. } => json!({"kind": "done"}),
        StreamEvent::Error { error, fatal } => json!({
            "kind": "error", "code": error.code, "message": error.message, "fatal": fatal,
        }),
        _ => return None,
    };
    Some(TtsEvent {
        metadata,
        audio: None,
    })
}
