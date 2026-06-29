//! `orchest-provider-stream` — WebSocket wire dialects (the WS weight tier).
//!
//! Houses the openspeech binary protocol (shared by Volcengine asr/tts/omni),
//! minimax-ws TTS, the per-vendor streaming-ASR dialects, and the omni
//! `RealtimeSession` impl — the absorbed `agent-runtime-realtime-providers`
//! (Issue 006). The entry-producing functions below register each dialect through
//! the wall; every WS `Asr`/`Tts` factory is synchronous (the handshake is
//! deferred to the streaming call), while omni's `spawn_live` bridges the
//! sync-factory / async-connect gap.

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

/// Omni full-duplex realtime sessions (Volcengine openspeech). The factory is
/// sync but the omni connect is async, so `spawn_live` spawns a connect-then-run
/// task and hands back the session immediately; a connect failure surfaces as a
/// fatal `Error` on `events()`.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn realtime_entries() -> Vec<Entry<Box<dyn RealtimeSession>>> {
    vec![Entry::new(omni::entry_descriptor(), |cfg| {
        Ok(Box::new(omni::from_provider_config(cfg)?) as Box<dyn RealtimeSession>)
    })]
}
