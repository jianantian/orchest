//! `orchest-provider-stream` — WebSocket wire dialects (the WS weight tier).
//!
//! **Skeleton (Issue 004).** The openspeech binary protocol (shared by
//! Volcengine asr/tts/omni), minimax-ws TTS, the per-vendor streaming-ASR
//! dialects, and the omni `RealtimeSession` impl are added in Issue 006, which
//! also absorbs and deletes `agent-runtime-realtime-providers`. The
//! entry-producing functions below return empty vectors for now.

pub mod openspeech;

use orchest_protocol::{Asr, RealtimeSession, Tts};
use orchest_provider_core::registry::Entry;

/// Streaming + WS-backed one-shot ASR dialects. Filled in Issue 006.
pub fn asr_entries() -> Vec<Entry<Box<dyn Asr>>> {
    Vec::new()
}

/// Streaming + WS-backed TTS dialects (openspeech, minimax-ws). Filled in Issue 006.
pub fn tts_entries() -> Vec<Entry<Box<dyn Tts>>> {
    Vec::new()
}

/// Omni full-duplex realtime sessions (openspeech). Filled in Issue 006.
pub fn realtime_entries() -> Vec<Entry<Box<dyn RealtimeSession>>> {
    Vec::new()
}
