//! `orchest-provider-stream` — WebSocket wire dialects (the WS weight tier).
//!
//! **Internal crate — not for direct use.** It is published only because
//! `orchest-provider` depends on it; its API carries no SemVer guarantee and
//! `orchest-provider` pins it to an exact version (ADR-0003 D2). Depend on
//! `orchest-provider` instead.
//!
//! Houses the openspeech binary protocol (shared by Volcengine asr/tts/omni),
//! minimax-ws TTS, the per-vendor streaming-ASR dialects, and the omni
//! `RealtimeSession` impl — the absorbed `agent-runtime-realtime-providers`
//! (Issue 006). The entry-producing functions below register each dialect through
//! the wall; every WS `Asr`/`Tts` factory is synchronous (the handshake is
//! deferred to the streaming call), while omni's `spawn_live` bridges the
//! sync-factory / async-connect gap.

pub mod asr;
pub mod catalog;
pub mod omni;
pub mod openspeech;
pub mod transport;
pub mod tts;

use orchest_protocol::{Asr, ErrorCode, ProtocolError, RealtimeSession, Tts};
use orchest_provider_core::registry::Entry;

/// Streaming + WS-backed one-shot ASR dialects. Entries are expanded 1:1 from
/// the stream ASR catalog; each factory pins `provider`/`model` onto the
/// runtime config while dialect free functions still accept uncataloged models.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn asr_entries() -> Vec<Entry<Box<dyn Asr>>> {
    catalog::asr::stream_asr_models()
        .iter()
        .map(|rec| {
            let model = rec.model;
            let provider = rec.provider;
            Entry::new(rec.to_descriptor(), move |cfg| {
                let mut pinned = cfg.clone();
                pinned.provider = provider.to_string();
                pinned.model = model.to_string();
                let handle: Box<dyn Asr> = match provider {
                    "aliyun" => Box::new(asr::aliyun::from_provider_config(&pinned)?),
                    "volcengine" => Box::new(asr::volcengine::from_provider_config(&pinned)?),
                    "deepgram" => Box::new(asr::deepgram::from_provider_config(&pinned)?),
                    "soniox" => Box::new(asr::soniox::from_provider_config(&pinned)?),
                    "elevenlabs" => Box::new(asr::elevenlabs::from_provider_config(&pinned)?),
                    other => {
                        return Err(ProtocolError::new(
                            ErrorCode::UnknownProvider,
                            format!("no stream ASR dialect for {other}"),
                        ))
                    }
                };
                Ok(handle)
            })
        })
        .collect()
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
