//! Multi-capability model catalog materialization (Batch 0).
//!
//! Tables live in impl crates; this wall module projects them into a process-
//! global index for credential-free discovery (`list_models` / `find_model*`).

use std::sync::LazyLock;

use orchest_protocol::Capability;
#[cfg(feature = "http")]
use orchest_protocol::Modality;
#[cfg(feature = "http")]
use orchest_provider_core::catalog::ChatCatalogExt;

// Re-export discovery types used by consumers via `orchest_provider::catalog`.
pub use orchest_provider_core::catalog::{CatalogExt, ModelFilter, ModelRecord, ModelStatus};

static CATALOG: LazyLock<Vec<ModelRecord>> = LazyLock::new(build_catalog);

fn build_catalog() -> Vec<ModelRecord> {
    #[cfg(any(feature = "http", feature = "stream"))]
    {
        let mut out = Vec::new();

        #[cfg(feature = "http")]
        {
            for m in orchest_provider_http::catalog::list_models() {
                out.push(project_llm(m));
            }
            out.extend(orchest_provider_http::decision_models().iter().cloned());
            out.extend(
                orchest_provider_http::catalog::http_asr_models()
                    .iter()
                    .cloned(),
            );
        }

        #[cfg(feature = "stream")]
        {
            out.extend(
                orchest_provider_stream::catalog::stream_asr_models()
                    .iter()
                    .cloned(),
            );
        }

        out
    }
    #[cfg(not(any(feature = "http", feature = "stream")))]
    {
        Vec::new()
    }
}

#[cfg(feature = "http")]
fn project_llm(m: &orchest_provider_http::catalog::LlmModelEntry) -> ModelRecord {
    let model = m
        .model_id
        .split_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(m.model_id);
    ModelRecord {
        id: m.model_id,
        provider: m.provider,
        model,
        capability: Capability::Chat,
        display_name: m.display_name,
        description: m.description,
        input_modalities: map_modalities(m.input_modalities),
        output_modalities: map_modalities(m.output_modalities),
        streaming: true,
        duplex: false,
        interruptible: false,
        tools: true,
        thinking: m.thinking.is_some(),
        status: ModelStatus::Stable,
        // No Chat defaults until Batch 4.
        default_for_provider: false,
        pricing: m.pricing.clone(),
        ext: CatalogExt::Chat(ChatCatalogExt {
            context_window: m.context_window,
            max_output_tokens: m.max_output_tokens,
            max_input_tokens: m.max_input_tokens,
            thinking_max_tokens: m.thinking.and_then(|t| t.max_thinking_tokens),
        }),
    }
}

#[cfg(feature = "http")]
fn map_modalities(ms: &[orchest_provider_http::catalog::Modality]) -> Vec<Modality> {
    ms.iter().copied().map(to_proto_modality).collect()
}

#[cfg(feature = "http")]
fn to_proto_modality(m: orchest_provider_http::catalog::Modality) -> Modality {
    use orchest_provider_http::catalog::Modality as CatalogModality;
    match m {
        CatalogModality::Text => Modality::Text,
        CatalogModality::Image => Modality::Image,
        CatalogModality::Video => Modality::Video,
        CatalogModality::Audio => Modality::Audio,
    }
}

/// Credential-free catalog discovery.
pub fn list_models(filter: ModelFilter) -> impl Iterator<Item = &'static ModelRecord> {
    CATALOG.iter().filter(move |r| filter.matches(r))
}

/// Resolve a model id to a catalog row.
///
/// Chat ids (feature `http`) go through ADR-0002 `normalize_provider_model`
/// so protocol segments are peeled. Bare multi-capability ids Chat-prefer.
pub fn find_model(id: &str) -> Option<&'static ModelRecord> {
    if let Some(hit) = resolve_chat_lookup(id, /* require_chat */ false) {
        return Some(hit);
    }
    resolve_simple(id, None)
}

/// Resolve a model id constrained to a single capability.
pub fn find_model_for(id: &str, capability: Capability) -> Option<&'static ModelRecord> {
    if capability == Capability::Chat {
        if let Some(hit) = resolve_chat_lookup(id, /* require_chat */ true) {
            return Some(hit);
        }
    }
    resolve_simple(id, Some(capability))
}

/// Chat path: normalize via ADR-0002 when `http` is enabled.
///
/// `require_chat` forces capability == Chat (for `find_model_for`); bare
/// `find_model` still prefers Chat rows from this path but may later fall
/// through to multi-cap simple resolution.
#[cfg(feature = "http")]
fn resolve_chat_lookup(id: &str, require_chat: bool) -> Option<&'static ModelRecord> {
    match orchest_provider_http::normalize_provider_model(id) {
        Ok(n) => {
            let mut chat_hit = None;
            let mut any_hit = None;
            for r in CATALOG.iter() {
                if r.provider == n.provider && r.model == n.model {
                    if r.capability == Capability::Chat {
                        chat_hit = Some(r);
                        break;
                    }
                    if !require_chat && any_hit.is_none() {
                        any_hit = Some(r);
                    }
                }
            }
            chat_hit.or(if require_chat { None } else { any_hit })
        }
        Err(_) => None, // fall back to non-Chat simple split
    }
}

#[cfg(not(feature = "http"))]
fn resolve_chat_lookup(_id: &str, _require_chat: bool) -> Option<&'static ModelRecord> {
    None
}

/// Non-Chat / fallback resolution: first `/` split or bare model match.
///
/// Protocol peel never applies here. For bare `find_model` (no capability
/// constraint), multi-capability hits Chat-prefer.
fn resolve_simple(id: &str, capability: Option<Capability>) -> Option<&'static ModelRecord> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((provider, model)) = trimmed.split_once('/') {
        // First-slash only — never invent protocol segments for non-Chat.
        return CATALOG.iter().find(|r| {
            r.provider == provider
                && r.model == model
                && capability.map(|c| r.capability == c).unwrap_or(true)
        });
    }

    // Bare model id.
    let mut chat_hit = None;
    let mut other_hit = None;
    for r in CATALOG.iter() {
        if r.model != trimmed {
            continue;
        }
        if let Some(c) = capability {
            if r.capability == c {
                return Some(r);
            }
            continue;
        }
        // Unconstrained: Chat-prefer for multi-cap bare ids.
        if r.capability == Capability::Chat {
            chat_hit = Some(r);
        } else if other_hit.is_none() {
            other_hit = Some(r);
        }
    }
    chat_hit.or(other_hit)
}
