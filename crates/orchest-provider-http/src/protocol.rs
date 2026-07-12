//! ADR-0002 protocol layer: wire-dialect factories decoupled from provider identity.
//!
//! Born in hotfix slice 001 as the walking skeleton — OpenAI routes through
//! [`ChatProtocolFactory`] while the other five LLM providers stay on the legacy
//! [`ProviderFactory`](crate::registry::ProviderFactory) bridge (see
//! `create_adapter_from_config`). Machinery is introduced only as far as this one
//! path needs it; profiles ([`ProviderProfile`] and its hooks) are born in slice
//! 002, the Messages factory in 005, the full `provider/[protocol/]model` grammar
//! and URL-resolution rules in 008.
//!
//! [`Protocol`] is **chat-scoped and lives entirely below the ADR-0001 wall**: it
//! is `pub(crate)`, never re-exported at the crate root, and never appears in a
//! wall-level (`orchest-provider`) or consumer-level (`orchest`/`orchest-node`/
//! `orchest-py`) type.

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::catalog::LlmModelEntry;

/// Wire protocol (the dialect an adapter speaks), chat-scoped. This is the
/// implementation seam ADR-0002 decouples from provider identity: a provider is
/// no longer "the Anthropic adapter" but "an entry that speaks Messages".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// Anthropic Messages API (`/v1/messages`). Constructed from slice 005.
    #[allow(dead_code)] // Messages factory + entries born in slice 005
    Messages,
    /// OpenAI Chat Completions API (`/v1/chat/completions`).
    Chat,
    /// OpenAI Responses API (`/v1/responses`). Identifier only — stateful, out
    /// of scope for Phases 1–2, no factory behind it (ADR "Caution on Responses").
    #[allow(dead_code)] // parseable/routable identifier; wired in slice 008
    Responses,
}

/// Value source for a provider header. `Env` values are read at *adapter
/// construction* time by the protocol factory — a runtime env-var value cannot
/// be `'static`, so the entry stores the env-var *name*, not the value.
// Constructed once `extra_headers` is populated by header injection in slice 004.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum HeaderValue {
    Static(&'static str),
    Env(&'static str),
}

/// A provider entry: identity + protocol preferences, **no adapter logic**.
///
/// Slice 001 populates only the fields OpenAI needs; `protocol_aliases` /
/// `path_overrides` / `extra_headers` are declared here (so the shape is stable
/// for later slices) and left empty. The `profiles` field is born with
/// [`ProviderProfile`] in slice 002.
// Several fields are declared here so the entry shape is stable for later
// slices but are not consumed until the slice that introduces the routing that
// reads them (noted per field). Silenced with justification rather than trimmed,
// to keep the ADR-0002 entry shape visible from slice 001.
#[derive(Debug)]
pub struct ProviderEntry {
    pub name: &'static str,
    /// Base URL (scheme + host, optionally a path prefix) — NOT a complete
    /// endpoint. The protocol factory appends the canonical path. Full
    /// URL-resolution rules are consolidated in slice 008.
    #[allow(dead_code)] // consumed by URL resolution in slice 008
    pub default_base_url: &'static str,
    pub default_api_key_env: &'static str,
    /// Protocols this provider supports, in preference order. The first is the
    /// default when the model string carries no explicit protocol segment
    /// (explicit selection lands in slice 008).
    pub protocols: &'static [Protocol],
    /// Provider-scoped aliases for the protocol segment of the model string
    /// (e.g. elss: `("anthropic", Messages)`, `("openai", Chat)`). Slice 009.
    #[allow(dead_code)] // consumed by the model-string grammar in slices 008/009
    pub protocol_aliases: &'static [(&'static str, Protocol)],
    /// Per-protocol endpoint path overrides for non-standard layouts
    /// (e.g. minimax: `(Messages, "/anthropic/v1/messages")`). Slice 006.
    #[allow(dead_code)] // consumed by the Messages factory in slice 006
    pub path_overrides: &'static [(Protocol, &'static str)],
    /// Provider-specific headers injected into every request
    /// (e.g. openrouter env-var headers). Slice 004.
    #[allow(dead_code)] // consumed by header injection in slice 004
    pub extra_headers: &'static [(&'static str, HeaderValue)],
}

/// Resolved once by the registry when a model string is parsed, then handed to
/// the protocol factory (and, from slice 002, every profile hook). Carries the
/// identity and capability facts that [`ProviderConfig`] does not, so nothing
/// downstream rediscovers them from the model name.
#[derive(Debug)]
pub struct ResolvedModel<'a> {
    // `provider`/`protocol`/`catalog` are read by profile hooks (slice 002) and
    // the capability wiring (slice 007); slice 001 only needs `model` to build
    // the wrapped adapter, but resolves the full context now so the shape the
    // factory receives is final.
    #[allow(dead_code)] // read by profile hooks from slice 002
    pub provider: &'a ProviderEntry,
    #[allow(dead_code)] // read by profile hooks / explicit routing from slice 002
    pub protocol: Protocol,
    /// Bare model name (provider prefix stripped).
    pub model: &'a str,
    /// The model's catalog entry — the canonical source of capability facts.
    /// `None` for dynamic-gateway models that cannot be enumerated statically.
    #[allow(dead_code)] // read as the canonical capability source from slice 007
    pub catalog: Option<&'a LlmModelEntry>,
}

/// A factory that builds an adapter for a specific wire protocol. Reusable
/// across providers: the canonical behavior lives here, per-provider residual
/// (from slice 002) rides in a [`ProviderProfile`] attached to the entry.
///
/// Lives below the ADR-0001 wall. Construction input is the same
/// [`ProviderConfig`] the wall already uses — not the retired positional
/// `(model, max_tokens, api_key, api_url)` signature — plus the [`ResolvedModel`].
pub trait ProtocolFactory: Send + Sync {
    // Identifies the factory's protocol; the registry indexes factories by it
    // once more than one exists (Messages is born in slice 005).
    #[allow(dead_code)] // used for factory indexing from slice 005
    fn protocol(&self) -> Protocol;

    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
    fn create_adapter(
        &self,
        config: &ProviderConfig,
        resolved: &ResolvedModel<'_>,
    ) -> Result<Box<dyn ChatModel>, ProtocolError>;
}

/// The canonical OpenAI Chat Completions dialect.
///
/// Transitional (Phase 1): this factory *wraps* the existing
/// [`OpenAiAdapter`](crate::providers::openai::OpenAiAdapter), which already owns
/// the canonical Chat envelope (messages array, tool-call assembly, SSE decode,
/// `stream_options.include_usage`) and canonical-path append/idempotency via its
/// `normalize_chat_url`. v0.12 collapses that adapter's logic into this core.
pub struct ChatProtocolFactory;

impl ProtocolFactory for ChatProtocolFactory {
    fn protocol(&self) -> Protocol {
        Protocol::Chat
    }

    fn create_adapter(
        &self,
        config: &ProviderConfig,
        resolved: &ResolvedModel<'_>,
    ) -> Result<Box<dyn ChatModel>, ProtocolError> {
        use crate::providers::openai::{OpenAiAdapter, OpenAiConfig};

        let adapter = OpenAiAdapter::from_config(OpenAiConfig {
            model: resolved.model.to_string(),
            max_tokens: config.max_tokens.unwrap_or(crate::defaults::MAX_TOKENS),
            api_key: config.api_key.clone(),
            api_url: config.api_url.clone(),
        })
        .map_err(ProtocolError::from)?;
        Ok(Box::new(adapter))
    }
}

/// The provider entries already migrated to the protocol-factory path. Grows one
/// entry per slice; a provider absent here stays on the legacy `ProviderFactory`
/// bridge. Slice 001 migrates OpenAI only.
static OPENAI_ENTRY: ProviderEntry = ProviderEntry {
    name: "openai",
    // Base URL, not the complete endpoint; ChatProtocolFactory (via the wrapped
    // adapter's normalize_chat_url) appends `/v1/chat/completions`.
    default_base_url: "https://api.openai.com",
    default_api_key_env: "OPENAI_API_KEY",
    protocols: &[Protocol::Chat],
    protocol_aliases: &[],
    path_overrides: &[],
    extra_headers: &[],
};

/// The migrated provider entry for `name`, or `None` if the provider is still on
/// the legacy bridge.
pub fn provider_entry(name: &str) -> Option<&'static ProviderEntry> {
    match name {
        "openai" => Some(&OPENAI_ENTRY),
        _ => None,
    }
}

/// The protocol factory for `protocol`, or `None` when no factory exists yet
/// (Messages is born in slice 005; Responses has no factory by design).
pub fn protocol_factory(protocol: Protocol) -> Option<Box<dyn ProtocolFactory>> {
    match protocol {
        Protocol::Chat => Some(Box::new(ChatProtocolFactory)),
        Protocol::Messages | Protocol::Responses => None,
    }
}
