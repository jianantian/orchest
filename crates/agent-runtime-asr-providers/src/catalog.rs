//! Static ASR model catalog for discovery — no credentials needed.
//!
//! Use `list_models()` to see all available ASR models and their capabilities summary.

use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Summary of what an ASR model supports, for display and routing decisions.
#[derive(Debug, Clone)]
pub struct AsrModelCapabilitiesSummary {
    pub streaming: bool,
    pub batch: bool,
    pub word_timestamps: bool,
    pub speaker_diarization: bool,
    pub code_switching: bool,
    pub hot_words: bool,
    pub context_prompt: bool,
}

/// A statically known ASR model.
#[derive(Debug, Clone)]
pub struct AsrModelEntry {
    /// Full model ID to put in `AsrProviderRuntimeConfig { model: "..." }`.
    /// Always in `"provider/model-name"` form.
    pub model_id: &'static str,
    pub provider: &'static str,
    pub display_name: &'static str,
    pub languages: &'static [&'static str],
    pub capabilities: AsrModelCapabilitiesSummary,
    /// The provider-side resource identifier that selects this model.
    /// For Volcengine this is the `X-Api-Resource-Id` header value.
    /// `None` for providers that identify models by other means (e.g. URL / request param).
    pub resource_id: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// Static catalog
// ---------------------------------------------------------------------------

static ASR_MODELS: LazyLock<Vec<AsrModelEntry>> = LazyLock::new(build_catalog);

fn build_catalog() -> Vec<AsrModelEntry> {
    vec![
        AsrModelEntry {
            model_id: "soniox/stt-rt-v5",
            provider: "soniox",
            display_name: "Soniox STT Realtime v5",
            languages: &["*", "multi", "en", "es", "fr", "de", "ja", "zh"],
            capabilities: AsrModelCapabilitiesSummary {
                streaming: true,
                batch: false,
                word_timestamps: true,
                speaker_diarization: false,
                code_switching: true,
                hot_words: true,
                context_prompt: true,
            },
            resource_id: None,
        },
        AsrModelEntry {
            model_id: "elevenlabs/scribe_v2_realtime",
            provider: "elevenlabs",
            display_name: "ElevenLabs Scribe v2 Realtime",
            languages: &["auto", "en", "es", "fr", "de", "it", "pt", "ja", "ko", "zh"],
            capabilities: AsrModelCapabilitiesSummary {
                streaming: true,
                batch: false,
                word_timestamps: true,
                speaker_diarization: false,
                code_switching: false,
                hot_words: true,
                context_prompt: false,
            },
            resource_id: None,
        },
        AsrModelEntry {
            model_id: "elevenlabs/scribe_v2",
            provider: "elevenlabs",
            display_name: "ElevenLabs Scribe v2 Batch",
            languages: &["auto", "en", "es", "fr", "de", "it", "pt", "ja", "ko", "zh"],
            capabilities: AsrModelCapabilitiesSummary {
                streaming: false,
                batch: true,
                word_timestamps: true,
                speaker_diarization: false,
                code_switching: false,
                hot_words: true,
                context_prompt: false,
            },
            resource_id: None,
        },
        AsrModelEntry {
            model_id: "deepgram/nova-3",
            provider: "deepgram",
            display_name: "Deepgram Nova-3 Realtime",
            languages: &["en", "es", "fr", "de", "it", "pt", "ja", "ko", "zh"],
            capabilities: AsrModelCapabilitiesSummary {
                streaming: true,
                batch: false,
                word_timestamps: true,
                speaker_diarization: false,
                code_switching: false,
                hot_words: true,
                context_prompt: false,
            },
            resource_id: None,
        },
        AsrModelEntry {
            model_id: "aliyun/fun-asr-realtime",
            provider: "aliyun",
            display_name: "Aliyun FunASR Realtime",
            languages: &["zh-CN", "en", "ja"],
            capabilities: AsrModelCapabilitiesSummary {
                streaming: true,
                batch: false,
                word_timestamps: true,
                speaker_diarization: false,
                code_switching: true,
                hot_words: true,
                context_prompt: false,
            },
            resource_id: None,
        },
        AsrModelEntry {
            model_id: "volcengine/bigasr",
            provider: "volcengine",
            display_name: "Volcengine BigASR (Doubao ASR 1.0)",
            languages: &["zh-CN", "en"],
            capabilities: AsrModelCapabilitiesSummary {
                streaming: true,
                batch: false,
                word_timestamps: true,
                speaker_diarization: false,
                code_switching: true,
                hot_words: true,
                context_prompt: true,
            },
            resource_id: Some("volc.bigasr.sauc.duration"),
        },
        AsrModelEntry {
            model_id: "volcengine/seedasr",
            provider: "volcengine",
            display_name: "Volcengine SeedASR (Doubao ASR 2.0)",
            languages: &["zh-CN", "en"],
            capabilities: AsrModelCapabilitiesSummary {
                streaming: true,
                batch: false,
                word_timestamps: true,
                speaker_diarization: false,
                code_switching: true,
                hot_words: true,
                context_prompt: true,
            },
            resource_id: Some("volc.seedasr.sauc.duration"),
        },
    ]
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns all known ASR models.
pub fn list_models() -> &'static [AsrModelEntry] {
    &ASR_MODELS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_seven_models() {
        assert_eq!(list_models().len(), 7);
    }

    #[test]
    fn soniox_stt_rt_v5_present() {
        let entry = list_models()
            .iter()
            .find(|m| m.model_id == "soniox/stt-rt-v5")
            .expect("soniox/stt-rt-v5 should be in catalog");
        assert!(entry.capabilities.streaming);
        assert!(entry.capabilities.code_switching);
        assert!(entry.capabilities.context_prompt);
        assert!(entry.languages.contains(&"*"));
    }

    #[test]
    fn elevenlabs_scribe_models_present() {
        let realtime = list_models()
            .iter()
            .find(|m| m.model_id == "elevenlabs/scribe_v2_realtime")
            .expect("elevenlabs/scribe_v2_realtime should be in catalog");
        assert!(realtime.capabilities.streaming);
        assert!(!realtime.capabilities.batch);
        assert!(realtime.capabilities.word_timestamps);

        let batch = list_models()
            .iter()
            .find(|m| m.model_id == "elevenlabs/scribe_v2")
            .expect("elevenlabs/scribe_v2 should be in catalog");
        assert!(!batch.capabilities.streaming);
        assert!(batch.capabilities.batch);
        assert!(batch.capabilities.word_timestamps);
    }

    #[test]
    fn deepgram_nova_3_present() {
        let entry = list_models()
            .iter()
            .find(|m| m.model_id == "deepgram/nova-3")
            .expect("deepgram/nova-3 should be in catalog");
        assert!(entry.capabilities.streaming);
        assert!(!entry.capabilities.batch);
        assert!(entry.capabilities.word_timestamps);
        assert_eq!(entry.resource_id, None);
    }

    #[test]
    fn aliyun_fun_asr_present() {
        let entry = list_models()
            .iter()
            .find(|m| m.model_id == "aliyun/fun-asr-realtime")
            .expect("aliyun/fun-asr-realtime should be in catalog");
        assert!(entry.capabilities.streaming);
        assert!(entry.capabilities.word_timestamps);
        assert!(entry.resource_id.is_none());
    }

    #[test]
    fn volcengine_bigasr_resource_id() {
        let entry = list_models()
            .iter()
            .find(|m| m.model_id == "volcengine/bigasr")
            .expect("volcengine/bigasr should be in catalog");
        assert_eq!(entry.resource_id, Some("volc.bigasr.sauc.duration"));
    }

    #[test]
    fn volcengine_seedasr_resource_id() {
        let entry = list_models()
            .iter()
            .find(|m| m.model_id == "volcengine/seedasr")
            .expect("volcengine/seedasr should be in catalog");
        assert_eq!(entry.resource_id, Some("volc.seedasr.sauc.duration"));
    }
}
