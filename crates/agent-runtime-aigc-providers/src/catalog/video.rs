//! Static AIGC video generation model catalog for discovery — no credentials needed.

use std::sync::LazyLock;

/// An individually enumerable video generation model.
#[derive(Debug, Clone)]
pub struct VideoModelEntry {
    /// Full model ID to put in `AigcProviderRuntimeConfig { model: "..." }`.
    pub model_id: &'static str,
    pub provider: &'static str,
    pub display_name: &'static str,
}

/// Top-level metadata about a video generation provider.
#[derive(Debug, Clone)]
pub struct VideoProviderInfo {
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub models: Vec<VideoModelEntry>,
}

static VIDEO_PROVIDERS: LazyLock<Vec<VideoProviderInfo>> = LazyLock::new(build_video_catalog);

fn volcengine_video_models() -> VideoProviderInfo {
    // Source: docs/external/volceengine/aigc/video/create.md
    // API: POST https://ark.cn-beijing.volces.com/api/v3/contents/generations/tasks
    // Auth: ARK_API_KEY (Bearer)
    //
    // NOTE: like the image catalog, the docs reference dotted display names
    // ("Seedance 1.0 Pro", "Seedance 2.0") that are not directly invocable model
    // IDs. Both entries below were confirmed against the live API (2026-06-17,
    // real ARK_API_KEY): a create-task call returned a task id, and the query
    // endpoint matched docs/external/volceengine/aigc/video/query.md exactly.
    let models = vec![
        VideoModelEntry {
            model_id: "doubao-seedance-1-0-pro-250528",
            provider: "volcengine",
            display_name: "Doubao Seedance 1.0 Pro",
        },
        VideoModelEntry {
            model_id: "doubao-seedance-1-5-pro-251215",
            provider: "volcengine",
            display_name: "Doubao Seedance 1.5 Pro",
        },
    ];

    VideoProviderInfo {
        provider_id: "volcengine",
        display_name: "Volcengine (火山引擎 / Doubao Seedance)",
        models,
    }
}

fn build_video_catalog() -> Vec<VideoProviderInfo> {
    vec![volcengine_video_models()]
}

/// Returns metadata for all known video generation providers.
pub fn list_video_providers() -> &'static [VideoProviderInfo] {
    &VIDEO_PROVIDERS
}

/// Returns all individually enumerable video generation models.
pub fn list_video_models() -> impl Iterator<Item = &'static VideoModelEntry> {
    VIDEO_PROVIDERS.iter().flat_map(|p| p.models.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_catalog_has_volcengine_seedance_models() {
        let ids: Vec<_> = list_video_models().map(|m| m.model_id).collect();
        assert!(ids.contains(&"doubao-seedance-1-0-pro-250528"));
        assert!(ids.contains(&"doubao-seedance-1-5-pro-251215"));
    }
}
