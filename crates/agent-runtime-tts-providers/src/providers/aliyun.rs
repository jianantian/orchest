use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{self, Message};

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
    ListVoicesRequest, SpeechControls, SynthesizeRequest, SynthesizeResult, TextChunk, TtsInput,
    TtsInputKind, TtsModelCapabilities, TtsOperation, TtsStreamSummary, TtsUsage,
    VoiceCatalogSource, VoiceGender, VoiceInfo, VoiceKind,
};
use crate::voices::filter_voices;

const DEFAULT_INFERENCE_WS_URL: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference/";
const DEFAULT_ENV: &str = "ALIYUN_TTS_API_KEY";

#[derive(Clone)]
struct AliyunTtsConfig {
    model: String,
    api_key: String,
    ws_url: String,
    timeout: Duration,
    provider_options: serde_json::Value,
}

pub struct AliyunTtsAdapter {
    config: AliyunTtsConfig,
    transport: Arc<dyn AliyunTtsTransport>,
}

pub fn create_provider(config: TtsProviderRuntimeConfig) -> Result<Arc<dyn TtsProvider>, TtsError> {
    Ok(Arc::new(AliyunTtsAdapter::from_runtime_config(config)?))
}

impl AliyunTtsAdapter {
    fn from_runtime_config(config: TtsProviderRuntimeConfig) -> Result<Self, TtsError> {
        let normalized = normalize_tts_provider_model(&config.model)?;
        if normalized.provider != "aliyun" {
            return Err(TtsError::new(
                TtsErrorCode::UnknownProvider,
                "expected aliyun provider selector",
            ));
        }
        capabilities_for_model(normalized.model)?;
        let api_key = resolve_api_key(
            config.api_key.as_deref(),
            config.api_key_env.as_deref(),
            DEFAULT_ENV,
        )?
        .ok_or_else(|| {
            TtsError::new(
                TtsErrorCode::MissingApiKey,
                format!("missing Aliyun API key; set api_key or {DEFAULT_ENV}"),
            )
        })?;
        Ok(Self {
            config: AliyunTtsConfig {
                model: normalized.model.to_owned(),
                api_key,
                ws_url: config
                    .api_url
                    .unwrap_or_else(|| DEFAULT_INFERENCE_WS_URL.to_owned()),
                timeout: config.timeout.unwrap_or(Duration::from_secs(30)),
                provider_options: config.provider_options,
            },
            transport: Arc::new(AliyunWebSocketTransport),
        })
    }

    #[cfg(test)]
    fn with_transport(config: AliyunTtsConfig, transport: Arc<dyn AliyunTtsTransport>) -> Self {
        Self { config, transport }
    }

    fn validate_selector(&self, model: &Option<String>) -> Result<(), TtsError> {
        validate_direct_model_selector("aliyun", &self.config.model, model)
    }

    fn provider_request(
        &self,
        trace_id: String,
        voice_id: String,
        output_format: AudioFormat,
        controls: crate::types::SpeechControls,
    ) -> AliyunSynthesisRequest {
        AliyunSynthesisRequest {
            trace_id,
            model: self.config.model.clone(),
            api_key: self.config.api_key.clone(),
            ws_url: self.config.ws_url.clone(),
            timeout: self.config.timeout,
            voice_id,
            output_format,
            controls,
            provider_options: self.config.provider_options.clone(),
        }
    }

    fn voices(&self) -> Vec<VoiceInfo> {
        let voice = if self.config.model.contains("qwen3") {
            "Cherry"
        } else {
            "longxiaochun"
        };
        vec![VoiceInfo {
            provider: "aliyun".to_owned(),
            model: self.config.model.clone(),
            id: voice.to_owned(),
            display_name: voice.to_owned(),
            kind: VoiceKind::System,
            gender: Some(VoiceGender::Neutral),
            languages: vec![Language::new("zh-CN"), Language::new("en-US")],
            is_custom: false,
            supports_instruction: self.capabilities().supports_instruction,
            supports_emotion: false,
            supports_style: false,
            supports_cloning: false,
            supports_design: false,
            source: VoiceCatalogSource::StaticCatalog,
            provider_metadata: serde_json::json!({"model_family": model_family(&self.config.model)}),
        }]
    }
}

#[async_trait]
impl TtsProvider for AliyunTtsAdapter {
    fn provider_name(&self) -> &str {
        "aliyun"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> TtsModelCapabilities {
        capabilities_for_model(&self.config.model).unwrap_or_default()
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
        let response = self
            .transport
            .synthesize(
                provider_request,
                vec![TextChunk {
                    text: text.clone(),
                    is_final: true,
                }],
            )
            .await?;
        let usage = response.usage_for_text(&text);
        Ok(SynthesizeResult {
            audio: AudioData::Bytes(response.audio.clone()),
            format: request.output.format,
            duration_ms: response.duration_ms,
            usage: usage.clone(),
            option_adjustments: Vec::new(),
            provider_metadata: response.provider_metadata,
            telemetry: TtsTelemetryBuilder::new(
                trace_id,
                "aliyun",
                self.config.model.clone(),
                TtsOperation::Batch,
            )
            .voice_id(Some(voice_id))
            .input_chars(usage.input_chars)
            .output_bytes(Some(response.audio.len() as u64))
            .build_with_final_latency(started.elapsed()),
        })
    }

    async fn stream_synthesize(
        &self,
        request: crate::types::StreamSynthesizeRequest,
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
            .stream(
                provider_request,
                vec![TextChunk {
                    text,
                    is_final: true,
                }],
            )
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
            "Aliyun adapter does not support SSML in v0.9.3",
        )),
    }
}

fn validate_output_format(format: &AudioFormat, supported: &[AudioFormat]) -> Result<(), TtsError> {
    if supported.iter().any(|candidate| candidate == format) {
        return Ok(());
    }
    Err(TtsError::new(
        TtsErrorCode::UnsupportedAudioFormat,
        "unsupported Aliyun output audio format",
    ))
}

fn validate_controls(
    controls: &mut SpeechControls,
    compatibility: &CompatibilityPolicy,
    capabilities: &TtsModelCapabilities,
) -> Result<(), TtsError> {
    let allow_coerce = controls.allow_semantic_coercions;
    strip_unsupported_control(
        "instruction",
        &mut controls.instruction,
        capabilities.supports_instruction,
        compatibility,
        allow_coerce,
    )?;
    strip_unsupported_control(
        "emotion",
        &mut controls.emotion,
        capabilities.supports_emotion,
        compatibility,
        allow_coerce,
    )?;
    strip_unsupported_control(
        "style",
        &mut controls.style,
        capabilities.supports_style,
        compatibility,
        allow_coerce,
    )
}

fn strip_unsupported_control(
    name: &str,
    value: &mut Option<String>,
    supported: bool,
    compatibility: &CompatibilityPolicy,
    allow_coerce: bool,
) -> Result<(), TtsError> {
    if value.is_none() || supported {
        return Ok(());
    }
    if *compatibility == CompatibilityPolicy::Strict || !allow_coerce {
        return Err(TtsError::new(
            TtsErrorCode::UnsupportedOption,
            format!("unsupported speech control '{name}'"),
        ));
    }
    *value = None;
    Ok(())
}

fn capabilities_for_model(model: &str) -> Result<TtsModelCapabilities, TtsError> {
    let mut capabilities = TtsModelCapabilities {
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
    };
    match model {
        "cosyvoice-v3-flash" => {
            capabilities.supports_instruction = true;
        }
        "qwen3-tts-flash-realtime" => {}
        "qwen3-tts-instruct-flash-realtime" => {
            capabilities.supports_instruction = true;
        }
        other => {
            return Err(TtsError::new(
                TtsErrorCode::UnknownModel,
                format!("unknown Aliyun TTS model '{other}'"),
            ))
        }
    }
    Ok(capabilities)
}

fn model_family(model: &str) -> &'static str {
    if model.starts_with("qwen3") {
        "qwen3-tts"
    } else {
        "cosyvoice"
    }
}

#[derive(Clone)]
struct AliyunSynthesisRequest {
    trace_id: String,
    model: String,
    api_key: String,
    ws_url: String,
    timeout: Duration,
    voice_id: String,
    output_format: AudioFormat,
    controls: crate::types::SpeechControls,
    provider_options: serde_json::Value,
}

struct AliyunSynthesisResponse {
    audio: Bytes,
    duration_ms: Option<u64>,
    provider_metadata: serde_json::Value,
}

impl AliyunSynthesisResponse {
    fn usage_for_text(&self, text: &str) -> TtsUsage {
        TtsUsage {
            input_chars: text.chars().count() as u64,
            billable_chars: Some(text.chars().count() as u64),
            audio_duration_ms: self.duration_ms,
            output_bytes: Some(self.audio.len() as u64),
            cost_estimate_micros: None,
        }
    }
}

#[async_trait]
trait AliyunTtsTransport: Send + Sync {
    async fn synthesize(
        &self,
        request: AliyunSynthesisRequest,
        text: Vec<TextChunk>,
    ) -> Result<AliyunSynthesisResponse, TtsError>;

    async fn stream(
        &self,
        request: AliyunSynthesisRequest,
        text: Vec<TextChunk>,
    ) -> Result<TtsOutputStream, TtsError>;

    async fn duplex(&self, request: AliyunSynthesisRequest) -> Result<TtsDuplexStream, TtsError>;
}

struct AliyunWebSocketTransport;

#[async_trait]
impl AliyunTtsTransport for AliyunWebSocketTransport {
    async fn synthesize(
        &self,
        request: AliyunSynthesisRequest,
        text: Vec<TextChunk>,
    ) -> Result<AliyunSynthesisResponse, TtsError> {
        let mut stream = self.stream(request, text).await?;
        let mut audio = BytesMut::new();
        let mut metadata = serde_json::Value::Null;
        while let Some(event) = stream.events.next().await {
            match event {
                TtsStreamEvent::AudioChunk { data, .. } => audio.extend_from_slice(&data),
                TtsStreamEvent::Completed { summary, .. } => {
                    metadata = summary.provider_metadata;
                    break;
                }
                TtsStreamEvent::Error {
                    error, fatal: true, ..
                } => return Err(error),
                _ => {}
            }
        }
        Ok(AliyunSynthesisResponse {
            audio: audio.freeze(),
            duration_ms: None,
            provider_metadata: metadata,
        })
    }

    async fn stream(
        &self,
        request: AliyunSynthesisRequest,
        text: Vec<TextChunk>,
    ) -> Result<TtsOutputStream, TtsError> {
        let (event_tx, event_rx) = mpsc::channel(32);
        tokio::spawn(async move {
            if let Err(error) =
                aliyun_session_task(request, Some(text), None, event_tx.clone()).await
            {
                let _ = event_tx
                    .send(TtsStreamEvent::Error {
                        trace_id: "aliyun-stream".to_owned(),
                        error,
                        fatal: true,
                    })
                    .await;
            }
        });
        Ok(TtsOutputStream::new(event_rx))
    }

    async fn duplex(&self, request: AliyunSynthesisRequest) -> Result<TtsDuplexStream, TtsError> {
        let (input_tx, input_rx) = mpsc::channel(16);
        let (event_tx, event_rx) = mpsc::channel(32);
        tokio::spawn(async move {
            if let Err(error) =
                aliyun_session_task(request, None, Some(input_rx), event_tx.clone()).await
            {
                let _ = event_tx
                    .send(TtsStreamEvent::Error {
                        trace_id: "aliyun-duplex".to_owned(),
                        error,
                        fatal: true,
                    })
                    .await;
            }
        });
        Ok(TtsDuplexStream::new(input_tx, event_rx))
    }
}

async fn aliyun_session_task(
    request: AliyunSynthesisRequest,
    initial_text: Option<Vec<TextChunk>>,
    mut input_rx: Option<mpsc::Receiver<TextChunk>>,
    event_tx: mpsc::Sender<TtsStreamEvent>,
) -> Result<(), TtsError> {
    let ws_request = tungstenite::http::Request::builder()
        .uri(&request.ws_url)
        .header("Authorization", format!("bearer {}", request.api_key))
        .header("X-DashScope-DataInspection", "enable")
        .body(())
        .map_err(|e| {
            TtsError::new(
                TtsErrorCode::InvalidRequest,
                format!("build websocket request: {e}"),
            )
        })?;
    let (ws_stream, _response) =
        tokio_tungstenite::connect_async(ws_request)
            .await
            .map_err(|e| {
                TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    format!("websocket connect failed: {e}"),
                )
            })?;
    let (mut sink, mut source) = ws_stream.split();
    let task_id = uuid::Uuid::new_v4().simple().to_string();
    sink.send(Message::Text(
        protocol::run_task_command(&request, &task_id).to_string(),
    ))
    .await
    .map_err(stream_send_error)?;

    let voice = VoiceInfo {
        provider: "aliyun".to_owned(),
        model: request.model.clone(),
        id: request.voice_id.clone(),
        display_name: request.voice_id.clone(),
        kind: VoiceKind::System,
        gender: None,
        languages: vec![],
        is_custom: false,
        supports_instruction: request.controls.instruction.is_some(),
        supports_emotion: false,
        supports_style: false,
        supports_cloning: false,
        supports_design: false,
        source: VoiceCatalogSource::ConservativeAssumption,
        provider_metadata: serde_json::Value::Null,
    };
    event_tx
        .send(TtsStreamEvent::Started {
            trace_id: request.trace_id.clone(),
            provider: "aliyun".to_owned(),
            model: request.model.clone(),
            voice,
        })
        .await
        .ok();

    let mut text_seq = 0;
    let mut audio_seq = 0;
    let started = Instant::now();
    let mut first_audio_at: Option<Instant> = None;
    let mut input_chars = 0_u64;
    let mut output_bytes = 0_u64;
    let mut pending_initial: VecDeque<TextChunk> =
        initial_text.unwrap_or_default().into_iter().collect();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(request.timeout) => {
                return Err(TtsError::new(TtsErrorCode::Timeout, "Aliyun TTS stream timed out"));
            }
            maybe_chunk = async {
                if let Some(chunk) = pending_initial.pop_front() {
                    Some(chunk)
                } else {
                    match input_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => None,
                    }
                }
            }, if !pending_initial.is_empty() || input_rx.is_some() => {
                if let Some(chunk) = maybe_chunk {
                    if !chunk.text.is_empty() {
                        input_chars += chunk.text.chars().count() as u64;
                        sink.send(Message::Text(protocol::continue_task_command(&task_id, &chunk.text).to_string()))
                            .await
                            .map_err(stream_send_error)?;
                        event_tx.send(TtsStreamEvent::TextDelta {
                            trace_id: request.trace_id.clone(),
                            text: chunk.text,
                            sequence: text_seq,
                            is_final: chunk.is_final,
                        }).await.ok();
                        text_seq += 1;
                    }
                    if chunk.is_final {
                        sink.send(Message::Text(protocol::finish_task_command(&task_id).to_string()))
                            .await
                            .map_err(stream_send_error)?;
                    }
                }
            }
            next = source.next() => {
                let Some(message) = next else { break; };
                let message = message.map_err(|e| TtsError::new(TtsErrorCode::ProviderStreamError, format!("websocket receive failed: {e}")))?;
                match message {
                    Message::Binary(data) => {
                        if first_audio_at.is_none() {
                            first_audio_at = Some(Instant::now());
                        }
                        output_bytes += data.len() as u64;
                        event_tx.send(TtsStreamEvent::AudioChunk {
                            trace_id: request.trace_id.clone(),
                            data: Bytes::from(data),
                            format: request.output_format.clone(),
                            sequence: audio_seq,
                        }).await.ok();
                        audio_seq += 1;
                    }
                    Message::Text(text) => match protocol::parse_server_event(&text)? {
                        protocol::AliyunServerEvent::TaskFinished { metadata } => {
                            let mut telemetry = TtsTelemetryBuilder::new(
                                request.trace_id.clone(),
                                "aliyun",
                                request.model.clone(),
                                TtsOperation::DuplexStream,
                            )
                            .voice_id(Some(request.voice_id.clone()))
                            .input_chars(input_chars)
                            .output_bytes(Some(output_bytes))
                            .build_with_final_latency(started.elapsed());
                            if let Some(first) = first_audio_at {
                                telemetry.first_audio_latency_ms = Some(first.duration_since(started).as_millis() as u64);
                            }
                            event_tx.send(TtsStreamEvent::Completed {
                                trace_id: request.trace_id.clone(),
                                summary: TtsStreamSummary {
                                    format: request.output_format,
                                    duration_ms: None,
                                    usage: TtsUsage {
                                        input_chars,
                                        billable_chars: Some(input_chars),
                                        audio_duration_ms: None,
                                        output_bytes: Some(output_bytes),
                                        cost_estimate_micros: None,
                                    },
                                    option_adjustments: Vec::new(),
                                    provider_metadata: metadata,
                                    telemetry,
                                },
                            }).await.ok();
                            break;
                        }
                        protocol::AliyunServerEvent::TaskFailed { code, message, metadata } => {
                            return Err(TtsError::new(TtsErrorCode::ProviderStreamError, message)
                                .with_upstream(None, code, None, Some(metadata)));
                        }
                        protocol::AliyunServerEvent::Other => {}
                    },
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn stream_send_error(e: tungstenite::Error) -> TtsError {
    TtsError::new(
        TtsErrorCode::ProviderStreamError,
        format!("websocket send failed: {e}"),
    )
}

mod protocol {
    use super::{AliyunSynthesisRequest, AudioFormat};
    use crate::error::{TtsError, TtsErrorCode};

    pub enum AliyunServerEvent {
        TaskFinished {
            metadata: serde_json::Value,
        },
        TaskFailed {
            code: Option<String>,
            message: String,
            metadata: serde_json::Value,
        },
        Other,
    }

    pub fn run_task_command(request: &AliyunSynthesisRequest, task_id: &str) -> serde_json::Value {
        serde_json::json!({
            "header": {
                "action": "run-task",
                "task_id": task_id,
                "streaming": "duplex"
            },
            "payload": {
                "task_group": "audio",
                "task": "tts",
                "function": "SpeechSynthesizer",
                "model": request.model,
                "parameters": parameters(request),
                "input": {}
            }
        })
    }

    pub fn continue_task_command(task_id: &str, text: &str) -> serde_json::Value {
        serde_json::json!({
            "header": {
                "action": "continue-task",
                "task_id": task_id,
                "streaming": "duplex"
            },
            "payload": {
                "input": {"text": text}
            }
        })
    }

    pub fn finish_task_command(task_id: &str) -> serde_json::Value {
        serde_json::json!({
            "header": {
                "action": "finish-task",
                "task_id": task_id,
                "streaming": "duplex"
            },
            "payload": {"input": {}}
        })
    }

    pub fn parse_server_event(text: &str) -> Result<AliyunServerEvent, TtsError> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
            TtsError::new(
                TtsErrorCode::ProviderStreamError,
                format!("parse Aliyun event JSON: {e}"),
            )
        })?;
        let event = value
            .get("header")
            .and_then(|h| h.get("event"))
            .and_then(serde_json::Value::as_str);
        match event {
            Some("task-finished") => Ok(AliyunServerEvent::TaskFinished { metadata: value }),
            Some("task-failed") => Ok(AliyunServerEvent::TaskFailed {
                code: value
                    .get("header")
                    .and_then(|h| h.get("error_code"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned),
                message: value
                    .get("header")
                    .and_then(|h| h.get("error_message"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Aliyun task failed")
                    .to_owned(),
                metadata: value,
            }),
            _ => Ok(AliyunServerEvent::Other),
        }
    }

    fn parameters(request: &AliyunSynthesisRequest) -> serde_json::Value {
        let mut params = serde_json::json!({
            "text_type": "PlainText",
            "voice": request.voice_id,
            "format": format_name(&request.output_format),
            "sample_rate": 24000,
            "volume": (request.controls.volume * 50.0).round() as i64,
            "rate": request.controls.speed,
            "pitch": request.controls.pitch,
            "enable_ssml": false,
        });
        if let Some(instruction) = request.controls.instruction.as_ref() {
            if request.model.starts_with("qwen3-") {
                params["instructions"] = serde_json::json!(instruction);
            } else {
                params["instruction"] = serde_json::json!(instruction);
            }
        }
        if let Some(options) = request.provider_options.as_object() {
            for (key, value) in options {
                params[key] = value.clone();
            }
        }
        params
    }

    fn format_name(format: &AudioFormat) -> &'static str {
        match format {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Pcm16Le => "pcm",
            AudioFormat::WavPcm16Le => "wav",
            AudioFormat::OggOpus => "opus",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CapturingTransport;

    #[async_trait]
    impl AliyunTtsTransport for CapturingTransport {
        async fn synthesize(
            &self,
            request: AliyunSynthesisRequest,
            text: Vec<TextChunk>,
        ) -> Result<AliyunSynthesisResponse, TtsError> {
            assert_eq!(request.model, "cosyvoice-v3-flash");
            assert_eq!(text[0].text, "hello");
            Ok(AliyunSynthesisResponse {
                audio: Bytes::from_static(b"aliyun-audio"),
                duration_ms: Some(30),
                provider_metadata: serde_json::json!({"task_id": "t"}),
            })
        }

        async fn stream(
            &self,
            _request: AliyunSynthesisRequest,
            _text: Vec<TextChunk>,
        ) -> Result<TtsOutputStream, TtsError> {
            unreachable!()
        }

        async fn duplex(
            &self,
            _request: AliyunSynthesisRequest,
        ) -> Result<TtsDuplexStream, TtsError> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn synthesize_uses_transport_audio_not_synthetic_payload() {
        let adapter = AliyunTtsAdapter::with_transport(
            AliyunTtsConfig {
                model: "cosyvoice-v3-flash".to_owned(),
                api_key: "key".to_owned(),
                ws_url: DEFAULT_INFERENCE_WS_URL.to_owned(),
                timeout: Duration::from_secs(1),
                provider_options: serde_json::Value::Null,
            },
            Arc::new(CapturingTransport),
        );
        let result = adapter
            .synthesize(SynthesizeRequest {
                model: Some("aliyun/cosyvoice-v3-flash".to_owned()),
                input: TtsInput::Text("hello".to_owned()),
                voice: crate::types::VoiceSelection::by_id("longxiaochun"),
                output: crate::types::AudioOutputConfig::new(AudioFormat::Mp3),
                controls: crate::types::SpeechControls::default(),
                compatibility: crate::types::CompatibilityPolicy::Strict,
                trace_id: Some("trace".to_owned()),
                provider_options: serde_json::Value::Null,
            })
            .await
            .unwrap();
        match result.audio {
            AudioData::Bytes(bytes) => assert_eq!(bytes, Bytes::from_static(b"aliyun-audio")),
            AudioData::Url { .. } => panic!("expected bytes"),
        }
        assert_eq!(result.telemetry.operation, TtsOperation::Batch);
        assert_eq!(result.telemetry.trace_id, "trace");
    }

    #[test]
    fn protocol_builds_dashscope_run_task_command() {
        let mut request = AliyunSynthesisRequest {
            trace_id: "trace".to_owned(),
            model: "cosyvoice-v3-flash".to_owned(),
            api_key: "key".to_owned(),
            ws_url: DEFAULT_INFERENCE_WS_URL.to_owned(),
            timeout: Duration::from_secs(1),
            voice_id: "longxiaochun".to_owned(),
            output_format: AudioFormat::Mp3,
            controls: crate::types::SpeechControls::default(),
            provider_options: serde_json::Value::Null,
        };
        request.controls.instruction = Some("warm".to_owned());
        let cmd = protocol::run_task_command(&request, "task");
        assert_eq!(cmd["header"]["action"], "run-task");
        assert_eq!(cmd["payload"]["model"], "cosyvoice-v3-flash");
        assert_eq!(cmd["payload"]["parameters"]["voice"], "longxiaochun");
        assert_eq!(cmd["payload"]["parameters"]["instruction"], "warm");
    }

    #[test]
    fn protocol_uses_qwen_instruction_field_name() {
        let mut request = AliyunSynthesisRequest {
            trace_id: "trace".to_owned(),
            model: "qwen3-tts-instruct-flash-realtime".to_owned(),
            api_key: "key".to_owned(),
            ws_url: DEFAULT_INFERENCE_WS_URL.to_owned(),
            timeout: Duration::from_secs(1),
            voice_id: "Cherry".to_owned(),
            output_format: AudioFormat::Pcm16Le,
            controls: crate::types::SpeechControls::default(),
            provider_options: serde_json::Value::Null,
        };
        request.controls.instruction = Some("warm".to_owned());
        let cmd = protocol::run_task_command(&request, "task");
        assert_eq!(cmd["payload"]["parameters"]["instructions"], "warm");
        assert!(cmd["payload"]["parameters"].get("instruction").is_none());
    }
}
