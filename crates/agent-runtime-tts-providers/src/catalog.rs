//! Static TTS model catalog for discovery — no credentials needed.
//!
//! Use `list_models()` to see all available TTS models and their capability summary.

use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Summary of what a TTS model supports, for display and routing decisions.
#[derive(Debug, Clone)]
pub struct TtsModelCapabilitiesSummary {
    pub batch_synthesis: bool,
    pub single_streaming: bool,
    pub duplex_streaming: bool,
    pub supports_instruction: bool,
    pub supports_emotion: bool,
    pub supports_style: bool,
    pub supports_ssml: bool,
    pub supports_cloning: bool,
    pub supports_design: bool,
}

/// A statically known TTS model.
#[derive(Debug, Clone)]
pub struct TtsModelEntry {
    /// Full model ID to put in `TtsSynthesizeRequest` or provider config.
    /// Always in `"provider/model-name"` form.
    pub model_id: &'static str,
    pub provider: &'static str,
    pub display_name: &'static str,
    pub languages: &'static [&'static str],
    pub capabilities: TtsModelCapabilitiesSummary,
}

// ---------------------------------------------------------------------------
// Static catalog
// ---------------------------------------------------------------------------

static TTS_MODELS: LazyLock<Vec<TtsModelEntry>> = LazyLock::new(build_catalog);

fn build_catalog() -> Vec<TtsModelEntry> {
    vec![
        // --- Aliyun CosyVoice ---
        TtsModelEntry {
            model_id: "aliyun/cosyvoice-v3-flash",
            provider: "aliyun",
            display_name: "Aliyun CosyVoice V3 Flash",
            languages: &["zh-CN", "en", "ja", "ko"],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                supports_instruction: true,
                supports_emotion: false,
                supports_style: false,
                supports_ssml: false,
                supports_cloning: true,
                supports_design: true,
            },
        },
        TtsModelEntry {
            model_id: "aliyun/cosyvoice-v3.5-flash",
            provider: "aliyun",
            display_name: "Aliyun CosyVoice V3.5 Flash",
            languages: &[
                "zh-CN", "en", "fr", "de", "ja", "ko", "ru", "pt", "th", "id", "vi",
            ],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                supports_instruction: true,
                supports_emotion: false,
                supports_style: false,
                supports_ssml: false,
                supports_cloning: true,
                supports_design: true,
            },
        },
        TtsModelEntry {
            model_id: "aliyun/cosyvoice-v3.5-plus",
            provider: "aliyun",
            display_name: "Aliyun CosyVoice V3.5 Plus",
            languages: &[
                "zh-CN", "en", "fr", "de", "ja", "ko", "ru", "pt", "th", "id", "vi",
            ],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                supports_instruction: true,
                supports_emotion: false,
                supports_style: false,
                supports_ssml: false,
                supports_cloning: true,
                supports_design: true,
            },
        },
        // --- Aliyun Qwen3-TTS ---
        TtsModelEntry {
            model_id: "aliyun/qwen3-tts-flash-realtime",
            provider: "aliyun",
            display_name: "Aliyun Qwen3-TTS Flash Realtime",
            languages: &[
                "zh-CN", "en", "fr", "de", "it", "pt", "es", "ja", "ko", "ru",
            ],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: false,
                single_streaming: false,
                duplex_streaming: true,
                supports_instruction: false,
                supports_emotion: false,
                supports_style: false,
                supports_ssml: false,
                supports_cloning: false,
                supports_design: false,
            },
        },
        TtsModelEntry {
            model_id: "aliyun/qwen3-tts-instruct-flash-realtime",
            provider: "aliyun",
            display_name: "Aliyun Qwen3-TTS Instruct Flash Realtime",
            languages: &[
                "zh-CN", "en", "fr", "de", "it", "pt", "es", "ja", "ko", "ru",
            ],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: false,
                single_streaming: false,
                duplex_streaming: true,
                supports_instruction: true,
                supports_emotion: false,
                supports_style: false,
                supports_ssml: false,
                supports_cloning: false,
                supports_design: false,
            },
        },
        // --- Volcengine Seed-TTS ---
        TtsModelEntry {
            model_id: "volcengine/seed-tts-1.0",
            provider: "volcengine",
            display_name: "Volcengine Seed-TTS 1.0 (per-hour billing)",
            languages: &["zh-CN", "en"],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                supports_instruction: true,
                supports_emotion: true,
                supports_style: false,
                supports_ssml: false,
                supports_cloning: false,
                supports_design: false,
            },
        },
        TtsModelEntry {
            model_id: "volcengine/seed-tts-1.0-concurr",
            provider: "volcengine",
            display_name: "Volcengine Seed-TTS 1.0 (concurrent billing)",
            languages: &["zh-CN", "en"],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                supports_instruction: true,
                supports_emotion: true,
                supports_style: false,
                supports_ssml: false,
                supports_cloning: false,
                supports_design: false,
            },
        },
        TtsModelEntry {
            model_id: "volcengine/seed-tts-2.0",
            provider: "volcengine",
            display_name: "Volcengine Seed-TTS 2.0",
            languages: &["zh-CN", "en"],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                supports_instruction: true,
                supports_emotion: true,
                supports_style: false,
                supports_ssml: false,
                supports_cloning: false,
                supports_design: false,
            },
        },
        // --- Minimax speech series (issue 004) ---
        // 同步 WSS (`/ws/v1/t2a_v2`) + 异步 HTTP (`/v1/t2a_async_v2`)。
        // 8 models per docs/external/minimax/tts_sync.md model enum.
        TtsModelEntry {
            model_id: "minimax/speech-2.8-hd",
            provider: "minimax",
            display_name: "MiniMax Speech 2.8 HD",
            languages: &["zh-CN", "en", "ja", "ko"],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                supports_instruction: false,
                supports_emotion: true,
                supports_style: false,
                supports_ssml: true,
                supports_cloning: true,
                supports_design: true,
            },
        },
        TtsModelEntry {
            model_id: "minimax/speech-2.8-turbo",
            provider: "minimax",
            display_name: "MiniMax Speech 2.8 Turbo",
            languages: &["zh-CN", "en", "ja", "ko"],
            capabilities: TtsModelCapabilitiesSummary {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                supports_instruction: false,
                supports_emotion: true,
                supports_style: false,
                supports_ssml: true,
                supports_cloning: true,
                supports_design: true,
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns all known TTS models.
pub fn list_models() -> &'static [TtsModelEntry] {
    &TTS_MODELS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_expected_count() {
        assert_eq!(list_models().len(), 10);
    }

    #[test]
    fn aliyun_models_present() {
        let ids: Vec<_> = list_models()
            .iter()
            .filter(|m| m.provider == "aliyun")
            .map(|m| m.model_id)
            .collect();
        assert!(ids.contains(&"aliyun/cosyvoice-v3.5-plus"));
        assert!(ids.contains(&"aliyun/qwen3-tts-instruct-flash-realtime"));
    }

    #[test]
    fn volcengine_models_present() {
        let ids: Vec<_> = list_models()
            .iter()
            .filter(|m| m.provider == "volcengine")
            .map(|m| m.model_id)
            .collect();
        assert!(ids.contains(&"volcengine/seed-tts-2.0"));
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn instruct_models_support_instruction() {
        let instruct = list_models()
            .iter()
            .find(|m| m.model_id == "aliyun/qwen3-tts-instruct-flash-realtime")
            .unwrap();
        assert!(instruct.capabilities.supports_instruction);

        let plain = list_models()
            .iter()
            .find(|m| m.model_id == "aliyun/qwen3-tts-flash-realtime")
            .unwrap();
        assert!(!plain.capabilities.supports_instruction);
    }
}

#[cfg(test)]
mod minimax_tests {
    use super::*;

    #[test]
    fn minimax_speech_models_present() {
        let ids: Vec<_> = list_models()
            .iter()
            .filter(|m| m.provider == "minimax")
            .map(|m| m.model_id)
            .collect();
        assert!(ids.contains(&"minimax/speech-2.8-hd"));
        assert!(ids.contains(&"minimax/speech-2.8-turbo"));
    }
}
