//! ADR-0002 protocol layer: wire-dialect factories decoupled from provider identity.
//!
//! After the 2026-07-12 hotfix (Phases 1–2) all six LLM providers plus the Elss
//! gateway route through a [`ProtocolFactory`] — Chat ([`ChatProtocolFactory`]) or
//! Messages ([`MessagesProtocolFactory`]) — selected from the resolved
//! [`ProviderEntry`]. Per-provider behavioral residual rides in a
//! [`ProviderProfile`] attached to the entry; canonical behavior is the hook
//! defaults. During this transitional (wrapping) phase each factory delegates
//! construction to the entry's [`AdapterCtor`], which wraps the provider's
//! existing adapter; v0.12 collapses those adapters into the protocol cores and
//! removes the ctor indirection and the legacy `ProviderFactory` bridge.
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
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock,
    ModelCapabilities, ModelError, OptionAdjustment, ReasoningCapability, RequestOptions, Role,
    ThinkingLevel, TokenUsage,
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
    /// OpenAI Responses API (`/v1/responses`). Parseable/routable identifier —
    /// stateful, out of scope for Phases 1–2, no factory behind it (ADR "Caution
    /// on Responses").
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
    /// endpoint. The rule is base URL + idempotent append of the canonical path;
    /// today each wrapped adapter's `normalize_*_url` implements it (with lenient
    /// `/v1`//`/v3` handling). Physical consolidation into one resolver lands with
    /// the v0.12 adapter collapse, where the lenient forms can change together.
    #[allow(dead_code)] // consumed by URL resolution at the v0.12 collapse
    pub default_base_url: &'static str,
    pub default_api_key_env: &'static str,
    /// Protocols this provider supports, in preference order. The first is the
    /// default when the model string carries no explicit protocol segment
    /// (explicit selection lands in slice 008).
    pub protocols: &'static [Protocol],
    /// Provider-scoped aliases for the protocol segment of the model string
    /// (e.g. elss: `("anthropic", Messages)`, `("openai", Chat)`). Read by
    /// [`recognize_protocol`].
    pub protocol_aliases: &'static [(&'static str, Protocol)],
    /// Per-protocol endpoint path overrides for non-standard layouts
    /// (e.g. minimax: `(Messages, "/anthropic/v1/messages")`). The wrapped
    /// adapter's `normalize_messages_url` produces the path today; factory-driven
    /// URL resolution consumes this at the v0.12 collapse (see `default_base_url`).
    #[allow(dead_code)] // consumed by URL resolution at the v0.12 collapse
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
    /// The resolved provider entry — carries the adapter ctor, profiles, and
    /// URL/header data. Read by the factory dispatch and profile lookup. Entries
    /// are `'static`, so an adapter can retain this.
    pub provider: &'static ProviderEntry,
    /// The resolved protocol. Read by ctors that dispatch on it (e.g. the Elss
    /// gateway builds the Messages or Chat adapter accordingly).
    pub protocol: Protocol,
    /// Bare model name (provider prefix stripped).
    pub model: &'a str,
    /// The model's catalog entry — the canonical source of capability facts.
    /// `None` for dynamic-gateway models that cannot be enumerated statically.
    /// Catalog rows are `'static`, so an adapter can retain this.
    pub catalog: Option<&'static LlmModelEntry>,
}

/// Narrow, named extension surface for provider-specific behavior *within* a
/// Chat protocol — the home of Problem 5's residual. Every hook has a default =
/// the protocol-canonical behavior; a profile overrides only what its provider
/// actually deviates on.
///
/// Hooks are added **by name, one at a time, when a real provider demonstrates
/// the need** (ADR rule 3): `lower_options` + `replay_reasoning` (DeepSeek),
/// `option_support` (Volcengine), `interpret_usage` (OpenRouter), `messages_wire_role`
/// (Minimax). `normalize_error` from the ADR sketch is intentionally NOT here —
/// no provider has needed it yet, so it stays unwritten. There is deliberately
/// **no** generic `modify_request(&mut body)` escape hatch: each hook's scope is
/// its name.
pub trait ProviderProfile: Send + Sync {
    /// Lower canonical request options (thinking level, sampling) onto the wire
    /// body, reporting any degradation as `OptionAdjustment`s. Default:
    /// Chat-canonical `reasoning_effort` lowering, with reasoning support read
    /// from `cx.catalog` (ADR "Capability metadata"). DeepSeek overrides this
    /// with its top-level `thinking: {type}` dialect.
    fn lower_options(
        &self,
        _cx: &ResolvedModel<'_>,
        options: &RequestOptions,
        body: &mut Value,
    ) -> Vec<OptionAdjustment> {
        // `options` are the *effective* options: `resolve_chat_preflight` has
        // already errored/degraded unsupported reasoning + budget, so thinking is
        // Off for models that don't support it and those adjustments are recorded
        // upstream — this default only shapes the body.
        let mut adjustments = Vec::new();

        if options.thinking != ThinkingLevel::Off {
            let effort = match options.thinking {
                ThinkingLevel::Off => unreachable!(),
                ThinkingLevel::Minimal | ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High | ThinkingLevel::XHigh | ThinkingLevel::Max => "high",
                // Levels added to the protocol later map to the middle.
                _ => "medium",
            };
            body["reasoning_effort"] = json!(effort);
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
    /// OpenAI does). DeepSeek/Volcengine override this to emit `reasoning_content`;
    /// OpenRouter emits `reasoning_details` and may error on invalid replay data
    /// (hence fallible).
    #[allow(clippy::result_large_err)] // justified: ModelError carries diagnostic context (workspace convention)
    fn replay_reasoning(
        &self,
        _cx: &ResolvedModel<'_>,
        _assistant_msg: &mut Value,
        _blocks: &[ContentBlock],
    ) -> Result<(), ModelError> {
        Ok(())
    }

    /// Declare support for a canonical option so shared `CompatibilityPolicy`
    /// handling can degrade or error uniformly instead of each adapter carrying
    /// its own branch. Default: permissive — the catalog schema carries no
    /// output-exclusion flag yet, so a provider that cannot honor an option
    /// declares it `Unsupported` explicitly (Volcengine, slice 003).
    fn option_support(&self, _cx: &ResolvedModel<'_>, _option: RequestOption) -> OptionSupport {
        OptionSupport::Supported
    }

    /// Map a canonical non-System role onto the provider-accepted Messages wire
    /// role, recording any downgrade as an `OptionAdjustment`. Default: the
    /// canonical Messages mapping (Anthropic) — Minimax-only roles are dropped to
    /// `user` with a `minimax_only_role_dropped` adjustment. Minimax overrides to
    /// emit its native roles (`user_system` / `group` / `sample_message_*`) with
    /// no adjustment.
    fn messages_wire_role(
        &self,
        _cx: &ResolvedModel<'_>,
        role: &Role,
        adjustments: &mut Vec<OptionAdjustment>,
    ) -> &'static str {
        match role {
            Role::User | Role::Tool => "user",
            Role::Assistant => "assistant",
            Role::System => "user", // System is handled separately by the core.
            Role::UserSystem => {
                adjustments.push(OptionAdjustment {
                    option: "role".into(),
                    requested: json!("user_system"),
                    applied: json!("user"),
                    reason: "minimax_only_role_dropped".into(),
                });
                "user"
            }
            // Group / SampleMessageUser / SampleMessageAi, and roles added to
            // the protocol later (`Role` is `#[non_exhaustive]`).
            _ => {
                adjustments.push(OptionAdjustment {
                    option: "role".into(),
                    requested: json!(format!("{role:?}")),
                    applied: json!("user"),
                    reason: "minimax_only_role_dropped".into(),
                });
                "user"
            }
        }
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

    /// The Chat SSE reasoning field names `(reasoning_delta_field,
    /// reasoning_details_field)` passed to the SSE decoder. Default `(None, None)`
    /// — canonical Chat (OpenAI) carries no separate reasoning stream. DeepSeek
    /// (`reasoning`), Volcengine (`reasoning_content`), and OpenRouter
    /// (`reasoning` + `reasoning_details`) override this.
    fn chat_sse_reasoning(
        &self,
        _cx: &ResolvedModel<'_>,
    ) -> (Option<&'static str>, Option<&'static str>) {
        (None, None)
    }

    /// Whether the model accepts image input as canonical Chat `image_url`
    /// content parts. Encoding stays in the shared Chat core (it is the
    /// protocol-canonical OpenAI shape, not a dialect fork); the profile only
    /// declares the capability fact, preferably from `cx.catalog`.
    ///
    /// Default: [`ImageInputSupport::Unsupported`] with no strict error — the
    /// core drops each image visibly (`chat_unsupported_content_block`) under
    /// every policy. DeepSeek overrides for its vision models.
    fn chat_image_input(&self, _cx: &ResolvedModel<'_>) -> ImageInputSupport {
        ImageInputSupport::Unsupported { strict_error: None }
    }

    /// Report the model's capabilities. Default: the catalog-driven canonical Chat
    /// capabilities. Providers whose reasoning-capability detail (effort list,
    /// budget/exclusion flags, replay-metadata, source, pricing, name-prefix
    /// fallback) is not fully captured by the catalog override this.
    fn capabilities(&self, cx: &ResolvedModel<'_>, max_output_tokens: u32) -> ModelCapabilities {
        canonical_chat_capabilities(cx, max_output_tokens)
    }

    // -- Messages-protocol hooks (used by the shared MessagesAdapter) -------

    /// Whether the model uses adaptive thinking (`thinking: {type: adaptive}` +
    /// `output_config`) vs. explicit `budget_tokens`. Default `false`. Anthropic
    /// and Minimax override with their model checks.
    fn messages_supports_adaptive(&self, _cx: &ResolvedModel<'_>) -> bool {
        false
    }

    /// Encode a **multimodal** content block (`Image` / `Video` / `Audio` /
    /// `MidConvSystem`) for the Messages wire; the shared core encodes the common
    /// blocks (text / thinking / tool_use / tool_result). Return `Some(json)` to
    /// include it, or `None` to drop it (pushing an `OptionAdjustment`). This is
    /// the narrow, named home for the per-provider content-encoding divergence
    /// (ADR rule 4 dialect-fork consideration — see the v0.12 slice 002 spec).
    fn encode_multimodal_block(
        &self,
        _cx: &ResolvedModel<'_>,
        _block: &ContentBlock,
        _adjustments: &mut Vec<OptionAdjustment>,
    ) -> Option<Value> {
        None
    }

    /// The Messages auth headers for `api_key`. Default: `Authorization: Bearer`.
    /// Anthropic overrides with `x-api-key` + `anthropic-version`.
    fn messages_auth_headers(
        &self,
        _cx: &ResolvedModel<'_>,
        api_key: &str,
    ) -> Vec<(&'static str, String)> {
        vec![("authorization", format!("Bearer {api_key}"))]
    }
}

/// Canonical Chat capabilities read from the catalog (the
/// [`ProviderProfile::capabilities`] default). Reasoning support / context window
/// come from `cx.catalog`; when absent (dynamic/unlisted models) reasoning is
/// assumed unsupported and the context window unknown — providers with a
/// name-prefix fallback override.
pub fn canonical_chat_capabilities(
    cx: &ResolvedModel<'_>,
    max_output_tokens: u32,
) -> ModelCapabilities {
    let supports_reasoning = cx.catalog.map(|c| c.thinking.is_some()).unwrap_or(false);
    ModelCapabilities {
        streaming: true,
        tool_use: true,
        parallel_tool_use: true,
        reasoning: ReasoningCapability {
            supported: supports_reasoning,
            efforts: if supports_reasoning {
                vec![
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                ]
            } else {
                vec![]
            },
            budget_tokens: false,
            output_exclusion: false,
            replay_metadata_required: false,
        },
        prompt_cache: CacheCapability {
            supported: true,
            explicit_breakpoints: false,
            long_ttl: false,
        },
        max_output_tokens: Some(max_output_tokens),
        context_window_size: cx.catalog.map(|c| c.context_window),
        source: CapabilitySource::Static,
        pricing: cx.catalog.and_then(|c| c.pricing.clone()),
    }
}

/// A profile that overrides nothing — the protocol-canonical behavior. Used as
/// the Chat core's fallback when an entry declares no profile.
pub struct CanonicalChat;
impl ProviderProfile for CanonicalChat {}
/// The shared no-op profile singleton.
pub static CANONICAL_CHAT: CanonicalChat = CanonicalChat;

/// The Messages-core fallback profile (canonical Messages hook defaults). Used
/// only when an entry declares no Messages profile; Anthropic/Minimax always do.
pub struct CanonicalMessages;
impl ProviderProfile for CanonicalMessages {}
/// The shared Messages fallback profile singleton.
pub static CANONICAL_MESSAGES: CanonicalMessages = CanonicalMessages;

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
/// [`ProviderProfile::option_support`] and applied uniformly by
/// [`resolve_chat_preflight`]. Added by name as providers demonstrate the need
/// (ADR rule 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOption {
    /// Reasoning/thinking itself — a model may not support it at all.
    Reasoning,
    /// A caller-specified thinking token budget (`thinking_budget_tokens`).
    ThinkingBudget,
    /// Emit reasoning internally but exclude it from the response
    /// (`include_thinking: false` while thinking is enabled).
    ReasoningOutputExclusion,
}

/// Chat image-input support declared by [`ProviderProfile::chat_image_input`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageInputSupport {
    /// `Image` blocks in user messages are encoded as `image_url` parts.
    Supported,
    /// `Image` blocks are dropped and recorded. With `strict_error` set,
    /// [`resolve_chat_content_preflight`] fails a `Strict` request that carries
    /// an image with that `(code, message)` before anything is sent.
    Unsupported {
        strict_error: Option<(&'static str, &'static str)>,
    },
}

/// The value recorded in a degradation [`OptionAdjustment`]'s `applied` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliedValue {
    Null,
    Bool(bool),
    Str(&'static str),
}

impl AppliedValue {
    fn to_json(self) -> Value {
        match self {
            AppliedValue::Null => Value::Null,
            AppliedValue::Bool(b) => json!(b),
            AppliedValue::Str(s) => json!(s),
        }
    }
}

/// The `OptionAdjustment` a provider records when it degrades an unsupported
/// option. `requested` is supplied by [`resolve_chat_preflight`] from the actual
/// value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustmentSpec {
    pub option: &'static str,
    pub applied: AppliedValue,
    pub reason: &'static str,
}

/// The result of a [`ProviderProfile::option_support`] query. `Unsupported`
/// carries the provider's exact behavior as data, so [`resolve_chat_preflight`]
/// reproduces each provider's Strict/degrade semantics and reason strings without
/// a per-provider branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionSupport {
    Supported,
    Unsupported {
        /// Error `(code, message)` surfaced under `CompatibilityPolicy::Strict`.
        /// `None` → Strict degrades silently like Coerce (e.g. Volcengine
        /// disabling thinking on an unlisted model raises no error).
        strict_error: Option<(&'static str, &'static str)>,
        /// Whether degrading turns thinking off (reasoning / exclusion) rather
        /// than merely dropping the option (thinking budget).
        disables_thinking: bool,
        /// The adjustment recorded when degrading. `None` → degrade silently
        /// (e.g. Volcengine disabling thinking records no adjustment).
        adjustment: Option<AdjustmentSpec>,
    },
}

/// Apply the shared `CompatibilityPolicy` handling for the reasoning/budget/
/// exclusion options, driven by the profile's [`ProviderProfile::option_support`]
/// declarations — so the Strict-errors / degrade decision lives here once rather
/// than in each adapter. Returns the effective options (thinking possibly turned
/// off) plus the degradation adjustments, or an error under Strict.
#[allow(clippy::result_large_err)] // justified: ModelError carries diagnostic context (workspace convention)
pub fn resolve_chat_preflight(
    profile: &dyn ProviderProfile,
    cx: &ResolvedModel<'_>,
    options: &RequestOptions,
) -> Result<(RequestOptions, Vec<OptionAdjustment>), ModelError> {
    let mut effective = options.clone();
    let mut adjustments = Vec::new();

    if options.thinking != ThinkingLevel::Off {
        apply_option(
            profile,
            cx,
            RequestOption::Reasoning,
            options.compatibility_policy,
            json!(format!("{:?}", options.thinking)),
            &mut effective,
            &mut adjustments,
        )?;
    }

    if options.thinking_budget_tokens.is_some() {
        apply_option(
            profile,
            cx,
            RequestOption::ThinkingBudget,
            options.compatibility_policy,
            json!(options.thinking_budget_tokens),
            &mut effective,
            &mut adjustments,
        )?;
    }

    // Exclusion only matters when thinking is (still) on but its output is off.
    if !options.include_thinking && effective.thinking != ThinkingLevel::Off {
        apply_option(
            profile,
            cx,
            RequestOption::ReasoningOutputExclusion,
            options.compatibility_policy,
            json!(false),
            &mut effective,
            &mut adjustments,
        )?;
    }

    Ok((effective, adjustments))
}

/// Content-side Chat pre-flight: a `Strict` request carrying an `Image` block
/// fails before any request is built or sent when the profile declares the model
/// cannot take images and names a strict error. `Coerce` (and profiles without a
/// strict error) fall through to the core's visible per-block drop.
#[allow(clippy::result_large_err)] // justified: ModelError carries diagnostic context (workspace convention)
pub fn resolve_chat_content_preflight(
    profile: &dyn ProviderProfile,
    cx: &ResolvedModel<'_>,
    messages: &[crate::Message],
    policy: CompatibilityPolicy,
) -> Result<(), ModelError> {
    if policy != CompatibilityPolicy::Strict {
        return Ok(());
    }
    let ImageInputSupport::Unsupported {
        strict_error: Some((code, message)),
    } = profile.chat_image_input(cx)
    else {
        return Ok(());
    };
    let has_image = messages
        .iter()
        .flat_map(|m| m.content.iter())
        .any(|b| matches!(b, ContentBlock::Image { .. }));
    if has_image {
        return Err(ModelError {
            message: message.into(),
            code: Some(code.into()),
            provider: Some(cx.provider.name.into()),
            status: None,
            retry_after_secs: None,
            upstream: None,
        });
    }
    Ok(())
}

#[allow(clippy::result_large_err, clippy::too_many_arguments)] // justified: ModelError carries diagnostic context; args are the shared option-application inputs
fn apply_option(
    profile: &dyn ProviderProfile,
    cx: &ResolvedModel<'_>,
    option: RequestOption,
    policy: CompatibilityPolicy,
    requested: Value,
    effective: &mut RequestOptions,
    adjustments: &mut Vec<OptionAdjustment>,
) -> Result<(), ModelError> {
    let OptionSupport::Unsupported {
        strict_error,
        disables_thinking,
        adjustment,
    } = profile.option_support(cx, option)
    else {
        return Ok(());
    };

    if policy == CompatibilityPolicy::Strict {
        if let Some((code, message)) = strict_error {
            return Err(ModelError {
                message: message.into(),
                code: Some(code.into()),
                provider: Some(cx.provider.name.into()),
                status: None,
                retry_after_secs: None,
                upstream: None,
            });
        }
    }

    if disables_thinking {
        effective.thinking = ThinkingLevel::Off;
    }
    if let Some(a) = adjustment {
        adjustments.push(OptionAdjustment {
            option: a.option.into(),
            requested,
            applied: a.applied.to_json(),
            reason: a.reason.into(),
        });
    }
    Ok(())
}

/// Canonical Chat stop-reason post-processing: the SSE decoder already maps the
/// standard finish reasons; this folds the extra non-standard mapping
/// (`insufficient_system_resource -> Interrupted`) into the shared path (a no-op
/// for providers that never emit it).
pub fn normalize_chat_stop_reason(stop: crate::StopReason) -> crate::StopReason {
    use crate::StopReason;
    match stop {
        StopReason::Other(raw) if raw == "insufficient_system_resource" => StopReason::Interrupted,
        other => other,
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

/// The built-in provider entries. Every provider resolves through the protocol
/// factories from its entry (there is no longer a legacy fallback).
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
    profiles: &[(Protocol::Chat, &crate::providers::openai::OPENAI_PROFILE)],
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
    // Anthropic is the canonical Messages reference, but the collapsed core still
    // needs its capability facts, x-api-key auth, adaptive detection, and
    // image-only multimodal encoding as profile data.
    profiles: &[(
        Protocol::Messages,
        &crate::providers::anthropic::ANTHROPIC_PROFILE,
    )],
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

static ELSS_ENTRY: ProviderEntry = ProviderEntry {
    name: "elss",
    default_base_url: "https://api.elss.ai",
    default_api_key_env: "ELSS_API_KEY",
    // Elss is a dual-protocol gateway with zero adapter code.
    protocols: &[Protocol::Messages, Protocol::Chat],
    // Provider-scoped aliases keep the shipped three-segment forms working; they
    // are NOT global (openrouter/anthropic/... is unaffected).
    protocol_aliases: &[
        ("anthropic", Protocol::Messages),
        ("openai", Protocol::Chat),
    ],
    path_overrides: &[],
    extra_headers: &[],
    profiles: &[],
    build_adapter: crate::providers::elss::build_adapter,
};

/// Every built-in provider entry — the single source of truth the registry and
/// the parser enumerate.
static ALL_ENTRIES: &[&ProviderEntry] = &[
    &OPENAI_ENTRY,
    &DEEPSEEK_ENTRY,
    &VOLCENGINE_ENTRY,
    &OPENROUTER_ENTRY,
    &ANTHROPIC_ENTRY,
    &MINIMAX_ENTRY,
    &ELSS_ENTRY,
];

/// All built-in provider entries.
pub fn all_provider_entries() -> &'static [&'static ProviderEntry] {
    ALL_ENTRIES
}

/// The provider entry for `name`, or `None` for an unknown provider.
pub fn provider_entry(name: &str) -> Option<&'static ProviderEntry> {
    ALL_ENTRIES.iter().copied().find(|e| e.name == name)
}

/// Recognize an explicit protocol segment in a model string (ADR "Parsing
/// rule"): a canonical protocol name (`messages`/`chat`/`responses`) or a
/// provider-scoped alias from `protocol_aliases`. Returns `None` if the segment
/// is neither — in which case the caller keeps it as part of the model name
/// (this is what preserves `openrouter/<vendor>/<model>`). Canonical names are
/// therefore reserved words for the protocol segment.
pub fn recognize_protocol(provider: &str, segment: &str) -> Option<Protocol> {
    match segment {
        "messages" => Some(Protocol::Messages),
        "chat" => Some(Protocol::Chat),
        "responses" => Some(Protocol::Responses),
        _ => provider_entry(provider).and_then(|e| {
            e.protocol_aliases
                .iter()
                .find(|(alias, _)| *alias == segment)
                .map(|(_, proto)| *proto)
        }),
    }
}

/// Auto-detect the protocol for a model string with no explicit segment (ADR
/// "Auto-detection and precedence"): the model-prefix table (`claude-*` →
/// Messages, everything else → Chat) filtered by the provider's supported
/// protocols, falling back to the provider's first (preferred) protocol when the
/// table's pick is unsupported. `Responses` is never auto-detected.
pub fn auto_detect_protocol(entry: &ProviderEntry, model: &str) -> Option<Protocol> {
    let table_pick = if model.starts_with("claude-") {
        Protocol::Messages
    } else {
        Protocol::Chat
    };
    if entry.protocols.contains(&table_pick) {
        Some(table_pick)
    } else {
        entry.protocols.first().copied()
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
        // deepseek-flash declares thinking support in the catalog.
        let cx = resolved("deepseek-flash", "deepseek/deepseek-flash");
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
    fn canonical_lower_options_emits_reasoning_effort_without_a_support_gate() {
        // The canonical default no longer gates on model support — that moved to
        // `option_support` + `resolve_chat_preflight`. `lower_options` only shapes
        // the body; the pre-flight would have disabled thinking upstream for a
        // provider that doesn't support it. The canonical `option_support` is
        // permissive (Supported).
        let cx = ResolvedModel {
            provider: provider_entry("openai").unwrap(),
            protocol: Protocol::Chat,
            model: "made-up-model",
            catalog: None,
        };
        let opts = RequestOptions {
            thinking: ThinkingLevel::High,
            ..Default::default()
        };
        let mut body = json!({});
        let adj = NoProfile.lower_options(&cx, &opts, &mut body);
        assert_eq!(body["reasoning_effort"], "high");
        assert!(adj.is_empty());
        assert!(matches!(
            NoProfile.option_support(&cx, RequestOption::Reasoning),
            OptionSupport::Supported
        ));
    }

    #[test]
    fn canonical_replay_reasoning_is_noop() {
        let cx = resolved("deepseek-flash", "deepseek/deepseek-flash");
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
        NoProfile.replay_reasoning(&cx, &mut msg, &blocks).unwrap();
        // Canonical Chat drops Thinking — no reasoning_content injected.
        assert!(msg.get("reasoning_content").is_none());
    }
}
