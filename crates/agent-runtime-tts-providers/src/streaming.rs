use bytes::Bytes;
use tokio::sync::mpsc;

use crate::error::{TtsError, TtsErrorCode};
use crate::types::{AudioFormat, TextChunk, TtsStreamSummary, VoiceInfo};

#[derive(Debug, Clone)]
pub enum TtsStreamEvent {
    RouteSelected {
        trace_id: String,
        provider: String,
        model: String,
    },
    Started {
        trace_id: String,
        provider: String,
        model: String,
        voice: VoiceInfo,
    },
    TextDelta {
        trace_id: String,
        text: String,
        sequence: u64,
        is_final: bool,
    },
    TextAccepted {
        trace_id: String,
        chars: u64,
    },
    AudioChunk {
        trace_id: String,
        data: Bytes,
        format: AudioFormat,
        sequence: u64,
    },
    Completed {
        trace_id: String,
        summary: TtsStreamSummary,
    },
    Error {
        trace_id: String,
        error: TtsError,
        fatal: bool,
    },
}

impl TtsStreamEvent {
    pub fn is_route_selected(&self) -> bool {
        matches!(self, Self::RouteSelected { .. })
    }

    pub fn is_started(&self) -> bool {
        matches!(self, Self::Started { .. })
    }

    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Error { fatal: true, .. }
        )
    }

    pub fn trace_id(&self) -> &str {
        match self {
            Self::RouteSelected { trace_id, .. }
            | Self::Started { trace_id, .. }
            | Self::TextDelta { trace_id, .. }
            | Self::TextAccepted { trace_id, .. }
            | Self::AudioChunk { trace_id, .. }
            | Self::Completed { trace_id, .. }
            | Self::Error { trace_id, .. } => trace_id,
        }
    }
}

pub struct TtsEventStream {
    inner: mpsc::Receiver<TtsStreamEvent>,
}

impl std::fmt::Debug for TtsEventStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TtsEventStream").finish_non_exhaustive()
    }
}

impl TtsEventStream {
    pub fn new(receiver: mpsc::Receiver<TtsStreamEvent>) -> Self {
        Self { inner: receiver }
    }

    pub async fn next(&mut self) -> Option<TtsStreamEvent> {
        self.inner.recv().await
    }

    pub async fn collect_until_terminal(&mut self) -> Result<TtsStreamEvent, TtsError> {
        loop {
            match self.inner.recv().await {
                Some(event) if event.is_terminal() => return Ok(event),
                Some(_) => continue,
                None => {
                    return Err(TtsError::new(
                        TtsErrorCode::Cancelled,
                        "event stream closed before terminal event",
                    ))
                }
            }
        }
    }
}

pub struct TtsOutputStream {
    pub events: TtsEventStream,
}

impl std::fmt::Debug for TtsOutputStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TtsOutputStream")
            .field("events", &self.events)
            .finish()
    }
}

impl TtsOutputStream {
    pub fn new(receiver: mpsc::Receiver<TtsStreamEvent>) -> Self {
        Self {
            events: TtsEventStream::new(receiver),
        }
    }
}

#[derive(Clone)]
pub struct TtsTextSink {
    inner: mpsc::Sender<TextChunk>,
}

impl std::fmt::Debug for TtsTextSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TtsTextSink").finish_non_exhaustive()
    }
}

impl TtsTextSink {
    pub fn new(sender: mpsc::Sender<TextChunk>) -> Self {
        Self { inner: sender }
    }

    pub async fn send_text(&self, text: impl Into<String>) -> Result<(), TtsError> {
        self.inner
            .send(TextChunk {
                text: text.into(),
                is_final: false,
            })
            .await
            .map_err(|_| TtsError::new(TtsErrorCode::Cancelled, "duplex text stream closed"))
    }

    pub async fn finish(&self) -> Result<(), TtsError> {
        self.inner
            .send(TextChunk {
                text: String::new(),
                is_final: true,
            })
            .await
            .map_err(|_| TtsError::new(TtsErrorCode::Cancelled, "duplex text stream closed"))
    }
}

pub struct TtsDuplexStream {
    pub input: TtsTextSink,
    pub events: TtsEventStream,
}

impl std::fmt::Debug for TtsDuplexStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TtsDuplexStream")
            .field("input", &self.input)
            .field("events", &self.events)
            .finish()
    }
}

impl TtsDuplexStream {
    pub fn new(input: mpsc::Sender<TextChunk>, events: mpsc::Receiver<TtsStreamEvent>) -> Self {
        Self {
            input: TtsTextSink::new(input),
            events: TtsEventStream::new(events),
        }
    }
}

pub fn create_output_pair(capacity: usize) -> (mpsc::Sender<TtsStreamEvent>, TtsOutputStream) {
    let (tx, rx) = mpsc::channel(capacity);
    (tx, TtsOutputStream::new(rx))
}

pub fn create_duplex_pair(
    input_capacity: usize,
    event_capacity: usize,
) -> (
    mpsc::Sender<TextChunk>,
    mpsc::Receiver<TextChunk>,
    mpsc::Sender<TtsStreamEvent>,
    TtsDuplexStream,
) {
    let (input_tx, input_rx) = mpsc::channel(input_capacity);
    let (event_tx, event_rx) = mpsc::channel(event_capacity);
    (
        input_tx.clone(),
        input_rx,
        event_tx,
        TtsDuplexStream::new(input_tx, event_rx),
    )
}
