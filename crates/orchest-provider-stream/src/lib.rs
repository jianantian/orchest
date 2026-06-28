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
pub mod transport;
pub mod tts;

use orchest_protocol::{Asr, RealtimeSession, Tts};
use orchest_provider_core::registry::Entry;

/// Streaming + WS-backed one-shot ASR dialects. The Volcengine openspeech
/// streaming dialect is registered here; construction is synchronous (the WS
/// handshake is deferred to `Asr::start_stream`), so it fits the sync factory.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn asr_entries() -> Vec<Entry<Box<dyn Asr>>> {
    vec![
        Entry::new(asr::volcengine::entry_descriptor(), |cfg| {
            Ok(Box::new(asr::volcengine::from_provider_config(cfg)?) as Box<dyn Asr>)
        }),
        Entry::new(asr::deepgram::entry_descriptor(), |cfg| {
            Ok(Box::new(asr::deepgram::from_provider_config(cfg)?) as Box<dyn Asr>)
        }),
        Entry::new(asr::soniox::entry_descriptor(), |cfg| {
            Ok(Box::new(asr::soniox::from_provider_config(cfg)?) as Box<dyn Asr>)
        }),
        Entry::new(asr::aliyun::entry_descriptor(), |cfg| {
            Ok(Box::new(asr::aliyun::from_provider_config(cfg)?) as Box<dyn Asr>)
        }),
        Entry::new(asr::elevenlabs::entry_descriptor(), |cfg| {
            Ok(Box::new(asr::elevenlabs::from_provider_config(cfg)?) as Box<dyn Asr>)
        }),
    ]
}

/// Streaming + WS-backed TTS dialects (openspeech, minimax-ws). The Volcengine
/// openspeech unidirectional TTS dialect is registered here; like ASR,
/// construction is synchronous (the handshake is deferred to the synthesize
/// calls), so it fits the sync factory.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn tts_entries() -> Vec<Entry<Box<dyn Tts>>> {
    vec![
        Entry::new(tts::volcengine::entry_descriptor(), |cfg| {
            Ok(Box::new(tts::volcengine::from_provider_config(cfg)?) as Box<dyn Tts>)
        }),
        Entry::new(tts::minimax::entry_descriptor(), |cfg| {
            Ok(Box::new(tts::minimax::from_provider_config(cfg)?) as Box<dyn Tts>)
        }),
        Entry::new(tts::aliyun::entry_descriptor(), |cfg| {
            Ok(Box::new(tts::aliyun::from_provider_config(cfg)?) as Box<dyn Tts>)
        }),
    ]
}

/// Omni full-duplex realtime sessions (openspeech). Filled in Issue 006.
pub fn realtime_entries() -> Vec<Entry<Box<dyn RealtimeSession>>> {
    Vec::new()
}
