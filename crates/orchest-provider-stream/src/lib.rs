//! `orchest-provider-stream` — WebSocket wire dialects (the WS weight tier).
//!
//! **Skeleton (Issue 004).** The openspeech binary protocol (shared by
//! Volcengine asr/tts/omni), minimax-ws TTS, the per-vendor streaming-ASR
//! dialects, and the omni `RealtimeSession` impl are added in Issue 006, which
//! also absorbs and deletes `agent-runtime-realtime-providers`. The
//! entry-producing functions below return empty vectors for now.

pub mod asr;
pub mod omni;
pub mod openspeech;
pub mod tts;

use orchest_protocol::{Asr, RealtimeSession, Tts};
use orchest_provider_core::registry::Entry;

/// Streaming + WS-backed one-shot ASR dialects. The Volcengine openspeech
/// streaming dialect is registered here; construction is synchronous (the WS
/// handshake is deferred to `Asr::start_stream`), so it fits the sync factory.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn asr_entries() -> Vec<Entry<Box<dyn Asr>>> {
    vec![Entry::new(asr::volcengine::entry_descriptor(), |cfg| {
        Ok(Box::new(asr::volcengine::from_provider_config(cfg)?) as Box<dyn Asr>)
    })]
}

/// Streaming + WS-backed TTS dialects (openspeech, minimax-ws). Filled in Issue 006.
pub fn tts_entries() -> Vec<Entry<Box<dyn Tts>>> {
    Vec::new()
}

/// Omni full-duplex realtime sessions (openspeech). Filled in Issue 006.
pub fn realtime_entries() -> Vec<Entry<Box<dyn RealtimeSession>>> {
    Vec::new()
}
