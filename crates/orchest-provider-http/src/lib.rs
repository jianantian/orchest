//! `orchest-provider-http` — REST + SSE wire dialects (the light weight tier).
//!
//! **Skeleton (Issue 004).** Dialect modules (openai-compat, anthropic,
//! minimax-rest, REST one-shot asr/tts, minimax music) and their concrete
//! registry entries are added by Issues 005/006/007. The entry-producing
//! functions below return empty vectors so the wall (`orchest-providers`) can
//! wire them behind features and compile with zero registered impls.

use orchest_protocol::{Asr, ChatModel, GenTask, Tts};
use orchest_provider_core::registry::Entry;

/// Chat (LLM) dialects: openai-compat, anthropic, minimax-rest, volc-ark.
/// Filled in Issue 005.
pub fn chat_entries() -> Vec<Entry<Box<dyn ChatModel>>> {
    Vec::new()
}

/// REST/SSE one-shot ASR dialects. Filled in Issue 006.
pub fn asr_entries() -> Vec<Entry<Box<dyn Asr>>> {
    Vec::new()
}

/// REST TTS dialects. Filled in Issue 006.
pub fn tts_entries() -> Vec<Entry<Box<dyn Tts>>> {
    Vec::new()
}

/// REST gen-task dialects (minimax music). Filled in Issue 007.
pub fn gen_entries() -> Vec<Entry<Box<dyn GenTask>>> {
    Vec::new()
}
