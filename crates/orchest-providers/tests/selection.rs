//! Registry selection tests over **fixture descriptors** (no real impl crate).
//!
//! Proves the one mechanism serves capability-query AND identity-pick and that
//! they mix, and that pick-one / list-then-choose both work (Issue 004 acceptance).

use async_trait::async_trait;
use orchest_protocol::{
    Asr, Capability, CapabilityDescriptor, ChatModel, Language, Modality, ModelCapabilities,
    ProtocolError, StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};
use orchest_providers::{Entry, ProviderConfig, Registry};

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
