//! Unified streaming event model for incremental responses across capabilities.
//!
//! One **delta-granular content-event core** (`StreamEvent`, extended in place so
//! the existing push-based `ModelAdapter` keeps compiling) carries text/thinking/
//! tool-use deltas plus the duplex/asr/tts additions (`AudioDelta`, `Transcript`,
//! `Content`, `Lifecycle`, `Error`). Routing/control concerns ride in
//! [`LifecycleEvent`]; modality-specific detail with no core meaning rides in
//! [`CapabilityEventExt`]. Designed in
//! `docs/archive/iteration/v0_9_12/issues/001-protocol-design/design.md` §1.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::error::ProtocolError;
use crate::response::TokenUsage;
use crate::types::ContentBlock;

// ---------------------------------------------------------------------------
// Supporting content-shape types (lifted into the spine for the event model)
// ---------------------------------------------------------------------------

/// Audio encoding for [`StreamEvent::AudioDelta`]. Superset of the asr/tts
/// per-crate `AudioFormat`s; those converge onto this in Issue 006.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    Pcm,
    Pcm16Le,
    Wav,
    WavPcm16Le,
    Opus,
    OggOpus,
    Mp3,
    Ogg,
    Flac,
}

/// Whether a transcript fragment is still revisable or finalized.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptStability {
    Provisional,
    Committed,
}

/// Whether a transcript update replaces the segment or appends to it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptUpdateKind {
    Snapshot,
    Append,
}

/// Identifies which transcript segment an event refers to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SegmentRef {
    pub segment_id: Option<String>,
    pub update_kind: TranscriptUpdateKind,
}

/// Routing / session / control signals — capability-neutral, **not** content.
/// Subsumes asr `RouteSelected`/`Started`/`EndOfSpeech`, tts `RouteSelected`/
/// `Started`, and the realtime transport lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "lifecycle", rename_all = "snake_case")]
pub enum LifecycleEvent {
    RouteSelected {
        provider: String,
        model: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trace_id: Option<String>,
    },
    SessionStarted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    EndOfSpeech {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segment: Option<SegmentRef>,
    },
    /// Client- or server-initiated barge-in.
    Interrupted,
    SessionClosed,
}

/// Typed, **closed** per-capability extension — the escape hatch for modality
/// detail with no core analogue (e.g. ASR endpointing). One variant per
/// capability that genuinely needs it; not a god-payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "capability", rename_all = "snake_case")]
pub enum CapabilityEventExt {
    /// ASR-specific detail (segment/endpointing/diagnostics) the rich
    /// `AsrModelCapabilities`-class events fold into during Issue 006.
    Asr(Value),
}

// ---------------------------------------------------------------------------
// The unified streaming event
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamEvent {
    // --- text / thinking (unchanged delta granularity) ---
    Text {
        delta: String,
    },
    ThinkingStart,
    Thinking {
        delta: String,
    },
    ThinkingEnd {
        signature: Option<String>,
        provider_details: Option<Value>,
    },

    // --- tool use (unchanged delta granularity — load-bearing for omni) ---
    ToolUseStart {
        id: String,
        name: String,
    },
    ToolUseArgsChunk {
        id: String,
        delta: String,
    },
    ToolUseEnd {
        id: String,
    },

    // --- audio output (duplex / tts) ---
    AudioDelta {
        data: Bytes,
        format: AudioFormat,
        sequence: u64,
    },

    // --- transcript (asr / omni) ---
    Transcript {
        text: String,
        stability: TranscriptStability,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segment: Option<SegmentRef>,
    },

    // --- a full content block emitted mid-stream (Chameleon: turn emits Image) ---
    //     reuses the content model, not a provider-local struct
    Content {
        block: ContentBlock,
    },

    // --- routing / session / control ---
    Lifecycle(LifecycleEvent),

    // --- in-band error (non-fatal or fatal) ---
    Error {
        error: ProtocolError,
        fatal: bool,
    },

    // --- typed per-capability extension ---
    Extension(CapabilityEventExt),

    // --- terminal ---
    Done {
        usage: TokenUsage,
    },
}

// ---------------------------------------------------------------------------
// Pulled delivery (the target for asr/tts/realtime; chat converges in Issue 005)
// ---------------------------------------------------------------------------

/// A pulled stream of [`StreamEvent`]s. Generalizes the existing
/// `AsrEventStream`/`TtsEventStream` so every pull-side capability speaks one
/// receiver type. New traits (`RealtimeSession`) expose this; the push-based
/// `ModelAdapter` keeps its `mpsc::Sender<StreamEvent>` until Issue 005.
#[derive(Debug)]
pub struct EventStream {
    inner: mpsc::Receiver<StreamEvent>,
}

impl EventStream {
    pub fn new(inner: mpsc::Receiver<StreamEvent>) -> Self {
        Self { inner }
    }

    /// Build a connected sink/stream pair for an in-process producer.
    pub fn channel(capacity: usize) -> (mpsc::Sender<StreamEvent>, Self) {
        let (tx, rx) = mpsc::channel(capacity);
        (tx, Self::new(rx))
    }

    /// Pull the next event, or `None` when the producer is done.
    pub async fn next(&mut self) -> Option<StreamEvent> {
        self.inner.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MediaSource;

    /// The **Chameleon ruler** (Issue 007 acceptance): a chat turn that produces an
    /// image emits a full `Image` content block **in its own `StreamEvent` stream**
    /// — the same channel that carries its text — so it needs no `GenTask`
    /// submit/poll/fetch job. This pins the design that a multimodal `ChatModel`'s
    /// image output rides the content model, not the gen tier.
    #[tokio::test]
    async fn chameleon_chat_stream_emits_image_without_gentask() {
        let (tx, mut events) = EventStream::channel(8);

        // One chat turn: a text delta, then a full Image content block, over the
        // single ChatModel output channel.
        tx.send(StreamEvent::Text {
            delta: "here is the picture you asked for: ".to_string(),
        })
        .await
        .unwrap();
        tx.send(StreamEvent::Content {
            block: ContentBlock::Image {
                source: MediaSource::Url {
                    url: "https://example/cat.png".to_string(),
                },
                detail: None,
            },
        })
        .await
        .unwrap();
        drop(tx);

        let mut saw_text = false;
        let mut image_url = None;
        while let Some(event) = events.next().await {
            match event {
                StreamEvent::Text { .. } => saw_text = true,
                StreamEvent::Content {
                    block:
                        ContentBlock::Image {
                            source: MediaSource::Url { url },
                            ..
                        },
                } => {
                    image_url = Some(url);
                }
                _ => {}
            }
        }

        assert!(saw_text, "the same stream carried the turn's text");
        assert_eq!(
            image_url.as_deref(),
            Some("https://example/cat.png"),
            "the chat stream emitted the Image block itself — no GenTask involved",
        );
    }
}
