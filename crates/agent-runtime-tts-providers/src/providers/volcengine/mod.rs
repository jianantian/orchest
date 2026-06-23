use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::BytesMut;
use tokio_tungstenite::tungstenite;

use crate::config::{
    normalize_tts_provider_model, resolve_api_key, validate_direct_model_selector,
    TtsProviderRuntimeConfig,
};
use crate::error::{TtsError, TtsErrorCode};
use crate::observability::TtsTelemetryBuilder;
use crate::streaming::{TtsDuplexStream, TtsOutputStream, TtsStreamEvent};
use crate::traits::TtsProvider;
use crate::types::{
    AudioData, AudioFormat, CompatibilityPolicy, DuplexSynthesizeRequest, Language,
    ListVoicesRequest, SpeechControls, SynthesizeRequest, SynthesizeResult, TtsInput, TtsInputKind,
    TtsModelCapabilities, TtsOperation, VoiceCatalogSource, VoiceGender, VoiceInfo, VoiceKind,
};
use crate::voices::filter_voices;

mod bidirectional;
mod protocol;
mod unidirectional;

const DEFAULT_BIDIRECTIONAL_WS_URL: &str = "wss://openspeech.bytedance.com/api/v3/tts/bidirection";
const DEFAULT_UNIDIRECTIONAL_WS_URL: &str =
    "wss://openspeech.bytedance.com/api/v3/tts/unidirectional/stream";
const DEFAULT_ENV: &str = "VOLCENGINE_API_KEY";

#[derive(Clone)]
struct VolcengineTtsConfig {
    model: String,
    resource_id: String,
    api_key: String,
    /// Used by stream_synthesize and synthesize (collect).
    unidirectional_ws_url: String,
    /// Used by start_duplex_stream.
    bidirectional_ws_url: String,
    timeout: Duration,
    provider_options: serde_json::Value,
}

pub struct VolcengineTtsAdapter {
    config: VolcengineTtsConfig,
    transport: Arc<dyn VolcengineTtsTransport>,
}

pub fn create_provider(config: TtsProviderRuntimeConfig) -> Result<Arc<dyn TtsProvider>, TtsError> {
    Ok(Arc::new(VolcengineTtsAdapter::from_runtime_config(config)?))
}

impl VolcengineTtsAdapter {
    fn from_runtime_config(config: TtsProviderRuntimeConfig) -> Result<Self, TtsError> {
        let normalized = normalize_tts_provider_model(&config.model)?;
        if normalized.provider != "volcengine" {
            return Err(TtsError::new(
                TtsErrorCode::UnknownProvider,
                "expected volcengine provider selector",
            ));
        }
        if !matches!(
            normalized.model,
            "seed-tts-1.0" | "seed-tts-1.0-concurr" | "seed-tts-2.0"
        ) {
            return Err(TtsError::new(
                TtsErrorCode::UnknownModel,
                format!("unknown Volcengine TTS model '{}'", normalized.model),
            ));
        }
        let api_key = resolve_api_key(
            config.api_key.as_deref(),
            config.api_key_env.as_deref(),
            DEFAULT_ENV,
        )?
        .ok_or_else(|| {
            TtsError::new(
                TtsErrorCode::MissingApiKey,
                format!("missing Volcengine API key; set api_key or {DEFAULT_ENV}"),
            )
        })?;
        // resource_id defaults to the model name but can be overridden via
        // provider_options.resource_id for billing variants like "seed-tts-1.0-concurr".
        let resource_id = config
            .provider_options
            .get("resource_id")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| normalized.model.to_owned());
        Ok(Self {
            config: VolcengineTtsConfig {
                model: normalized.model.to_owned(),
                resource_id,
                api_key,
                unidirectional_ws_url: config
                    .api_url
                    .unwrap_or_else(|| DEFAULT_UNIDIRECTIONAL_WS_URL.to_owned()),
                bidirectional_ws_url: DEFAULT_BIDIRECTIONAL_WS_URL.to_owned(),
                timeout: config.timeout.unwrap_or(Duration::from_secs(30)),
                provider_options: config.provider_options,
            },
            transport: Arc::new(VolcengineWebSocketTransport),
        })
    }

    #[cfg(test)]
    fn with_transport(
        config: VolcengineTtsConfig,
        transport: Arc<dyn VolcengineTtsTransport>,
    ) -> Self {
        Self { config, transport }
    }

    fn validate_selector(&self, model: &Option<String>) -> Result<(), TtsError> {
        validate_direct_model_selector("volcengine", &self.config.model, model)
    }

    fn voices(&self) -> Vec<VoiceInfo> {
        vec![VoiceInfo {
            provider: "volcengine".to_owned(),
            model: self.config.model.clone(),
            id: "zh_female_wanwanxiaohe_moon_bigtts".to_owned(),
            display_name: "Wanwan Xiaohe".to_owned(),
            kind: VoiceKind::System,
            gender: Some(VoiceGender::Female),
            languages: vec![Language::new("zh-CN"), Language::new("en-US")],
            is_custom: false,
            supports_instruction: false,
            supports_emotion: false,
            supports_style: false,
            supports_cloning: false,
            supports_design: false,
            source: VoiceCatalogSource::StaticCatalog,
            provider_metadata: serde_json::json!({"resource_id": self.config.model}),
        }]
    }

    fn provider_request(
        &self,
        trace_id: String,
        voice_id: String,
        output_format: AudioFormat,
        controls: SpeechControls,
    ) -> VolcengineSynthesisRequest {
        VolcengineSynthesisRequest {
            trace_id,
            model: self.config.model.clone(),
            resource_id: self.config.resource_id.clone(),
            api_key: self.config.api_key.clone(),
            unidirectional_ws_url: self.config.unidirectional_ws_url.clone(),
            bidirectional_ws_url: self.config.bidirectional_ws_url.clone(),
            timeout: self.config.timeout,
            voice_id,
            output_format,
            controls,
            provider_options: self.config.provider_options.clone(),
        }
    }
}

#[async_trait]
impl TtsProvider for VolcengineTtsAdapter {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> TtsModelCapabilities {
        volcengine_model_capabilities()
    }

    async fn synthesize(&self, request: SynthesizeRequest) -> Result<SynthesizeResult, TtsError> {
        self.validate_selector(&request.model)?;
        validate_output_format(
            &request.output.format,
            &self.capabilities().batch_output_formats,
        )?;
        let mut controls = request.controls;
        validate_controls(&mut controls, &request.compatibility, &self.capabilities())?;
        let trace_id = request
            .trace_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let text = extract_plain_text(request.input)?;
        let voice_id = request.voice.id.clone();
        let provider_request = self.provider_request(
            trace_id.clone(),
            voice_id.clone(),
            request.output.format.clone(),
            controls,
        );
        let started = Instant::now();
        let mut stream = self
            .transport
            .stream_unidirectional(provider_request, text.clone())
            .await?;
        let mut audio = BytesMut::new();
        let mut duration_ms = None;
        let mut provider_metadata = serde_json::Value::Null;
        while let Some(event) = stream.events.next().await {
            match event {
                TtsStreamEvent::AudioChunk { data, .. } => audio.extend_from_slice(&data),
                TtsStreamEvent::Completed { summary, .. } => {
                    duration_ms = summary.duration_ms;
                    provider_metadata = summary.provider_metadata;
                    break;
                }
                TtsStreamEvent::Error {
                    error, fatal: true, ..
                } => return Err(error),
                _ => {}
            }
        }
        let audio = audio.freeze();
        let input_chars = text.chars().count() as u64;
        Ok(SynthesizeResult {
            audio: AudioData::Bytes(audio.clone()),
            format: request.output.format,
            duration_ms,
            usage: crate::types::TtsUsage {
                input_chars,
                billable_chars: Some(input_chars),
                audio_duration_ms: duration_ms,
                output_bytes: Some(audio.len() as u64),
                cost_estimate_micros: None,
            },
            option_adjustments: Vec::new(),
            provider_metadata,
            telemetry: TtsTelemetryBuilder::new(
                trace_id,
                "volcengine",
                self.config.model.clone(),
                TtsOperation::Batch,
            )
            .voice_id(Some(voice_id))
            .input_chars(input_chars)
            .output_bytes(Some(audio.len() as u64))
            .build_with_final_latency(started.elapsed()),
        })
    }

    async fn stream_synthesize(
        &self,
        request: SynthesizeRequest,
    ) -> Result<TtsOutputStream, TtsError> {
        self.validate_selector(&request.model)?;
        validate_output_format(
            &request.output.format,
            &self.capabilities().stream_output_formats,
        )?;
        let mut controls = request.controls;
        validate_controls(&mut controls, &request.compatibility, &self.capabilities())?;
        let trace_id = request
            .trace_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let text = extract_plain_text(request.input)?;
        let provider_request =
            self.provider_request(trace_id, request.voice.id, request.output.format, controls);
        self.transport
            .stream_unidirectional(provider_request, text)
            .await
    }

    async fn start_duplex_stream(
        &self,
        request: DuplexSynthesizeRequest,
    ) -> Result<TtsDuplexStream, TtsError> {
        self.validate_selector(&request.model)?;
        validate_output_format(
            &request.output.format,
            &self.capabilities().stream_output_formats,
        )?;
        let mut controls = request.controls;
        validate_controls(&mut controls, &request.compatibility, &self.capabilities())?;
        let trace_id = request
            .trace_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let provider_request =
            self.provider_request(trace_id, request.voice.id, request.output.format, controls);
        self.transport.duplex(provider_request).await
    }

    async fn list_voices(&self, request: ListVoicesRequest) -> Result<Vec<VoiceInfo>, TtsError> {
        self.validate_selector(&request.model)?;
        Ok(filter_voices(self.voices(), &request))
    }
}

fn extract_plain_text(input: TtsInput) -> Result<String, TtsError> {
    match input {
        TtsInput::Text(text) => Ok(text),
        TtsInput::Ssml(_) => Err(TtsError::new(
            TtsErrorCode::UnsupportedOption,
            "Volcengine does not support SSML",
        )),
    }
}

fn validate_output_format(format: &AudioFormat, supported: &[AudioFormat]) -> Result<(), TtsError> {
    if supported.iter().any(|c| c == format) {
        return Ok(());
    }
    Err(TtsError::new(
        TtsErrorCode::UnsupportedAudioFormat,
        "unsupported Volcengine output audio format",
    ))
}

fn validate_controls(
    controls: &mut SpeechControls,
    compatibility: &CompatibilityPolicy,
    capabilities: &TtsModelCapabilities,
) -> Result<(), TtsError> {
    strip_unsupported_control(
        "instruction",
        &mut controls.instruction,
        capabilities.supports_instruction,
        compatibility,
    )?;
    strip_unsupported_control(
        "emotion",
        &mut controls.emotion,
        capabilities.supports_emotion,
        compatibility,
    )?;
    strip_unsupported_control(
        "style",
        &mut controls.style,
        capabilities.supports_style,
        compatibility,
    )
}

fn strip_unsupported_control(
    name: &str,
    value: &mut Option<String>,
    supported: bool,
    compatibility: &CompatibilityPolicy,
) -> Result<(), TtsError> {
    if value.is_none() || supported {
        return Ok(());
    }
    if *compatibility == CompatibilityPolicy::Strict {
        return Err(TtsError::new(
            TtsErrorCode::UnsupportedOption,
            format!("unsupported speech control '{name}'"),
        ));
    }
    *value = None;
    Ok(())
}

fn volcengine_model_capabilities() -> TtsModelCapabilities {
    TtsModelCapabilities {
        batch_synthesis: true,
        single_streaming: true,
        duplex_streaming: true,
        input_kinds: vec![TtsInputKind::Text],
        languages: vec![Language::new("zh-CN"), Language::new("en-US")],
        voice_kinds: vec![VoiceKind::System, VoiceKind::Custom],
        batch_output_formats: vec![AudioFormat::Mp3, AudioFormat::Pcm16Le],
        stream_output_formats: vec![AudioFormat::Mp3, AudioFormat::Pcm16Le],
        supports_instruction: false,
        supports_emotion: false,
        supports_style: false,
        supports_ssml: false,
    }
}

pub(super) fn audio_format_name(format: &AudioFormat) -> &'static str {
    match format {
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Pcm16Le => "pcm",
        AudioFormat::WavPcm16Le => "wav",
        AudioFormat::OggOpus => "ogg_opus",
    }
}

pub(super) fn volcengine_speech_rate(speed: f32) -> i64 {
    ((speed - 1.0) * 100.0).round().clamp(-50.0, 100.0) as i64
}

pub(super) fn stream_send_error(e: tungstenite::Error) -> TtsError {
    TtsError::new(
        TtsErrorCode::ProviderStreamError,
        format!("websocket send failed: {e}"),
    )
}

#[derive(Clone)]
struct VolcengineSynthesisRequest {
    trace_id: String,
    model: String,
    resource_id: String,
    api_key: String,
    unidirectional_ws_url: String,
    bidirectional_ws_url: String,
    timeout: Duration,
    voice_id: String,
    output_format: AudioFormat,
    controls: SpeechControls,
    provider_options: serde_json::Value,
}

#[async_trait]
trait VolcengineTtsTransport: Send + Sync {
    async fn stream_unidirectional(
        &self,
        request: VolcengineSynthesisRequest,
        text: String,
    ) -> Result<TtsOutputStream, TtsError>;

    async fn duplex(
        &self,
        request: VolcengineSynthesisRequest,
    ) -> Result<TtsDuplexStream, TtsError>;
}

struct VolcengineWebSocketTransport;

#[async_trait]
impl VolcengineTtsTransport for VolcengineWebSocketTransport {
    async fn stream_unidirectional(
        &self,
        request: VolcengineSynthesisRequest,
        text: String,
    ) -> Result<TtsOutputStream, TtsError> {
        Ok(unidirectional::spawn_stream(request, text))
    }

    async fn duplex(
        &self,
        request: VolcengineSynthesisRequest,
    ) -> Result<TtsDuplexStream, TtsError> {
        Ok(bidirectional::spawn_duplex(request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::TtsTelemetryBuilder;
    use crate::types::{TtsStreamSummary, TtsUsage};
    use bytes::Bytes;
    use tokio::sync::mpsc;

    struct CapturingTransport;

    #[async_trait]
    impl VolcengineTtsTransport for CapturingTransport {
        async fn stream_unidirectional(
            &self,
            request: VolcengineSynthesisRequest,
            text: String,
        ) -> Result<TtsOutputStream, TtsError> {
            assert_eq!(request.model, "seed-tts-2.0");
            assert_eq!(text, "hello");
            let (tx, rx) = mpsc::channel(4);
            tokio::spawn(async move {
                let _ = tx
                    .send(TtsStreamEvent::Started {
                        trace_id: request.trace_id.clone(),
                        provider: "volcengine".to_owned(),
                        model: request.model.clone(),
                        voice: bidirectional::make_voice_info(&request),
                    })
                    .await;
                let _ = tx
                    .send(TtsStreamEvent::AudioChunk {
                        trace_id: request.trace_id.clone(),
                        data: Bytes::from_static(b"provider-audio"),
                        format: request.output_format.clone(),
                        sequence: 0,
                    })
                    .await;
                let _ = tx
                    .send(TtsStreamEvent::Completed {
                        trace_id: request.trace_id.clone(),
                        summary: TtsStreamSummary {
                            format: request.output_format,
                            duration_ms: Some(20),
                            usage: TtsUsage::for_text("hello", Some(0)),
                            option_adjustments: Vec::new(),
                            provider_metadata: serde_json::json!({"transport": "captured"}),
                            telemetry: TtsTelemetryBuilder::new(
                                request.trace_id,
                                "volcengine",
                                request.model,
                                TtsOperation::SingleStream,
                            )
                            .build(),
                        },
                    })
                    .await;
            });
            Ok(TtsOutputStream::new(rx))
        }

        async fn duplex(
            &self,
            _request: VolcengineSynthesisRequest,
        ) -> Result<TtsDuplexStream, TtsError> {
            unreachable!()
        }
    }

    fn test_config() -> VolcengineTtsConfig {
        VolcengineTtsConfig {
            model: "seed-tts-2.0".to_owned(),
            resource_id: "seed-tts-2.0".to_owned(),
            api_key: "key".to_owned(),
            unidirectional_ws_url: DEFAULT_UNIDIRECTIONAL_WS_URL.to_owned(),
            bidirectional_ws_url: DEFAULT_BIDIRECTIONAL_WS_URL.to_owned(),
            timeout: Duration::from_secs(1),
            provider_options: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn synthesize_uses_transport_audio_not_synthetic_payload() {
        let adapter =
            VolcengineTtsAdapter::with_transport(test_config(), Arc::new(CapturingTransport));
        let result = adapter
            .synthesize(SynthesizeRequest {
                model: Some("volcengine/seed-tts-2.0".to_owned()),
                input: TtsInput::Text("hello".to_owned()),
                voice: crate::types::VoiceSelection::by_id("v"),
                output: crate::types::AudioOutputConfig::new(AudioFormat::Mp3),
                controls: SpeechControls::default(),
                compatibility: CompatibilityPolicy::Strict,
                trace_id: Some("trace".to_owned()),
                provider_options: serde_json::Value::Null,
            })
            .await
            .unwrap();
        match result.audio {
            AudioData::Bytes(bytes) => assert_eq!(bytes, Bytes::from_static(b"provider-audio")),
            AudioData::Url { .. } => panic!("expected bytes"),
        }
    }

    #[tokio::test]
    async fn direct_provider_stream_starts_without_route_selected() {
        let adapter =
            VolcengineTtsAdapter::with_transport(test_config(), Arc::new(CapturingTransport));
        let mut stream = adapter
            .stream_synthesize(SynthesizeRequest {
                model: Some("volcengine/seed-tts-2.0".to_owned()),
                input: TtsInput::Text("hello".to_owned()),
                voice: crate::types::VoiceSelection::by_id("v"),
                output: crate::types::AudioOutputConfig::new(AudioFormat::Mp3),
                controls: SpeechControls::default(),
                compatibility: CompatibilityPolicy::Strict,
                trace_id: Some("trace".to_owned()),
                provider_options: serde_json::Value::Null,
            })
            .await
            .unwrap();

        let first = stream.events.next().await.unwrap();
        assert!(first.is_started());
        assert!(!first.is_route_selected());
    }

    #[test]
    fn protocol_parses_audio_response_frame() {
        let frame = protocol::build_meta_frame(
            protocol::EVENT_TTS_RESPONSE,
            "session",
            &serde_json::json!("audio"),
        )
        .unwrap();
        let mut audio_frame = frame;
        audio_frame[1] = (0b1011 << 4) | 0b0100;
        let parsed = protocol::parse_frame(&audio_frame).unwrap();
        match parsed {
            protocol::VolcengineFrame::Audio { event, data, .. } => {
                assert_eq!(event, protocol::EVENT_TTS_RESPONSE);
                assert_eq!(data, br#""audio""#);
            }
            _ => panic!("expected audio frame"),
        }
    }

    #[test]
    fn session_payload_maps_portable_controls_to_volcengine_fields() {
        let controls = SpeechControls {
            speed: 1.5,
            pitch: 3.4,
            ..SpeechControls::default()
        };
        let request = VolcengineSynthesisRequest {
            trace_id: "trace".to_owned(),
            model: "seed-tts-2.0".to_owned(),
            resource_id: "seed-tts-2.0".to_owned(),
            api_key: "key".to_owned(),
            unidirectional_ws_url: DEFAULT_UNIDIRECTIONAL_WS_URL.to_owned(),
            bidirectional_ws_url: DEFAULT_BIDIRECTIONAL_WS_URL.to_owned(),
            timeout: Duration::from_secs(1),
            voice_id: "voice".to_owned(),
            output_format: AudioFormat::Mp3,
            controls,
            provider_options: serde_json::Value::Null,
        };
        let payload = bidirectional::build_session_payload(&request);
        assert_eq!(payload["req_params"]["audio_params"]["speech_rate"], 50);
        // additions is JSON-encoded as a string in the Volcengine wire format
        let additions: serde_json::Value =
            serde_json::from_str(payload["req_params"]["additions"].as_str().unwrap()).unwrap();
        assert_eq!(additions["post_process"]["pitch"], 3);
    }
}
