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

use serde_json::{json, Value};

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::catalog::LlmModelEntry;
use crate::{
    CachePolicy, CompatibilityPolicy, ContentBlock, ModelError, OptionAdjustment, RequestOptions,
    Role, ThinkingLevel, TokenUsage,
};

/// Wire protocol (the dialect an adapter speaks), chat-scoped. This is the
/// implementation seam ADR-0002 decouples from provider identity: a provider is
/// no longer "the Anthropic adapter" but "an entry that speaks Messages".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// Anthropic Messages API (`/v1/messages`).
    Messages,
    /// OpenAI Chat Completions API (`/v1/chat/completions`).
    Chat,
    /// OpenAI Responses API (`/v1/responses`). Identifier only — stateful, out
    /// of scope for Phases 1–2, no factory behind it (ADR "Caution on Responses").
    #[allow(dead_code)] // parseable/routable identifier; wired in slice 008
    Responses,
}

/// Value source for a provider header. `Env` values are read at *adapter
/// construction* time (via [`resolve_headers`]) — a runtime env-var value cannot
/// be `'static`, so the entry stores the env-var *name*, not the value.
#[derive(Debug, Clone, Copy)]
pub enum HeaderValue {
    Static(&'static str),
    Env(&'static str),
}

/// Transitional per-provider adapter constructor referenced (as data) by a
/// [`ProviderEntry`], for either protocol. This is how a `ProtocolFactory` builds
/// the right wrapped adapter **without matching on provider name** (ADR rule 1):
/// the entry carries its constructor, the factory just calls it. Phase 1 keeps
/// each provider's existing adapter behind this pointer; v0.12 collapses the
/// adapters into the protocol cores and this indirection goes away.
pub type AdapterCtor =
    fn(&ProviderConfig, &ResolvedModel<'_>) -> Result<Box<dyn ChatModel>, ProtocolError>;

/// A provider entry: identity + protocol preferences + profiles, **no adapter
/// logic of its own** (construction is delegated to the `build_adapter` ctor).
///
/// `protocol_aliases` / `path_overrides` / `extra_headers` are declared here (so
/// the shape is stable for later slices) and left empty until the slice that
/// reads them.
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
    /// (e.g. minimax: `(Messages, "/anthropic/v1/messages")`). Declared here in
    /// slice 006; the wrapped adapter's `normalize_messages_url` produces the path
    /// today, and factory-driven URL resolution consumes this in slice 008.
    #[allow(dead_code)] // consumed by URL resolution in slice 008
    pub path_overrides: &'static [(Protocol, &'static str)],
    /// Provider-specific headers injected into every request
    /// (e.g. openrouter env-var headers), resolved via [`resolve_headers`].
    pub extra_headers: &'static [(&'static str, HeaderValue)],
    /// Behavior profiles per protocol, for providers that deviate from
    /// protocol-canonical behavior. Empty for fully compatible providers.
    pub profiles: &'static [(Protocol, &'static dyn ProviderProfile)],
    /// Constructs the wrapped adapter for this provider (transitional; see
    /// [`AdapterCtor`]).
    pub build_adapter: AdapterCtor,
}

impl ProviderEntry {
    /// The profile attached to this entry for `protocol`, if any.
    pub fn profile_for(&self, protocol: Protocol) -> Option<&'static dyn ProviderProfile> {
        self.profiles
            .iter()
            .find(|(p, _)| *p == protocol)
            .map(|(_, profile)| *profile)
    }
}

impl std::fmt::Debug for ProviderEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderEntry")
            .field("name", &self.name)
            .field("protocols", &self.protocols)
            .finish_non_exhaustive()
    }
}

/// Resolved once by the registry when a model string is parsed, then handed to
/// the protocol factory (and, from slice 002, every profile hook). Carries the
/// identity and capability facts that [`ProviderConfig`] does not, so nothing
/// downstream rediscovers them from the model name.
#[derive(Debug)]
pub struct ResolvedModel<'a> {
    /// The resolved provider entry — carries the Chat ctor, profiles, and
    /// URL/header data. Read by the factory dispatch and profile lookup.
    pub provider: &'a ProviderEntry,
    // `protocol` is resolved and carried now so the shape is final, but is not
    // yet read (explicit-protocol routing that branches on it lands in slice 008).
    #[allow(dead_code)] // read by explicit-protocol routing from slice 008
    pub protocol: Protocol,
    /// Bare model name (provider prefix stripped).
    pub model: &'a str,
    /// The model's catalog entry — the canonical source of capability facts.
    /// `None` for dynamic-gateway models that cannot be enumerated statically.
    pub catalog: Option<&'a LlmModelEntry>,
}

/// Narrow, named extension surface for provider-specific behavior *within* a
/// Chat protocol — the home of Problem 5's residual. Every hook has a default =
/// the protocol-canonical behavior; a profile overrides only what its provider
/// actually deviates on.
///
/// Hooks are added **by name, one at a time, when a real provider demonstrates
/// the need** (ADR rule 3). This slice births `lower_options` + `replay_reasoning`
/// for DeepSeek; `map_role` / `interpret_usage` / `option_support` /
/// `normalize_error` arrive with the providers that need them (slices 003/004/006).
/// There is deliberately **no** generic `modify_request(&mut body)` escape hatch:
/// each hook's scope is its name.
pub trait ProviderProfile: Send + Sync {
    /// Lower canonical request options (thinking level, sampling) onto the wire
    /// body, reporting any degradation as `OptionAdjustment`s. Default:
    /// Chat-canonical `reasoning_effort` lowering, with reasoning support read
    /// from `cx.catalog` (ADR "Capability metadata"). DeepSeek overrides this
    /// with its top-level `thinking: {type}` dialect.
    fn lower_options(
        &self,
        cx: &ResolvedModel<'_>,
        options: &RequestOptions,
        body: &mut Value,
    ) -> Vec<OptionAdjustment> {
        let mut adjustments = Vec::new();

        let supports_reasoning = cx.catalog.map(|c| c.thinking.is_some()).unwrap_or(false);
        if options.thinking != ThinkingLevel::Off && supports_reasoning {
            let effort = match options.thinking {
                ThinkingLevel::Off => unreachable!(),
                ThinkingLevel::Minimal | ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High | ThinkingLevel::XHigh | ThinkingLevel::Max => "high",
            };
            body["reasoning_effort"] = json!(effort);
        } else if options.thinking != ThinkingLevel::Off {
            adjustments.push(OptionAdjustment {
                option: "thinking".into(),
                requested: json!(format!("{:?}", options.thinking)),
                applied: json!("Off"),
                reason: "unsupported_reasoning_model".into(),
            });
        }

        if options.thinking_budget_tokens.is_some() {
            adjustments.push(OptionAdjustment {
                option: "thinking_budget_tokens".into(),
                requested: json!(options.thinking_budget_tokens),
                applied: json!(null),
                reason: "unsupported_by_provider".into(),
            });
        }

        if options.cache_policy == CachePolicy::Long {
            adjustments.push(OptionAdjustment {
                option: "cache_policy".into(),
                requested: json!("Long"),
                applied: json!("Auto"),
                reason: "unsupported_cache_retention".into(),
            });
        }

        if let Some(temp) = options.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(tp) = options.top_p {
            body["top_p"] = json!(tp);
        }

        adjustments
    }

    /// Re-inject prior assistant reasoning when replaying a historical assistant
    /// message. Default: no replay (canonical Chat drops Thinking blocks, as
    /// OpenAI does). DeepSeek overrides this to emit `reasoning_content`.
    fn replay_reasoning(
        &self,
        _cx: &ResolvedModel<'_>,
        _assistant_msg: &mut Value,
        _blocks: &[ContentBlock],
    ) {
    }

    /// Declare support for a canonical option so shared `CompatibilityPolicy`
    /// handling can degrade or error uniformly instead of each adapter carrying
    /// its own branch. Default: permissive — the catalog schema carries no
    /// output-exclusion flag yet, so a provider that cannot honor an option
    /// declares it `Unsupported` explicitly (Volcengine, slice 003).
    fn option_support(&self, _cx: &ResolvedModel<'_>, _option: RequestOption) -> OptionSupport {
        OptionSupport::Supported
    }

    /// Map a canonical role onto the provider-accepted wire role. Default: the
    /// canonical Messages mapping, which downgrades Minimax-only roles (as the
    /// Chat providers do via `role_compat`). Minimax overrides this to emit its
    /// native roles (`user_system` / `group` / `sample_message_*`).
    fn map_role(&self, _cx: &ResolvedModel<'_>, role: &Role) -> WireRole {
        WireRole(match role {
            Role::Assistant => "assistant",
            Role::System | Role::UserSystem => "system",
            _ => "user",
        })
    }

    /// Interpret provider-specific usage reporting into canonical `TokenUsage`,
    /// returning any degradation adjustments. Default: the canonical
    /// usage-missing handling shared by OpenAI / DeepSeek / OpenRouter — when a
    /// provider reports no usage, record telemetry and a `usage_not_reported`
    /// adjustment. `raw` is the provider's raw usage value for profiles that need
    /// to reinterpret specific fields (unused by the default).
    fn interpret_usage(
        &self,
        cx: &ResolvedModel<'_>,
        _raw: &Value,
        usage: &mut TokenUsage,
    ) -> Vec<OptionAdjustment> {
        if usage.input_tokens == 0 && usage.output_tokens == 0 {
            crate::telemetry::record_usage_missing(cx.provider.name, cx.model);
            vec![OptionAdjustment {
                option: "usage".into(),
                requested: json!(null),
                applied: json!(null),
                reason: "usage_not_reported".into(),
            }]
        } else {
            Vec::new()
        }
    }
}

/// Resolve an entry's static/env headers to concrete `(name, value)` pairs at
/// adapter-construction time. `Env` values are read from the environment now (a
/// runtime value cannot be `'static`); unset env vars are skipped.
pub fn resolve_headers(entry: &ProviderEntry) -> Vec<(&'static str, String)> {
    entry
        .extra_headers
        .iter()
        .filter_map(|(name, value)| {
            let resolved = match value {
                HeaderValue::Static(s) => Some((*s).to_string()),
                HeaderValue::Env(var) => std::env::var(var).ok(),
            };
            resolved.map(|v| (*name, v))
        })
        .collect()
}

/// A canonical request option whose provider support is queried via
/// [`ProviderProfile::option_support`]. Added by name as providers demonstrate
/// the need (ADR rule 3); slice 003 births reasoning output exclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOption {
    /// Emit reasoning internally but exclude it from the response
    /// (`include_thinking: false` while thinking is enabled).
    ReasoningOutputExclusion,
}

/// A provider-accepted wire role name, produced by [`ProviderProfile::map_role`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireRole(pub &'static str);

/// The result of an [`ProviderProfile::option_support`] query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionSupport {
    Supported,
    /// Unsupported, with the canonical error `code`/`message` to surface under
    /// `CompatibilityPolicy::Strict`.
    Unsupported {
        code: &'static str,
        message: &'static str,
    },
}

/// Shared `CompatibilityPolicy` handling for reasoning output exclusion, driven
/// by the profile's [`ProviderProfile::option_support`] declaration — so the
/// Strict-errors / Coerce-degrades decision lives here once rather than in each
/// adapter. Returns the effective thinking flag plus any degradation adjustment.
#[allow(clippy::result_large_err)] // justified: ModelError carries diagnostic context (workspace convention)
pub fn resolve_reasoning_exclusion(
    profile: &dyn ProviderProfile,
    cx: &ResolvedModel<'_>,
    options: &RequestOptions,
    thinking_enabled: bool,
) -> Result<(bool, Option<OptionAdjustment>), ModelError> {
    // Exclusion only matters when the caller wants thinking on but its output off.
    if options.include_thinking || !thinking_enabled {
        return Ok((thinking_enabled, None));
    }
    match profile.option_support(cx, RequestOption::ReasoningOutputExclusion) {
        OptionSupport::Supported => Ok((thinking_enabled, None)),
        OptionSupport::Unsupported { code, message } => match options.compatibility_policy {
            CompatibilityPolicy::Strict => Err(ModelError {
                message: message.into(),
                code: Some(code.into()),
                provider: Some(cx.provider.name.into()),
                status: None,
                retry_after_secs: None,
                upstream: None,
            }),
            CompatibilityPolicy::Coerce => Ok((
                false,
                Some(OptionAdjustment {
                    option: "include_thinking".into(),
                    requested: json!(false),
                    applied: json!(false),
                    reason: "thinking_disabled_for_output_exclusion".into(),
                }),
            )),
        },
    }
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

/// The canonical Chat Completions dialect, shared across Chat providers.
///
/// Transitional (Phase 1): construction is delegated to the resolved provider's
/// [`ProviderEntry::build_adapter`] ctor, which wraps that provider's existing
/// adapter — so the factory never matches on provider name (ADR rule 1). The
/// per-provider behavioral residual lives in the [`ProviderProfile`] attached to
/// the entry (DeepSeek from slice 002). v0.12 collapses the wrapped adapters into
/// this core and the ctor indirection goes away.
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
        (resolved.provider.build_adapter)(config, resolved)
    }
}

/// The canonical Anthropic Messages dialect (`/v1/messages`), shared across
/// Messages providers. Symmetric with [`ChatProtocolFactory`]: construction is
/// delegated to the entry's [`AdapterCtor`] (no provider-name match). Anthropic
/// is canonical Messages with no profile; Messages-side profile dispatch reuses
/// the same [`ProviderProfile`] trait when Minimax needs it (slice 006).
pub struct MessagesProtocolFactory;

impl ProtocolFactory for MessagesProtocolFactory {
    fn protocol(&self) -> Protocol {
        Protocol::Messages
    }

    fn create_adapter(
        &self,
        config: &ProviderConfig,
        resolved: &ResolvedModel<'_>,
    ) -> Result<Box<dyn ChatModel>, ProtocolError> {
        (resolved.provider.build_adapter)(config, resolved)
    }
}

/// The provider entries already migrated to the protocol-factory path. Grows one
/// entry per slice; a provider absent here stays on the legacy `ProviderFactory`
/// bridge. Slices 001–002 migrate OpenAI (canonical, no profile) and DeepSeek
/// (Chat + `DeepSeekProfile`).
static OPENAI_ENTRY: ProviderEntry = ProviderEntry {
    name: "openai",
    // Base URL, not the complete endpoint; the wrapped adapter's
    // normalize_chat_url appends `/v1/chat/completions`.
    default_base_url: "https://api.openai.com",
    default_api_key_env: "OPENAI_API_KEY",
    protocols: &[Protocol::Chat],
    protocol_aliases: &[],
    path_overrides: &[],
    extra_headers: &[],
    // OpenAI is canonical Chat — no deviation, so no profile.
    profiles: &[],
    build_adapter: crate::providers::openai::build_chat_adapter,
};

static DEEPSEEK_ENTRY: ProviderEntry = ProviderEntry {
    name: "deepseek",
    default_base_url: "https://api.deepseek.com",
    default_api_key_env: "DEEPSEEK_API_KEY",
    protocols: &[Protocol::Chat],
    protocol_aliases: &[],
    path_overrides: &[],
    extra_headers: &[],
    // DeepSeek is OpenAI-compatible Chat with a reasoning-dialect deviation.
    profiles: &[(
        Protocol::Chat,
        &crate::providers::deepseek::DEEPSEEK_PROFILE,
    )],
    build_adapter: crate::providers::deepseek::build_chat_adapter,
};

static VOLCENGINE_ENTRY: ProviderEntry = ProviderEntry {
    name: "volcengine",
    default_base_url: "https://ark.cn-beijing.volces.com/api/v3",
    default_api_key_env: "ARK_API_KEY",
    protocols: &[Protocol::Chat],
    protocol_aliases: &[],
    path_overrides: &[],
    extra_headers: &[],
    // Volcengine: OpenAI-compatible Chat with a distinct thinking shape and no
    // reasoning-output exclusion.
    profiles: &[(
        Protocol::Chat,
        &crate::providers::volcengine::VOLCENGINE_PROFILE,
    )],
    build_adapter: crate::providers::volcengine::build_chat_adapter,
};

static OPENROUTER_ENTRY: ProviderEntry = ProviderEntry {
    name: "openrouter",
    default_base_url: "https://openrouter.ai/api",
    default_api_key_env: "OPENROUTER_API_KEY",
    protocols: &[Protocol::Chat],
    protocol_aliases: &[],
    path_overrides: &[],
    // Routing headers whose values come from the environment at construction.
    extra_headers: &[
        (
            "X-OpenRouter-Title",
            HeaderValue::Env("OPENROUTER_APP_TITLE"),
        ),
        ("HTTP-Referer", HeaderValue::Env("OPENROUTER_SITE_URL")),
    ],
    profiles: &[(
        Protocol::Chat,
        &crate::providers::openrouter::OPENROUTER_PROFILE,
    )],
    build_adapter: crate::providers::openrouter::build_chat_adapter,
};

static ANTHROPIC_ENTRY: ProviderEntry = ProviderEntry {
    name: "anthropic",
    default_base_url: "https://api.anthropic.com",
    default_api_key_env: "ANTHROPIC_API_KEY",
    protocols: &[Protocol::Messages],
    protocol_aliases: &[],
    path_overrides: &[],
    extra_headers: &[],
    // Anthropic is canonical Messages — no deviation, so no profile.
    profiles: &[],
    build_adapter: crate::providers::anthropic::build_messages_adapter,
};

static MINIMAX_ENTRY: ProviderEntry = ProviderEntry {
    name: "minimax",
    default_base_url: "https://api.minimaxi.com",
    default_api_key_env: "MINIMAX_API_KEY",
    protocols: &[Protocol::Messages],
    protocol_aliases: &[],
    // Minimax's Anthropic-compatible endpoint is /anthropic/v1/messages, not the
    // canonical /v1/messages.
    path_overrides: &[(Protocol::Messages, "/anthropic/v1/messages")],
    extra_headers: &[],
    // Minimax is Anthropic-Messages-compatible with the widest Messages-side
    // deviation (role downgrade + thinking:{type,display} dialect).
    profiles: &[(
        Protocol::Messages,
        &crate::providers::minimax::MINIMAX_PROFILE,
    )],
    build_adapter: crate::providers::minimax::build_messages_adapter,
};

/// The migrated provider entry for `name`, or `None` if the provider is still on
/// the legacy bridge.
pub fn provider_entry(name: &str) -> Option<&'static ProviderEntry> {
    match name {
        "openai" => Some(&OPENAI_ENTRY),
        "deepseek" => Some(&DEEPSEEK_ENTRY),
        "volcengine" => Some(&VOLCENGINE_ENTRY),
        "openrouter" => Some(&OPENROUTER_ENTRY),
        "anthropic" => Some(&ANTHROPIC_ENTRY),
        "minimax" => Some(&MINIMAX_ENTRY),
        _ => None,
    }
}

/// The protocol factory for `protocol`, or `None` when no factory exists yet
/// (Messages is born in slice 005; Responses has no factory by design).
pub fn protocol_factory(protocol: Protocol) -> Option<Box<dyn ProtocolFactory>> {
    match protocol {
        Protocol::Chat => Some(Box::new(ChatProtocolFactory)),
        Protocol::Messages => Some(Box::new(MessagesProtocolFactory)),
        Protocol::Responses => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile that overrides nothing — every hook is the protocol-canonical
    /// default. Stands in for a fully compatible provider.
    struct NoProfile;
    impl ProviderProfile for NoProfile {}

    fn resolved(model: &'static str, model_id: &'static str) -> ResolvedModel<'static> {
        ResolvedModel {
            provider: provider_entry("deepseek").unwrap(),
            protocol: Protocol::Chat,
            model,
            catalog: crate::catalog::find_model(model_id),
        }
    }

    #[test]
    fn canonical_lower_options_emits_reasoning_effort_for_thinking_model() {
        // deepseek-v4-flash declares thinking support in the catalog.
        let cx = resolved("deepseek-v4-flash", "deepseek/deepseek-v4-flash");
        let opts = RequestOptions {
            thinking: ThinkingLevel::Medium,
            temperature: Some(0.5),
            ..Default::default()
        };
        let mut body = json!({});
        let adj = NoProfile.lower_options(&cx, &opts, &mut body);
        assert_eq!(body["reasoning_effort"], "medium");
        assert_eq!(body["temperature"], 0.5);
        assert!(adj.is_empty());
    }

    #[test]
    fn canonical_lower_options_records_adjustment_for_non_thinking_model() {
        // A model absent from the catalog: the canonical default treats reasoning
        // as unsupported (it reads only the catalog, no name-prefix fallback).
        let cx = ResolvedModel {
            provider: provider_entry("openai").unwrap(),
            protocol: Protocol::Chat,
            model: "made-up-model",
            catalog: crate::catalog::find_model("openai/made-up-model"),
        };
        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            ..Default::default()
        };
        let mut body = json!({});
        let adj = NoProfile.lower_options(&cx, &opts, &mut body);
        assert!(body.get("reasoning_effort").is_none());
        assert!(adj
            .iter()
            .any(|a| a.reason == "unsupported_reasoning_model"));
    }

    #[test]
    fn canonical_replay_reasoning_is_noop() {
        let cx = resolved("deepseek-v4-flash", "deepseek/deepseek-v4-flash");
        let mut msg = json!({"role": "assistant"});
        let blocks = vec![
            ContentBlock::Thinking {
                text: Some("hidden".into()),
                signature: None,
                provider_details: None,
            },
            ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "f".into(),
                input: json!({}),
            },
        ];
        NoProfile.replay_reasoning(&cx, &mut msg, &blocks);
        // Canonical Chat drops Thinking — no reasoning_content injected.
        assert!(msg.get("reasoning_content").is_none());
    }
}
