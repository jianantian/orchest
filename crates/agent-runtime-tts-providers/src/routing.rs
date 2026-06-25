use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::Instrument;

use crate::config::normalize_tts_provider_model;
use crate::error::{TtsError, TtsErrorCode};
use crate::observability;
use crate::streaming::{TtsDuplexStream, TtsOutputStream, TtsStreamEvent};
use crate::traits::TtsProvider;
use crate::types::{
    AudioFormat, CompatibilityPolicy, DuplexSynthesizeRequest, Language, ListVoicesRequest,
    SpeechControls, SynthesizeRequest, SynthesizeResult, TtsInputKind, VoiceKind,
};
use crate::voices::filter_voices;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsRoute {
    pub model: String,
    #[serde(default)]
    pub languages: Vec<Language>,
    #[serde(default)]
    pub voice_kinds: Vec<VoiceKind>,
    #[serde(default)]
    pub output_formats: Vec<AudioFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_micros_per_char: Option<u64>,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsGatewayConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_config_path: Option<PathBuf>,
    #[serde(default = "default_stream_channel_capacity")]
    pub stream_channel_capacity: usize,
}

impl Default for TtsGatewayConfig {
    fn default() -> Self {
        Self {
            route_config_path: None,
            stream_channel_capacity: default_stream_channel_capacity(),
        }
    }
}

fn default_stream_channel_capacity() -> usize {
    16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TtsRouteOperation {
    Batch,
    SingleStream,
    DuplexStream,
    /// Long-form async batch synthesis(Minimax `/v1/t2a_async_v2`)。
    /// 返回完整文件 URL,format 走 batch_output_formats。
    Async,
}

pub struct TtsRouter {
    providers: HashMap<String, Arc<dyn TtsProvider>>,
    routes: Vec<TtsRoute>,
}

impl TtsRouter {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            routes: Vec::new(),
        }
    }

    pub fn register_provider(&mut self, model: String, provider: Arc<dyn TtsProvider>) {
        self.providers.insert(model, provider);
    }

    pub fn set_routes(&mut self, routes: Vec<TtsRoute>) {
        self.routes = routes;
    }

    pub fn providers(&self) -> &HashMap<String, Arc<dyn TtsProvider>> {
        &self.providers
    }

    pub fn routes(&self) -> &[TtsRoute] {
        &self.routes
    }

    pub fn select_for_synthesize(
        &self,
        request: &SynthesizeRequest,
    ) -> Result<Arc<dyn TtsProvider>, TtsError> {
        self.select(
            TtsRouteOperation::Batch,
            request.model.as_deref(),
            Some(request.input.kind()),
            request.voice.language.as_ref(),
            request.voice.kind.as_ref(),
            &request.output.format,
        )
    }

    pub fn select_for_stream(
        &self,
        request: &SynthesizeRequest,
    ) -> Result<Arc<dyn TtsProvider>, TtsError> {
        self.select(
            TtsRouteOperation::SingleStream,
            request.model.as_deref(),
            Some(request.input.kind()),
            request.voice.language.as_ref(),
            request.voice.kind.as_ref(),
            &request.output.format,
        )
    }

    pub fn select_for_duplex(
        &self,
        request: &DuplexSynthesizeRequest,
    ) -> Result<Arc<dyn TtsProvider>, TtsError> {
        self.select(
            TtsRouteOperation::DuplexStream,
            request.model.as_deref(),
            None,
            request.voice.language.as_ref(),
            request.voice.kind.as_ref(),
            &request.output.format,
        )
    }

    /// 选择支持 `TtsOperation::Async` 的 provider(Minimax 长文本异步路径)。
    /// 用 `batch_output_formats` 做格式校验 —— 异步返回的是完整文件 URL,
    /// 语义与 batch 一致。
    pub fn select_for_async(
        &self,
        request: &SynthesizeRequest,
    ) -> Result<Arc<dyn TtsProvider>, TtsError> {
        self.select(
            TtsRouteOperation::Async,
            request.model.as_deref(),
            Some(request.input.kind()),
            request.voice.language.as_ref(),
            request.voice.kind.as_ref(),
            &request.output.format,
        )
    }

    pub fn select_for_voices(
        &self,
        request: &ListVoicesRequest,
    ) -> Result<Vec<Arc<dyn TtsProvider>>, TtsError> {
        if let Some(ref model) = request.model {
            return Ok(vec![self.provider_for_explicit_model(model)?]);
        }

        if self.routes.is_empty() {
            return Ok(self.providers.values().cloned().collect());
        }

        let mut candidates: Vec<&TtsRoute> = self
            .routes
            .iter()
            .filter(|route| {
                if let Some(ref language) = request.language {
                    if !route.languages.is_empty() && !route.languages.iter().any(|l| l == language)
                    {
                        return false;
                    }
                }
                if let Some(ref kind) = request.kind {
                    if !route.voice_kinds.is_empty()
                        && !route.voice_kinds.iter().any(|candidate| candidate == kind)
                    {
                        return false;
                    }
                }
                self.providers.contains_key(&route.model)
            })
            .collect();
        candidates.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.model.cmp(&b.model)));
        let mut selected = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for route in candidates {
            if seen.insert(route.model.clone()) {
                if let Some(provider) = self.providers.get(&route.model) {
                    selected.push(provider.clone());
                }
            }
        }
        Ok(selected)
    }

    fn provider_for_explicit_model(&self, model: &str) -> Result<Arc<dyn TtsProvider>, TtsError> {
        let normalized = normalize_tts_provider_model(model)?;
        let key = format!("{}/{}", normalized.provider, normalized.model);
        self.providers.get(&key).cloned().ok_or_else(|| {
            TtsError::new(
                TtsErrorCode::NoMatchingProvider,
                format!("no registered provider for model '{key}'"),
            )
        })
    }

    #[allow(clippy::too_many_arguments)] // justified: router selection needs explicit constraint inputs to keep filtering order readable.
    fn select(
        &self,
        operation: TtsRouteOperation,
        explicit_model: Option<&str>,
        input_kind: Option<TtsInputKind>,
        language: Option<&Language>,
        voice_kind: Option<&VoiceKind>,
        output_format: &AudioFormat,
    ) -> Result<Arc<dyn TtsProvider>, TtsError> {
        if let Some(model) = explicit_model {
            let provider = self.provider_for_explicit_model(model)?;
            validate_provider_capability(
                provider.as_ref(),
                operation,
                input_kind.as_ref(),
                language,
                voice_kind,
                output_format,
            )?;
            return Ok(provider);
        }

        if self.routes.is_empty() {
            return Err(TtsError::new(
                TtsErrorCode::NoMatchingProvider,
                "no model specified and no route config available",
            ));
        }

        let mut candidates: Vec<&TtsRoute> = self
            .routes
            .iter()
            .filter(|route| {
                route_matches_request(route, language, voice_kind, output_format)
                    && self.providers.contains_key(&route.model)
                    && self.providers.get(&route.model).is_some_and(|provider| {
                        validate_provider_capability(
                            provider.as_ref(),
                            operation,
                            input_kind.as_ref(),
                            language,
                            voice_kind,
                            output_format,
                        )
                        .is_ok()
                    })
            })
            .collect();
        candidates.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.model.cmp(&b.model)));
        let Some(route) = candidates.first() else {
            return Err(TtsError::new(
                TtsErrorCode::NoMatchingProvider,
                "no route matches the request constraints",
            )
            .with_metadata(serde_json::json!({
                "operation": format!("{operation:?}"),
                "output_format": output_format,
            })));
        };
        self.providers.get(&route.model).cloned().ok_or_else(|| {
            TtsError::new(
                TtsErrorCode::NoMatchingProvider,
                format!(
                    "route selected '{}' but provider is not registered",
                    route.model
                ),
            )
        })
    }
}

impl Default for TtsRouter {
    fn default() -> Self {
        Self::new()
    }
}

fn route_matches_request(
    route: &TtsRoute,
    language: Option<&Language>,
    voice_kind: Option<&VoiceKind>,
    output_format: &AudioFormat,
) -> bool {
    if let Some(language) = language {
        if !route.languages.is_empty()
            && !route
                .languages
                .iter()
                .any(|candidate| candidate == language)
        {
            return false;
        }
    }
    if let Some(voice_kind) = voice_kind {
        if !route.voice_kinds.is_empty()
            && !route
                .voice_kinds
                .iter()
                .any(|candidate| candidate == voice_kind)
        {
            return false;
        }
    }
    route.output_formats.is_empty()
        || route
            .output_formats
            .iter()
            .any(|candidate| candidate == output_format)
}

#[allow(clippy::too_many_arguments)] // justified: capability validation compares the same independent route constraints used by selection.
fn validate_provider_capability(
    provider: &dyn TtsProvider,
    operation: TtsRouteOperation,
    input_kind: Option<&TtsInputKind>,
    language: Option<&Language>,
    voice_kind: Option<&VoiceKind>,
    output_format: &AudioFormat,
) -> Result<(), TtsError> {
    let capabilities = provider.capabilities();
    match operation {
        TtsRouteOperation::Batch if !capabilities.batch_synthesis => {
            return Err(TtsError::unsupported_operation())
        }
        TtsRouteOperation::SingleStream if !capabilities.single_streaming => {
            return Err(TtsError::unsupported_operation())
        }
        TtsRouteOperation::DuplexStream if !capabilities.duplex_streaming => {
            return Err(TtsError::unsupported_operation())
        }
        TtsRouteOperation::Async if !capabilities.async_synthesis => {
            return Err(TtsError::unsupported_operation())
        }
        _ => {}
    }

    if let Some(input_kind) = input_kind {
        if !capabilities.input_kinds.is_empty()
            && !capabilities
                .input_kinds
                .iter()
                .any(|candidate| candidate == input_kind)
        {
            return Err(TtsError::new(
                TtsErrorCode::UnsupportedOption,
                "unsupported TTS input kind",
            ));
        }
    }

    if let Some(language) = language {
        if !capabilities.languages.is_empty()
            && !capabilities
                .languages
                .iter()
                .any(|candidate| candidate == language)
        {
            return Err(TtsError::new(
                TtsErrorCode::UnsupportedLanguage,
                format!("unsupported language '{}'", language.0),
            ));
        }
    }

    if let Some(voice_kind) = voice_kind {
        if !capabilities.voice_kinds.is_empty()
            && !capabilities
                .voice_kinds
                .iter()
                .any(|candidate| candidate == voice_kind)
        {
            return Err(TtsError::new(
                TtsErrorCode::UnsupportedOption,
                "unsupported voice kind",
            ));
        }
    }

    let formats = match operation {
        // Async returns a complete file URL — same format space as Batch.
        TtsRouteOperation::Batch | TtsRouteOperation::Async => &capabilities.batch_output_formats,
        TtsRouteOperation::SingleStream | TtsRouteOperation::DuplexStream => {
            &capabilities.stream_output_formats
        }
    };
    if !formats.iter().any(|candidate| candidate == output_format) {
        return Err(TtsError::new(
            TtsErrorCode::UnsupportedAudioFormat,
            "unsupported output audio format",
        ));
    }
    Ok(())
}

fn validate_controls(
    controls: &mut SpeechControls,
    compatibility: &CompatibilityPolicy,
    provider: &dyn TtsProvider,
    input_kind: Option<TtsInputKind>,
) -> Result<Vec<crate::types::OptionAdjustment>, TtsError> {
    let capabilities = provider.capabilities();
    let mut adjustments = Vec::new();
    clamp_or_reject(
        "speed",
        &mut controls.speed,
        0.5,
        2.0,
        compatibility,
        &mut adjustments,
    )?;
    clamp_or_reject(
        "pitch",
        &mut controls.pitch,
        -12.0,
        12.0,
        compatibility,
        &mut adjustments,
    )?;
    clamp_or_reject(
        "volume",
        &mut controls.volume,
        0.0,
        2.0,
        compatibility,
        &mut adjustments,
    )?;

    coerce_semantic_control(
        "instruction",
        &mut controls.instruction,
        capabilities.supports_instruction,
        compatibility,
        &mut adjustments,
    )?;
    coerce_semantic_control(
        "emotion",
        &mut controls.emotion,
        capabilities.supports_emotion,
        compatibility,
        &mut adjustments,
    )?;
    coerce_semantic_control(
        "style",
        &mut controls.style,
        capabilities.supports_style,
        compatibility,
        &mut adjustments,
    )?;
    if input_kind == Some(TtsInputKind::Ssml) && !capabilities.supports_ssml {
        return Err(TtsError::new(
            TtsErrorCode::UnsupportedOption,
            "provider does not support SSML input",
        ));
    }
    Ok(adjustments)
}

fn coerce_semantic_control(
    field: &str,
    value: &mut Option<String>,
    supported: bool,
    compatibility: &CompatibilityPolicy,
    adjustments: &mut Vec<crate::types::OptionAdjustment>,
) -> Result<(), TtsError> {
    let Some(requested) = value.take() else {
        return Ok(());
    };
    if supported {
        *value = Some(requested);
        return Ok(());
    }
    if *compatibility == CompatibilityPolicy::Strict {
        *value = Some(requested);
        return Err(TtsError::new(
            TtsErrorCode::UnsupportedOption,
            format!("unsupported speech control '{field}'"),
        ));
    }
    adjustments.push(crate::types::OptionAdjustment {
        option: field.to_owned(),
        requested: serde_json::json!(requested),
        applied: serde_json::Value::Null,
        reason: "provider does not support this semantic control".to_owned(),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)] // justified: clamp helper keeps numeric control policy centralized for speed, pitch, and volume.
fn clamp_or_reject(
    name: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    compatibility: &CompatibilityPolicy,
    adjustments: &mut Vec<crate::types::OptionAdjustment>,
) -> Result<(), TtsError> {
    if (*value >= min) && (*value <= max) {
        return Ok(());
    }
    if *compatibility == CompatibilityPolicy::Strict {
        return Err(TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("{name} must be in portable range {min}..={max}"),
        ));
    }
    let requested = *value;
    *value = value.clamp(min, max);
    adjustments.push(crate::types::OptionAdjustment {
        option: name.to_owned(),
        requested: serde_json::json!(requested),
        applied: serde_json::json!(*value),
        reason: "clamped to portable range".to_owned(),
    });
    Ok(())
}

pub struct TtsGateway {
    router: TtsRouter,
    config: TtsGatewayConfig,
}

impl TtsGateway {
    pub fn new(router: TtsRouter, config: TtsGatewayConfig) -> Self {
        Self { router, config }
    }

    pub fn router(&self) -> &TtsRouter {
        &self.router
    }

    pub fn config(&self) -> &TtsGatewayConfig {
        &self.config
    }

    pub async fn synthesize(
        &self,
        mut request: SynthesizeRequest,
    ) -> Result<SynthesizeResult, TtsError> {
        let trace_id = ensure_trace_id(&mut request.trace_id);
        let provider = {
            let _span = observability::router_select_span(&trace_id).entered();
            self.router.select_for_synthesize(&request)?
        };
        let mut adjustments = validate_controls(
            &mut request.controls,
            &request.compatibility,
            provider.as_ref(),
            Some(request.input.kind()),
        )?;
        let span = observability::gateway_synthesize_span(
            &trace_id,
            provider.provider_name(),
            provider.model_name(),
        );
        async {
            let mut result = provider.synthesize(request).await?;
            adjustments.append(&mut result.option_adjustments);
            result.option_adjustments = adjustments;
            result.telemetry.option_adjustment_count = result.option_adjustments.len() as u64;
            Ok(result)
        }
        .instrument(span)
        .await
    }

    pub async fn stream_synthesize(
        &self,
        mut request: SynthesizeRequest,
    ) -> Result<TtsOutputStream, TtsError> {
        let trace_id = ensure_trace_id(&mut request.trace_id);
        let provider = {
            let _span = observability::router_select_span(&trace_id).entered();
            self.router.select_for_stream(&request)?
        };
        validate_controls(
            &mut request.controls,
            &request.compatibility,
            provider.as_ref(),
            Some(request.input.kind()),
        )?;
        let provider_stream = provider.stream_synthesize(request).await?;
        let (public_tx, public_rx) = mpsc::channel(self.config.stream_channel_capacity);
        let provider_name = provider.provider_name().to_owned();
        let model_name = provider.model_name().to_owned();
        let span = observability::gateway_stream_span(&trace_id, &provider_name, &model_name);
        tokio::spawn(
            forward_provider_stream(
                trace_id.clone(),
                provider_name,
                model_name,
                provider_stream,
                public_tx,
            )
            .instrument(span),
        );
        Ok(TtsOutputStream::new(public_rx))
    }

    pub async fn start_duplex_stream(
        &self,
        mut request: DuplexSynthesizeRequest,
    ) -> Result<TtsDuplexStream, TtsError> {
        let trace_id = ensure_trace_id(&mut request.trace_id);
        let provider = {
            let _span = observability::router_select_span(&trace_id).entered();
            self.router.select_for_duplex(&request)?
        };
        validate_controls(
            &mut request.controls,
            &request.compatibility,
            provider.as_ref(),
            None,
        )?;
        let provider_stream = provider.start_duplex_stream(request).await?;
        let (public_event_tx, public_event_rx) = mpsc::channel(self.config.stream_channel_capacity);
        let (public_input_tx, public_input_rx) = mpsc::channel(self.config.stream_channel_capacity);
        let provider_name = provider.provider_name().to_owned();
        let model_name = provider.model_name().to_owned();
        tokio::spawn(forward_duplex_input(public_input_rx, provider_stream.input));
        tokio::spawn(forward_duplex_events(
            trace_id.clone(),
            provider_name,
            model_name,
            provider_stream.events,
            public_event_tx,
        ));
        Ok(TtsDuplexStream::new(public_input_tx, public_event_rx))
    }

    pub async fn list_voices(
        &self,
        mut request: ListVoicesRequest,
    ) -> Result<Vec<crate::types::VoiceInfo>, TtsError> {
        let trace_id = ensure_trace_id(&mut request.trace_id);
        let _span = observability::voices_list_span(&trace_id).entered();
        let providers = self.router.select_for_voices(&request)?;
        let mut voices = Vec::new();
        for provider in providers {
            voices.extend(provider.list_voices(request.clone()).await?);
        }
        Ok(filter_voices(voices, &request))
    }
}

async fn forward_provider_stream(
    trace_id: String,
    provider: String,
    model: String,
    mut provider_stream: TtsOutputStream,
    public_tx: mpsc::Sender<TtsStreamEvent>,
) {
    if public_tx
        .send(TtsStreamEvent::RouteSelected {
            trace_id: trace_id.clone(),
            provider,
            model,
        })
        .await
        .is_err()
    {
        return;
    }
    while let Some(event) = provider_stream.events.next().await {
        let terminal = event.is_terminal();
        if public_tx.send(event).await.is_err() || terminal {
            break;
        }
    }
}

async fn forward_duplex_input(
    mut public_rx: mpsc::Receiver<crate::types::TextChunk>,
    provider_input: crate::streaming::TtsTextSink,
) {
    while let Some(chunk) = public_rx.recv().await {
        let result = if chunk.is_final {
            provider_input.finish().await
        } else {
            provider_input.send_text(chunk.text).await
        };
        if result.is_err() || chunk.is_final {
            break;
        }
    }
}

async fn forward_duplex_events(
    trace_id: String,
    provider: String,
    model: String,
    mut provider_events: crate::streaming::TtsEventStream,
    public_tx: mpsc::Sender<TtsStreamEvent>,
) {
    if public_tx
        .send(TtsStreamEvent::RouteSelected {
            trace_id,
            provider,
            model,
        })
        .await
        .is_err()
    {
        return;
    }
    while let Some(event) = provider_events.next().await {
        let terminal = event.is_terminal();
        if public_tx.send(event).await.is_err() || terminal {
            break;
        }
    }
}

fn ensure_trace_id(trace_id: &mut Option<String>) -> String {
    if trace_id.is_none() {
        *trace_id = Some(uuid::Uuid::new_v4().to_string());
    }
    trace_id.clone().unwrap_or_default()
}

#[cfg(test)]
mod async_route_tests {
    use super::*;
    use crate::error::TtsErrorCode;
    use crate::streaming::{TtsDuplexStream, TtsOutputStream};
    use crate::types::{
        AudioOutputConfig, SpeechControls, SynthesizeRequest, SynthesizeResult, TtsInput,
        TtsModelCapabilities, VoiceSelection,
    };
    use async_trait::async_trait;

    /// Fake provider whose capability bool table is parameterised — used to
    /// verify the Async route arm in `validate_provider_capability`.
    struct CapFakeProvider {
        caps: TtsModelCapabilities,
    }

    #[async_trait]
    impl TtsProvider for CapFakeProvider {
        fn provider_name(&self) -> &str {
            "fake"
        }
        fn model_name(&self) -> &str {
            "fake/m"
        }
        fn capabilities(&self) -> TtsModelCapabilities {
            self.caps.clone()
        }
        async fn synthesize(
            &self,
            _: SynthesizeRequest,
        ) -> Result<SynthesizeResult, crate::error::TtsError> {
            unreachable!("not called")
        }
        async fn stream_synthesize(
            &self,
            _: SynthesizeRequest,
        ) -> Result<TtsOutputStream, crate::error::TtsError> {
            unreachable!()
        }
        async fn start_duplex_stream(
            &self,
            _: crate::types::DuplexSynthesizeRequest,
        ) -> Result<TtsDuplexStream, crate::error::TtsError> {
            unreachable!()
        }
        async fn list_voices(
            &self,
            _: crate::types::ListVoicesRequest,
        ) -> Result<Vec<crate::types::VoiceInfo>, crate::error::TtsError> {
            unreachable!()
        }
    }

    #[allow(dead_code)] // justified: kept as a fixture for upcoming router-driven tests of Async dispatch
    fn fake_request(model: &str) -> SynthesizeRequest {
        SynthesizeRequest {
            model: Some(model.into()),
            input: TtsInput::text("hi"),
            voice: VoiceSelection::by_id("v"),
            output: AudioOutputConfig::mp3(),
            controls: SpeechControls::default(),
            compatibility: crate::types::CompatibilityPolicy::default(),
            trace_id: None,
            provider_options: serde_json::Value::Null,
        }
    }

    #[test]
    fn async_route_rejects_provider_without_async_synthesis() {
        // Default capabilities have async_synthesis=false (aliyun/volcengine
        // baseline). Async route MUST return UnsupportedOperation.
        let provider = CapFakeProvider {
            caps: TtsModelCapabilities {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                async_synthesis: false,
                batch_output_formats: vec![AudioFormat::Mp3],
                stream_output_formats: vec![AudioFormat::Mp3],
                ..Default::default()
            },
        };
        let err = validate_provider_capability(
            &provider,
            TtsRouteOperation::Async,
            Some(&TtsInputKind::Text),
            None,
            None,
            &AudioFormat::Mp3,
        )
        .unwrap_err();
        assert_eq!(err.code, TtsErrorCode::UnsupportedOperation);
    }

    #[test]
    fn async_route_accepts_provider_with_async_synthesis() {
        let provider = CapFakeProvider {
            caps: TtsModelCapabilities {
                batch_synthesis: true,
                single_streaming: true,
                duplex_streaming: true,
                async_synthesis: true,
                batch_output_formats: vec![AudioFormat::Mp3],
                stream_output_formats: vec![AudioFormat::Mp3],
                ..Default::default()
            },
        };
        validate_provider_capability(
            &provider,
            TtsRouteOperation::Async,
            Some(&TtsInputKind::Text),
            None,
            None,
            &AudioFormat::Mp3,
        )
        .expect("Async route should accept provider with async_synthesis=true");
    }

    #[test]
    fn async_route_validates_against_batch_output_formats() {
        // Async uses batch_output_formats (not stream_output_formats) per
        // spec §4f: "Async returns a complete file URL; same format space as Batch."
        let provider = CapFakeProvider {
            caps: TtsModelCapabilities {
                async_synthesis: true,
                batch_output_formats: vec![AudioFormat::Mp3],
                stream_output_formats: vec![AudioFormat::OggOpus],
                ..Default::default()
            },
        };
        // Mp3 is in batch_output_formats → OK
        validate_provider_capability(
            &provider,
            TtsRouteOperation::Async,
            None,
            None,
            None,
            &AudioFormat::Mp3,
        )
        .expect("Mp3 in batch_output_formats");
        // OggOpus is only in stream_output_formats → rejected for Async
        let err = validate_provider_capability(
            &provider,
            TtsRouteOperation::Async,
            None,
            None,
            None,
            &AudioFormat::OggOpus,
        )
        .unwrap_err();
        assert_eq!(err.code, TtsErrorCode::UnsupportedAudioFormat);
    }

    #[test]
    fn default_tts_model_capabilities_has_async_synthesis_false() {
        let caps = TtsModelCapabilities::default();
        assert!(
            !caps.async_synthesis,
            "Default::default() async_synthesis must be false so existing \
             providers don't accidentally accept Async routes"
        );
    }
}
