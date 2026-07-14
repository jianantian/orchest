//! One module per LLM provider. Each submodule owns its endpoint resolution and
//! `ProviderProfile`, and (under
//! `#[cfg(test)]`) its own test suite — split into a sibling `tests.rs` (and, for
//! Anthropic, a shared `test_util.rs` mock-server helper reused by the other
//! providers' tests).

pub mod anthropic;
pub mod deepseek;
pub mod elss;
pub mod minimax;
pub mod openai;
pub mod openrouter;
pub mod volcengine;

// Every provider is now pure entry + profile over a shared protocol core (Chat via
// [`ChatAdapter`](crate::chat), Messages via [`MessagesAdapter`](crate::messages)) —
// no vendor adapter types remain (ADR-0002 Phase 3).
