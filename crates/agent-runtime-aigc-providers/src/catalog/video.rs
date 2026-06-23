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

fn minimax_video_models() -> VideoProviderInfo {
    // Source: docs/external/minimax/video/{t2v,i2v,frame2v,refvideo}.md
    // API: POST https://api.minimaxi.com/v1/video_generation
    // Auth: MINIMAX_API_KEY (Bearer)
    //
    // 9 个 model id 来自 video/{t2v,i2v,frame2v,refvideo}.md 的 model enum:
    // T2V/I2V/Frame2V 共享 Hailuo + T2V/I2V-01 系列;Subject Reference 仅 S2V-01。
    let models = vec![
        VideoModelEntry {
            model_id: "MiniMax-Hailuo-2.3",
            provider: "minimax",
            display_name: "MiniMax Hailuo 2.3",
        },
        VideoModelEntry {
            model_id: "MiniMax-Hailuo-2.3-Fast",
            provider: "minimax",
            display_name: "MiniMax Hailuo 2.3 Fast",
        },
        VideoModelEntry {
            model_id: "MiniMax-Hailuo-02",
            provider: "minimax",
            display_name: "MiniMax Hailuo 02",
        },
        VideoModelEntry {
            model_id: "T2V-01-Director",
            provider: "minimax",
            display_name: "T2V-01 Director",
        },
        VideoModelEntry {
            model_id: "T2V-01",
            provider: "minimax",
            display_name: "T2V-01",
        },
        VideoModelEntry {
            model_id: "I2V-01-Director",
            provider: "minimax",
            display_name: "I2V-01 Director",
        },
        VideoModelEntry {
            model_id: "I2V-01-live",
            provider: "minimax",
            display_name: "I2V-01 Live",
        },
        VideoModelEntry {
            model_id: "I2V-01",
            provider: "minimax",
            display_name: "I2V-01",
        },
        VideoModelEntry {
            model_id: "S2V-01",
            provider: "minimax",
            display_name: "S2V-01 (Subject Reference)",
        },
    ];

    VideoProviderInfo {
        provider_id: "minimax",
        display_name: "MiniMax (Hailuo / T2V / I2V / S2V)",
        models,
    }
}

fn build_video_catalog() -> Vec<VideoProviderInfo> {
    vec![volcengine_video_models(), minimax_video_models()]
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

#[cfg(test)]
mod minimax_tests {
    use super::*;

    #[test]
    fn video_catalog_has_minimax_models() {
        let ids: Vec<_> = list_video_models()
            .filter(|m| m.provider == "minimax")
            .map(|m| m.model_id)
            .collect();
        for expected in [
            "MiniMax-Hailuo-2.3",
            "MiniMax-Hailuo-2.3-Fast",
            "MiniMax-Hailuo-02",
            "T2V-01-Director",
            "T2V-01",
            "I2V-01-Director",
            "I2V-01-live",
            "I2V-01",
            "S2V-01",
        ] {
            assert!(ids.contains(&expected), "missing {expected}: {ids:?}");
        }
        assert_eq!(ids.len(), 9);
    }
}
