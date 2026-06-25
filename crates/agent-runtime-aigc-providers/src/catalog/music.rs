//! Static AIGC music generation model catalog for discovery — no credentials needed.

use std::sync::LazyLock;

/// An individually enumerable music generation model.
#[derive(Debug, Clone)]
pub struct MusicModelEntry {
    pub model_id: &'static str,
    pub provider: &'static str,
    pub display_name: &'static str,
}

/// Top-level metadata about a music generation provider.
#[derive(Debug, Clone)]
pub struct MusicProviderInfo {
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub models: Vec<MusicModelEntry>,
}

static MUSIC_PROVIDERS: LazyLock<Vec<MusicProviderInfo>> = LazyLock::new(build_music_catalog);

fn minimax_music_models() -> MusicProviderInfo {
    // Source: docs/external/minimax/music/generation.md `model` enum (4 ids).
    // API: POST https://api.minimaxi.com/v1/music_generation
    // Auth: MINIMAX_API_KEY (Bearer)
    let models = vec![
        MusicModelEntry {
            model_id: "music-2.6",
            provider: "minimax",
            display_name: "MiniMax Music 2.6",
        },
        MusicModelEntry {
            model_id: "music-cover",
            provider: "minimax",
            display_name: "MiniMax Music Cover",
        },
        MusicModelEntry {
            model_id: "music-2.6-free",
            provider: "minimax",
            display_name: "MiniMax Music 2.6 (Free)",
        },
        MusicModelEntry {
            model_id: "music-cover-free",
            provider: "minimax",
            display_name: "MiniMax Music Cover (Free)",
        },
    ];
    MusicProviderInfo {
        provider_id: "minimax",
        display_name: "MiniMax Music (generation + cover)",
        models,
    }
}

fn build_music_catalog() -> Vec<MusicProviderInfo> {
    vec![minimax_music_models()]
}

pub fn list_music_providers() -> &'static [MusicProviderInfo] {
    &MUSIC_PROVIDERS
}

pub fn list_music_models() -> impl Iterator<Item = &'static MusicModelEntry> {
    MUSIC_PROVIDERS.iter().flat_map(|p| p.models.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_catalog_has_four_minimax_models() {
        let ids: Vec<_> = list_music_models().map(|m| m.model_id).collect();
        for expected in [
            "music-2.6",
            "music-cover",
            "music-2.6-free",
            "music-cover-free",
        ] {
            assert!(ids.contains(&expected), "missing {expected}: {ids:?}");
        }
        assert_eq!(ids.len(), 4);
    }
}
