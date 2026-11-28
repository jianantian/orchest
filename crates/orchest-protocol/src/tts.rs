//! Typed controls for incremental text-to-speech.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AudioFormat, EventStream};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsDuplexRequest {
    pub voice: String,
    pub format: AudioFormat,
    #[serde(default)]
    pub options: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TtsInput {
    /// Submit more text to the current context. Empty strings are ignored.
    Append(String),
    /// Finish the current context's audio while keeping the connection open.
    Flush,
    /// Stop output for the current context and allow a subsequent append.
    Cancel,
    /// Drain pending speech and close the connection. Later controls are ignored.
    Close,
}

/// Audio context of a duplex chunk. Audio delivered before a flush has no
/// flush ID, even if that audio forms part of the same synthesis context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtsAudioContext {
    pub context_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flush_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TtsEvent {
    FlushAccepted { context_id: u64, flush_id: u64 },
    Flushed { context_id: u64, flush_id: u64 },
    Cancelled { context_id: u64 },
    ContextEnded { context_id: u64 },
}

/// Duplex speech input and output. Clone or move the sender to feed text
/// concurrently while draining events. Dropping all senders requests closure.
#[derive(Debug)]
pub struct TtsDuplexHandle {
    pub input: tokio::sync::mpsc::Sender<TtsInput>,
    pub events: EventStream,
}
