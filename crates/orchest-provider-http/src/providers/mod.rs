//! One module per LLM provider adapter. Each submodule owns its request/response
//! mapping, streaming, and (under `#[cfg(test)]`) its own test suite — split into a
//! sibling `tests.rs` (and, for Anthropic, a shared `test_util.rs` mock-server helper
//! reused by the other adapters' tests).

pub mod anthropic;
pub mod deepseek;
pub mod elss;
pub mod minimax;
pub mod openai;
pub mod openrouter;
pub mod volcengine;

// Chat providers (openai/deepseek/volcengine/openrouter) are pure entry +
// profile over the shared ChatAdapter (ADR-0002 Phase 3) — no vendor adapter
// types. Anthropic/Minimax (Messages) are collapsed in the follow-on.
pub use anthropic::{AnthropicAdapter, AnthropicConfig};
pub use minimax::{MinimaxAdapter, MinimaxConfig};
