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

pub use anthropic::{AnthropicAdapter, AnthropicConfig};
pub use deepseek::{DeepSeekAdapter, DeepSeekConfig};
pub use minimax::{MinimaxAdapter, MinimaxConfig};
pub use openai::{OpenAiAdapter, OpenAiConfig};
pub use openrouter::{OpenRouterAdapter, OpenRouterConfig};
pub use volcengine::{VolcengineAdapter, VolcengineConfig};
