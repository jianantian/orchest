//! Registry selection tests over **fixture descriptors** (no real impl crate).
//!
//! Proves the one mechanism serves capability-query AND identity-pick and that
//! they mix, and that pick-one / list-then-choose both work (Issue 004 acceptance).

use async_trait::async_trait;
use orchest_protocol::{
    Asr, Capability, CapabilityDescriptor, ChatModel, Language, Modality, ModelCapabilities,
    ProtocolError, StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};
use orchest_provider::{Entry, ProviderConfig, Registry};

// --- minimal fakes so factories can return real trait objects ---

struct FakeChat(&'static str, &'static str);

#[async_trait]
impl ChatModel for FakeChat {
    fn provider_name(&self) -> &str {
        self.0
    }
    fn model_name(&self) -> &str {
        self.1
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        _m: &[orchest_protocol::Message],
        _t: &[orchest_protocol::ToolDef],
        _o: &orchest_protocol::RequestOptions,
        _tx: Option<tokio::sync::mpsc::Sender<orchest_protocol::StreamEvent>>,
    ) -> Result<orchest_protocol::ModelResponse, orchest_protocol::ModelError> {
        unreachable!("selection tests never run completion")
    }
}

struct FakeAsr(&'static str, &'static str);

#[async_trait]
impl Asr for FakeAsr {
    fn provider_name(&self) -> &str {
        self.0
    }
    fn model_name(&self) -> &str {
        self.1
    }
    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new(self.0.to_string(), self.1.to_string(), Capability::Asr)
    }
    fn supported_languages(&self) -> &[Language] {
        &[]
    }
    async fn transcribe(&self, _req: TranscribeRequest) -> Result<TranscribeResult, ProtocolError> {
        unreachable!()
    }
    async fn start_stream(
        &self,
        _req: StreamingTranscribeRequest,
    ) -> Result<orchest_protocol::RealtimeHandle, ProtocolError> {
        unreachable!()
    }
}

fn chat_desc(provider: &'static str, model: &'static str) -> CapabilityDescriptor {
    CapabilityDescriptor::new(provider, model, Capability::Chat)
}

fn fixture_registry() -> Registry {
    let mut reg = Registry::new();

    // A thinking, multimodal chat model.
    reg.register_chat(Entry::new(
        chat_desc("openai", "gpt-5.4")
            .streaming(true)
            .tools(true)
            .thinking(true)
            .with_input_modalities([Modality::Text, Modality::Image, Modality::Video]),
        |c| Ok(Box::new(FakeChat("openai", leak(&c.model))) as Box<dyn ChatModel>),
    ));
    // A text-only, non-thinking chat model.
    reg.register_chat(Entry::new(
        chat_desc("deepseek", "deepseek-chat")
            .streaming(true)
            .tools(true),
        |c| Ok(Box::new(FakeChat("deepseek", leak(&c.model))) as Box<dyn ChatModel>),
    ));
    // Two ASR providers, one bidirectional (Volcengine), one not.
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("volcengine", "bigmodel", Capability::Asr)
            .streaming(true)
            .duplex(true),
        |_c| Ok(Box::new(FakeAsr("volcengine", "bigmodel")) as Box<dyn Asr>),
    ));
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("deepgram", "nova-3", Capability::Asr).streaming(true),
        |_c| Ok(Box::new(FakeAsr("deepgram", "nova-3")) as Box<dyn Asr>),
    ));
    reg
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

#[test]
fn capability_query_filters_on_descriptor() {
    let reg = fixture_registry();
    // accepts([Image]) + thinking() → only the openai entry
    let picked = reg
        .chat()
        .accepts([Modality::Image])
        .thinking()
        .select()
        .expect("a multimodal thinking model matches");
    assert_eq!(picked.descriptor.provider.as_ref(), "openai");
    assert_eq!(picked.descriptor.model.as_ref(), "gpt-5.4");
}

#[test]
fn identity_pick_by_provider_slash_model() {
    let reg = fixture_registry();
    let picked = reg.chat().id("deepseek/deepseek-chat").select().unwrap();
    assert_eq!(picked.descriptor.provider.as_ref(), "deepseek");
}

#[test]
fn capability_and_identity_mix() {
    let reg = fixture_registry();
    // "bidirectional ASR from Volcengine" — the mixed query from the spec.
    let picked = reg
        .asr()
        .provider("volcengine")
        .bidirectional()
        .select()
        .expect("volcengine asr is bidirectional");
    assert_eq!(picked.descriptor.model.as_ref(), "bigmodel");
    // bidirectional filter excludes deepgram (not duplex)
    assert!(reg
        .asr()
        .provider("deepgram")
        .bidirectional()
        .select()
        .is_err());
}

#[test]
fn list_then_choose_returns_all_matches_sorted() {
    let reg = fixture_registry();
    let all = reg.chat().streaming().list();
    assert_eq!(all.len(), 2);
    // deterministic (provider, model) sort: deepseek before openai
    assert_eq!(all[0].descriptor.provider.as_ref(), "deepseek");
    assert_eq!(all[1].descriptor.provider.as_ref(), "openai");
}

#[test]
fn no_match_is_an_error_not_a_panic() {
    let reg = fixture_registry();
    let err = reg
        .chat()
        .accepts([Modality::Audio])
        .select()
        .expect_err("no audio-input chat fixture");
    assert_eq!(err.code, orchest_protocol::ErrorCode::NoMatchingProvider);
}

#[test]
fn select_errors_when_multiple_match_without_default() {
    let mut reg = Registry::new();
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("aliyun", "fun-asr-realtime", Capability::Asr)
            .streaming(true)
            .duplex(true),
        |_| Ok(Box::new(FakeAsr("aliyun", "fun-asr-realtime")) as Box<dyn Asr>),
    ));
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new(
            "aliyun",
            "qwen-audio-3.0-asr-flash-streaming",
            Capability::Asr,
        )
        .streaming(true)
        .duplex(true),
        |_| Ok(Box::new(FakeAsr("aliyun", "qwen-audio-3.0-asr-flash-streaming")) as Box<dyn Asr>),
    ));
    let err = reg
        .asr()
        .provider("aliyun")
        .select()
        .expect_err("multi-match without default must error under C2");
    assert_eq!(err.code, orchest_protocol::ErrorCode::NoMatchingProvider);
    assert!(
        err.message.to_lowercase().contains("ambiguous")
            || err.message.to_lowercase().contains("multiple"),
        "message should explain ambiguity: {}",
        err.message
    );
}

#[test]
fn select_prefers_unique_default_for_provider() {
    let mut reg = Registry::new();
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("aliyun", "fun-asr-realtime", Capability::Asr)
            .streaming(true)
            .duplex(true)
            .default_for_provider(true),
        |_| Ok(Box::new(FakeAsr("aliyun", "fun-asr-realtime")) as Box<dyn Asr>),
    ));
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new(
            "aliyun",
            "qwen-audio-3.0-asr-flash-streaming",
            Capability::Asr,
        )
        .streaming(true)
        .duplex(true),
        |_| Ok(Box::new(FakeAsr("aliyun", "qwen-audio-3.0-asr-flash-streaming")) as Box<dyn Asr>),
    ));
    let picked = reg.asr().provider("aliyun").select().unwrap();
    assert_eq!(picked.descriptor.model.as_ref(), "fun-asr-realtime");
}

#[test]
fn list_still_sorts_by_provider_model_without_default_bias() {
    let mut reg = Registry::new();
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new("aliyun", "fun-asr-realtime", Capability::Asr)
            .default_for_provider(true),
        |_| Ok(Box::new(FakeAsr("aliyun", "fun-asr-realtime")) as Box<dyn Asr>),
    ));
    reg.register_asr(Entry::new(
        CapabilityDescriptor::new(
            "aliyun",
            "qwen-audio-3.0-asr-flash-streaming",
            Capability::Asr,
        ),
        |_| Ok(Box::new(FakeAsr("aliyun", "qwen-audio-3.0-asr-flash-streaming")) as Box<dyn Asr>),
    ));
    let list = reg.asr().provider("aliyun").list();
    assert_eq!(list[0].descriptor.model.as_ref(), "fun-asr-realtime");
    assert_eq!(
        list[1].descriptor.model.as_ref(),
        "qwen-audio-3.0-asr-flash-streaming"
    );
}

#[test]
fn build_instantiates_the_selected_entry() {
    let reg = fixture_registry();
    let model = reg
        .chat()
        .id("openai/gpt-5.4")
        .build(&ProviderConfig::new("openai", "gpt-5.4"))
        .expect("entry instantiates");
    assert_eq!(ChatModel::provider_name(&*model), "openai");
}

#[test]
fn mechanism_only_registry_is_empty() {
    // `Registry::new()` is the mechanism-only constructor: zero registered impls,
    // independent of which dialect features are enabled. (`with_builtin()` loads
    // the feature-gated impl crates, so once a dialect is registered — e.g. the
    // Volcengine ASR entry under `stream` — its contents are feature-dependent.)
    let reg = Registry::new();
    assert!(reg.chat().list().is_empty());
    assert!(reg.asr().list().is_empty());
    assert!(reg.tts().list().is_empty());
    assert!(reg.realtime().list().is_empty());
    assert!(reg.gen().list().is_empty());
}

#[cfg(feature = "stream")]
#[test]
fn with_builtin_registers_stream_asr_dialect() {
    // With the WS weight tier enabled, the wall exposes the openspeech ASR
    // dialect by capability/identity — selection works against a real descriptor.
    let reg = Registry::with_builtin();
    let picked = reg
        .asr()
        .provider("volcengine")
        .bidirectional()
        .select()
        .expect("volcengine streaming ASR is registered under the stream feature");
    assert_eq!(picked.descriptor.model.as_ref(), "bigmodel");
}

#[cfg(feature = "stream")]
#[test]
fn with_builtin_registers_deepgram_asr_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .asr()
        .provider("deepgram")
        .select()
        .expect("deepgram streaming ASR is registered under the stream feature");
    assert_eq!(picked.descriptor.model.as_ref(), "nova-3");
}

#[cfg(feature = "stream")]
#[test]
fn with_builtin_registers_soniox_asr_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .asr()
        .provider("soniox")
        .select()
        .expect("soniox streaming ASR is registered under the stream feature");
    assert_eq!(picked.descriptor.model.as_ref(), "stt-rt-v5");
}

#[cfg(feature = "stream")]
#[test]
fn with_builtin_registers_aliyun_asr_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .asr()
        .provider("aliyun")
        .select()
        .expect("aliyun streaming ASR is registered under the stream feature");
    assert_eq!(picked.descriptor.model.as_ref(), "fun-asr-realtime");
}

#[cfg(feature = "stream")]
#[test]
fn with_builtin_registers_elevenlabs_asr_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .asr()
        .provider("elevenlabs")
        .select()
        .expect("elevenlabs streaming ASR is registered under the stream feature");
    assert_eq!(picked.descriptor.model.as_ref(), "scribe-v2-realtime");
}

#[cfg(feature = "stream")]
#[test]
fn with_builtin_registers_stream_tts_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .tts()
        .provider("volcengine")
        .select()
        .expect("volcengine TTS is registered under the stream feature");
    assert_eq!(picked.descriptor.model.as_ref(), "tts");
}

#[cfg(feature = "stream")]
#[test]
fn with_builtin_registers_minimax_tts_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .tts()
        .provider("minimax")
        .select()
        .expect("minimax TTS is registered under the stream feature");
    assert_eq!(picked.descriptor.model.as_ref(), "speech-2.8-hd");
}

#[cfg(feature = "stream")]
#[test]
fn with_builtin_registers_aliyun_tts_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .tts()
        .provider("aliyun")
        .select()
        .expect("aliyun TTS is registered under the stream feature");
    assert_eq!(picked.descriptor.model.as_ref(), "cosyvoice-v2");
}

#[cfg(feature = "http")]
#[test]
fn with_builtin_registers_assemblyai_batch_asr_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .asr()
        .provider("assemblyai")
        .select()
        .expect("assemblyai batch ASR is registered under the http feature");
    assert_eq!(picked.descriptor.model.as_ref(), "universal");
    assert!(!picked.descriptor.streaming);
}

#[cfg(feature = "http")]
#[test]
fn with_builtin_registers_speechmatics_batch_asr_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .asr()
        .provider("speechmatics")
        .select()
        .expect("speechmatics batch ASR is registered under the http feature");
    assert_eq!(picked.descriptor.model.as_ref(), "enhanced");
    assert!(!picked.descriptor.streaming);
}

#[cfg(feature = "http")]
#[test]
fn with_builtin_registers_minimax_music_gen_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .gen()
        .provider("minimax")
        .select()
        .expect("minimax music gen-task is registered under the http feature");
    assert_eq!(picked.descriptor.model.as_ref(), "music-2.6");
    assert_eq!(picked.descriptor.capability, Capability::GenTask);
}

#[cfg(feature = "visual")]
#[test]
fn with_builtin_registers_renderful_gen_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .gen()
        .provider("renderful")
        .select()
        .expect("renderful gen-task is registered under the visual feature");
    assert_eq!(picked.descriptor.model.as_ref(), "renderful-default");
    assert_eq!(picked.descriptor.capability, Capability::GenTask);
}

#[cfg(feature = "visual")]
#[test]
fn with_builtin_registers_aliyun_gen_dialect() {
    // "aliyun" hosts two gen dialects across features — the visual wanx image
    // model here and the http fun-music audio model — so select by id, matching
    // the volcengine image/video pattern below. (`.provider("aliyun")` is
    // ambiguous once both the http and visual features are enabled together.)
    let reg = Registry::with_builtin();
    let picked = reg
        .gen()
        .id("aliyun/wanx2.1-t2i-turbo")
        .select()
        .expect("aliyun wanx gen-task is registered under the visual feature");
    assert_eq!(picked.descriptor.model.as_ref(), "wanx2.1-t2i-turbo");
    assert_eq!(picked.descriptor.capability, Capability::GenTask);
    assert!(picked
        .descriptor
        .output_modalities
        .contains(&Modality::Image));
}

#[cfg(feature = "visual")]
#[test]
fn with_builtin_registers_crazyrouter_gen_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .gen()
        .provider("crazyrouter")
        .select()
        .expect("crazyrouter gen-task is registered under the visual feature");
    assert_eq!(picked.descriptor.model.as_ref(), "crazyrouter-default");
    assert_eq!(picked.descriptor.capability, Capability::GenTask);
}

#[cfg(feature = "visual")]
#[test]
fn with_builtin_registers_volcengine_image_gen_dialect() {
    // Two volcengine gen entries (image + video) share the provider, so pick by id.
    let reg = Registry::with_builtin();
    let picked = reg
        .gen()
        .id("volcengine/doubao-seedream-5-0-260128")
        .select()
        .expect("volcengine Ark image gen-task is registered under the visual feature");
    assert_eq!(
        picked.descriptor.model.as_ref(),
        "doubao-seedream-5-0-260128"
    );
    assert!(picked
        .descriptor
        .output_modalities
        .contains(&Modality::Image));
}

#[cfg(feature = "visual")]
#[test]
fn with_builtin_registers_volcengine_video_gen_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .gen()
        .id("volcengine/doubao-seedance-1-0-pro")
        .select()
        .expect("volcengine Ark video gen-task is registered under the visual feature");
    assert_eq!(picked.descriptor.model.as_ref(), "doubao-seedance-1-0-pro");
    assert!(picked
        .descriptor
        .output_modalities
        .contains(&Modality::Video));
}

#[cfg(feature = "stream")]
#[test]
fn with_builtin_registers_omni_realtime_dialect() {
    let reg = Registry::with_builtin();
    let picked = reg
        .realtime()
        .provider("volcengine")
        .select()
        .expect("volcengine omni realtime is registered under the stream feature");
    assert_eq!(picked.descriptor.model.as_ref(), "1.2.1.1");
    assert!(picked.descriptor.duplex && picked.descriptor.interruptible);
}

#[cfg(feature = "stream")]
#[test]
fn aliyun_asr_lists_multiple_catalog_models() {
    let reg = Registry::with_builtin();
    let list = reg.asr().provider("aliyun").list();
    let models: Vec<_> = list.iter().map(|e| e.descriptor.model.as_ref()).collect();
    assert!(models.contains(&"fun-asr-realtime"));
    assert!(models.contains(&"qwen-audio-3.0-asr-flash-streaming"));
    assert!(list.len() >= 2);
}

#[cfg(feature = "stream")]
#[test]
fn aliyun_asr_provider_select_returns_fun_asr_default() {
    let reg = Registry::with_builtin();
    let picked = reg.asr().provider("aliyun").select().unwrap();
    assert_eq!(picked.descriptor.model.as_ref(), "fun-asr-realtime");
    assert!(picked.descriptor.default_for_provider);
}

#[cfg(feature = "stream")]
#[test]
fn aliyun_asr_id_pins_qwen_streaming_model() {
    let reg = Registry::with_builtin();
    let picked = reg
        .asr()
        .id("aliyun/qwen-audio-3.0-asr-flash-streaming")
        .select()
        .unwrap();
    assert_eq!(
        picked.descriptor.model.as_ref(),
        "qwen-audio-3.0-asr-flash-streaming"
    );
}

#[cfg(all(feature = "http", feature = "visual"))]
#[test]
fn gen_aliyun_provider_select_defaults_to_fun_music() {
    let reg = Registry::with_builtin();
    let picked = reg.gen().provider("aliyun").select().unwrap();
    assert_eq!(picked.descriptor.model.as_ref(), "fun-music-v1");
    assert!(picked.descriptor.default_for_provider);
}

#[cfg(all(feature = "http", feature = "visual"))]
#[test]
fn gen_volcengine_provider_select_defaults_to_seedream_image() {
    let reg = Registry::with_builtin();
    let picked = reg.gen().provider("volcengine").select().unwrap();
    assert_eq!(
        picked.descriptor.model.as_ref(),
        "doubao-seedream-5-0-260128"
    );
    assert!(picked.descriptor.default_for_provider);
}

#[cfg(all(feature = "http", feature = "stream", feature = "visual"))]
#[test]
fn defaults_unique_per_capability_provider() {
    // Global invariant under all weight features: each (capability, provider)
    // group may have 0 or 1 `default_for_provider` entry — never more — or
    // provider-only `Query::select` becomes ambiguous.
    let reg = Registry::with_builtin();

    fn assert_at_most_one_default(
        capability: &str,
        entries: Vec<&orchest_provider::Entry<impl Sized>>,
    ) {
        use std::collections::HashMap;

        let mut defaults_by_provider: HashMap<&str, Vec<&str>> = HashMap::new();
        for e in entries {
            if e.descriptor.default_for_provider {
                defaults_by_provider
                    .entry(e.descriptor.provider.as_ref())
                    .or_default()
                    .push(e.descriptor.model.as_ref());
            }
        }
        for (provider, models) in defaults_by_provider {
            assert!(
                models.len() <= 1,
                "{capability}/{provider} must have at most one default_for_provider; got {}: {models:?}",
                models.len()
            );
        }
    }

    assert_at_most_one_default("chat", reg.chat().list());
    assert_at_most_one_default("asr", reg.asr().list());
    assert_at_most_one_default("tts", reg.tts().list());
    assert_at_most_one_default("realtime", reg.realtime().list());
    assert_at_most_one_default("gen", reg.gen().list());

    // Required Gen defaults remain exactly one each.
    for provider in ["aliyun", "volcengine"] {
        let defaults: Vec<_> = reg
            .gen()
            .provider(provider)
            .list()
            .into_iter()
            .filter(|e| e.descriptor.default_for_provider)
            .collect();
        assert_eq!(
            defaults.len(),
            1,
            "{provider} must have exactly one gen default"
        );
    }
}
