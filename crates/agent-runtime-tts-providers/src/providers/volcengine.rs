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

const DEFAULT_WS_URL: &str = "wss://openspeech.bytedance.com/api/v3/tts/bidirection";
const DEFAULT_ENV: &str = "VOLCENGINE_TTS_API_KEY";

#[derive(Clone)]
struct VolcengineTtsConfig {
    model: String,
    api_key: String,
    ws_url: String,
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
        if normalized.model != "seed-tts-2.0" {
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
        Ok(Self {
            config: VolcengineTtsConfig {
                model: normalized.model.to_owned(),
                api_key,
                ws_url: config.api_url.unwrap_or_else(|| DEFAULT_WS_URL.to_owned()),
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
        controls: crate::types::SpeechControls,
    ) -> VolcengineSynthesisRequest {
        VolcengineSynthesisRequest {
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
                "volcengine",
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
            "Volcengine adapter does not support SSML in v0.9.3",
        )),
    }
}

fn validate_output_format(format: &AudioFormat, supported: &[AudioFormat]) -> Result<(), TtsError> {
    if supported.iter().any(|candidate| candidate == format) {
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

#[derive(Clone)]
struct VolcengineSynthesisRequest {
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

struct VolcengineSynthesisResponse {
    audio: Bytes,
    duration_ms: Option<u64>,
    provider_metadata: serde_json::Value,
}

impl VolcengineSynthesisResponse {
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
trait VolcengineTtsTransport: Send + Sync {
    async fn synthesize(
        &self,
        request: VolcengineSynthesisRequest,
        text: Vec<TextChunk>,
    ) -> Result<VolcengineSynthesisResponse, TtsError>;

    async fn stream(
        &self,
        request: VolcengineSynthesisRequest,
        text: Vec<TextChunk>,
    ) -> Result<TtsOutputStream, TtsError>;

    async fn duplex(
        &self,
        request: VolcengineSynthesisRequest,
    ) -> Result<TtsDuplexStream, TtsError>;
}

struct VolcengineWebSocketTransport;

#[async_trait]
impl VolcengineTtsTransport for VolcengineWebSocketTransport {
    async fn synthesize(
        &self,
        request: VolcengineSynthesisRequest,
        text: Vec<TextChunk>,
    ) -> Result<VolcengineSynthesisResponse, TtsError> {
        let mut stream = self.stream(request.clone(), text).await?;
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
        Ok(VolcengineSynthesisResponse {
            audio: audio.freeze(),
            duration_ms,
            provider_metadata,
        })
    }

    async fn stream(
        &self,
        request: VolcengineSynthesisRequest,
        text: Vec<TextChunk>,
    ) -> Result<TtsOutputStream, TtsError> {
        let (event_tx, event_rx) = mpsc::channel(32);
        tokio::spawn(async move {
            if let Err(error) =
                volcengine_session_task(request, Some(text), None, event_tx.clone()).await
            {
                let _ = event_tx
                    .send(TtsStreamEvent::Error {
                        trace_id: "volcengine-stream".to_owned(),
                        error,
                        fatal: true,
                    })
                    .await;
            }
        });
        Ok(TtsOutputStream::new(event_rx))
    }

    async fn duplex(
        &self,
        request: VolcengineSynthesisRequest,
    ) -> Result<TtsDuplexStream, TtsError> {
        let (input_tx, input_rx) = mpsc::channel(16);
        let (event_tx, event_rx) = mpsc::channel(32);
        tokio::spawn(async move {
            if let Err(error) =
                volcengine_session_task(request, None, Some(input_rx), event_tx.clone()).await
            {
                let _ = event_tx
                    .send(TtsStreamEvent::Error {
                        trace_id: "volcengine-duplex".to_owned(),
                        error,
                        fatal: true,
                    })
                    .await;
            }
        });
        Ok(TtsDuplexStream::new(input_tx, event_rx))
    }
}

async fn volcengine_session_task(
    request: VolcengineSynthesisRequest,
    initial_text: Option<Vec<TextChunk>>,
    mut input_rx: Option<mpsc::Receiver<TextChunk>>,
    event_tx: mpsc::Sender<TtsStreamEvent>,
) -> Result<(), TtsError> {
    let ws_request = tungstenite::http::Request::builder()
        .uri(&request.ws_url)
        .header("X-Api-Key", &request.api_key)
        .header("X-Api-Resource-Id", &request.model)
        .header("X-Api-Connect-Id", uuid::Uuid::new_v4().to_string())
        .header("X-Control-Require-Usage-Tokens-Return", "text_words")
        .body(())
        .map_err(|e| {
            TtsError::new(
                TtsErrorCode::InvalidRequest,
                format!("build websocket request: {e}"),
            )
        })?;

    let (ws_stream, response) =
        tokio_tungstenite::connect_async(ws_request)
            .await
            .map_err(|e| {
                TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    format!("websocket connect failed: {e}"),
                )
            })?;
    let log_id = response
        .headers()
        .get("X-Tt-Logid")
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);
    let (mut sink, mut source) = ws_stream.split();
    let session_id = uuid::Uuid::new_v4().simple().to_string();
    let provider_metadata = serde_json::json!({"x_tt_logid": log_id});

    sink.send(Message::Binary(protocol::build_meta_frame(
        protocol::EVENT_START_CONNECTION,
        "",
        &serde_json::json!({}),
    )?))
    .await
    .map_err(stream_send_error)?;
    sink.send(Message::Binary(protocol::build_meta_frame(
        protocol::EVENT_START_SESSION,
        &session_id,
        &build_session_payload(&request),
    )?))
    .await
    .map_err(stream_send_error)?;

    event_tx
        .send(TtsStreamEvent::Started {
            trace_id: request.trace_id.clone(),
            provider: "volcengine".to_owned(),
            model: request.model.clone(),
            voice: VoiceInfo {
                provider: "volcengine".to_owned(),
                model: request.model.clone(),
                id: request.voice_id.clone(),
                display_name: request.voice_id.clone(),
                kind: VoiceKind::System,
                gender: None,
                languages: vec![],
                is_custom: false,
                supports_instruction: false,
                supports_emotion: false,
                supports_style: false,
                supports_cloning: false,
                supports_design: false,
                source: VoiceCatalogSource::ConservativeAssumption,
                provider_metadata: serde_json::Value::Null,
            },
        })
        .await
        .ok();

    if let Some(chunks) = initial_text {
        for chunk in chunks {
            send_text_chunk(&mut sink, &session_id, &chunk).await?;
        }
        sink.send(Message::Binary(protocol::build_meta_frame(
            protocol::EVENT_FINISH_SESSION,
            &session_id,
            &serde_json::json!({}),
        )?))
        .await
        .map_err(stream_send_error)?;
    }

    let mut audio_seq = 0;
    let mut first_audio_at: Option<Instant> = None;
    let started = Instant::now();
    loop {
        tokio::select! {
            maybe_chunk = async {
                match input_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => None,
                }
            }, if input_rx.is_some() => {
                match maybe_chunk {
                    Some(chunk) => {
                        send_text_chunk(&mut sink, &session_id, &chunk).await?;
                        if chunk.is_final {
                            sink.send(Message::Binary(protocol::build_meta_frame(protocol::EVENT_FINISH_SESSION, &session_id, &serde_json::json!({}))?))
                                .await
                                .map_err(stream_send_error)?;
                        }
                    }
                    None => {
                        sink.send(Message::Binary(protocol::build_meta_frame(protocol::EVENT_CANCEL_SESSION, &session_id, &serde_json::json!({}))?))
                            .await
                            .ok();
                    }
                }
            }
            next = source.next() => {
                let Some(message) = next else { break; };
                let message = message.map_err(|e| TtsError::new(TtsErrorCode::ProviderStreamError, format!("websocket receive failed: {e}")))?;
                if let Message::Binary(data) = message {
                    match protocol::parse_frame(&data)? {
                        protocol::VolcengineFrame::Audio { event, data, .. } if event == protocol::EVENT_TTS_RESPONSE => {
                            if first_audio_at.is_none() {
                                first_audio_at = Some(Instant::now());
                            }
                            event_tx.send(TtsStreamEvent::AudioChunk {
                                trace_id: request.trace_id.clone(),
                                data: Bytes::from(data),
                                format: request.output_format.clone(),
                                sequence: audio_seq,
                            }).await.ok();
                            audio_seq += 1;
                        }
                        protocol::VolcengineFrame::Meta { event, payload, .. } if event == protocol::EVENT_SESSION_FINISHED => {
                            let duration_ms = payload.get("audio_info").and_then(|v| v.get("duration")).and_then(serde_json::Value::as_u64);
                            let input_chars = payload.get("usage").and_then(|v| v.get("text_words")).and_then(serde_json::Value::as_u64).unwrap_or_default();
                            let mut telemetry = TtsTelemetryBuilder::new(
                                request.trace_id.clone(),
                                "volcengine",
                                request.model.clone(),
                                TtsOperation::DuplexStream,
                            )
                            .voice_id(Some(request.voice_id.clone()))
                            .input_chars(input_chars)
                            .build_with_final_latency(started.elapsed());
                            if let Some(first) = first_audio_at {
                                telemetry.first_audio_latency_ms = Some(first.duration_since(started).as_millis() as u64);
                            }
                            event_tx.send(TtsStreamEvent::Completed {
                                trace_id: request.trace_id.clone(),
                                summary: TtsStreamSummary {
                                    format: request.output_format,
                                    duration_ms,
                                    usage: TtsUsage {
                                        input_chars,
                                        billable_chars: Some(input_chars),
                                        audio_duration_ms: duration_ms,
                                        output_bytes: None,
                                        cost_estimate_micros: None,
                                    },
                                    option_adjustments: Vec::new(),
                                    provider_metadata,
                                    telemetry,
                                },
                            }).await.ok();
                            break;
                        }
                        protocol::VolcengineFrame::Meta { event, payload, .. } if event == protocol::EVENT_SESSION_FAILED || event == protocol::EVENT_CONNECTION_FAILED => {
                            return Err(TtsError::new(
                                TtsErrorCode::ProviderStreamError,
                                payload.get("message").and_then(serde_json::Value::as_str).unwrap_or("Volcengine session failed"),
                            ).with_upstream(None, payload.get("status_code").map(ToString::to_string), None, Some(payload)));
                        }
                        protocol::VolcengineFrame::Error { code, message } => {
                            return Err(TtsError::new(TtsErrorCode::ProviderStreamError, message).with_upstream(None, Some(code.to_string()), None, None));
                        }
                        _ => {}
                    }
                }
            }
            _ = tokio::time::sleep(request.timeout) => {
                return Err(TtsError::new(TtsErrorCode::Timeout, "Volcengine TTS stream timed out"));
            }
        }
    }
    Ok(())
}

async fn send_text_chunk<S>(
    sink: &mut S,
    session_id: &str,
    chunk: &TextChunk,
) -> Result<(), TtsError>
where
    S: futures_util::Sink<Message, Error = tungstenite::Error> + Unpin,
{
    if chunk.text.is_empty() {
        return Ok(());
    }
    let frame = protocol::build_meta_frame(
        protocol::EVENT_TASK_REQUEST,
        session_id,
        &serde_json::json!({"text": chunk.text}),
    )?;
    sink.send(Message::Binary(frame))
        .await
        .map_err(stream_send_error)
}

fn stream_send_error(e: tungstenite::Error) -> TtsError {
    TtsError::new(
        TtsErrorCode::ProviderStreamError,
        format!("websocket send failed: {e}"),
    )
}

fn build_session_payload(request: &VolcengineSynthesisRequest) -> serde_json::Value {
    let format = match request.output_format {
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Pcm16Le => "pcm",
        AudioFormat::WavPcm16Le => "wav",
        AudioFormat::OggOpus => "ogg_opus",
    };
    let mut req_params = serde_json::json!({
        "speaker": request.voice_id,
        "audio_params": {
            "format": format,
            "sample_rate": 24000,
            "speech_rate": volcengine_speech_rate(request.controls.speed),
        },
        "additions": {
            "post_process": {
                "pitch": request.controls.pitch.round() as i64,
            }
        },
    });
    if let Some(options) = request.provider_options.as_object() {
        for (key, value) in options {
            req_params[key] = value.clone();
        }
    }
    serde_json::json!({
        "user": {"uid": "orchest-sdk"},
        "req_params": req_params,
    })
}

fn volcengine_speech_rate(speed: f32) -> i64 {
    ((speed - 1.0) * 100.0).round().clamp(-50.0, 100.0) as i64
}

mod protocol {
    use crate::error::{TtsError, TtsErrorCode};

    pub const EVENT_START_CONNECTION: i32 = 1;
    pub const EVENT_CONNECTION_FAILED: i32 = 51;
    pub const EVENT_START_SESSION: i32 = 100;
    pub const EVENT_FINISH_SESSION: i32 = 102;
    pub const EVENT_CANCEL_SESSION: i32 = 101;
    pub const EVENT_SESSION_FAILED: i32 = 153;
    pub const EVENT_SESSION_FINISHED: i32 = 152;
    pub const EVENT_TASK_REQUEST: i32 = 200;
    pub const EVENT_TTS_RESPONSE: i32 = 352;

    const MSG_FULL_CLIENT_REQUEST: u8 = 0b0001;
    const MSG_FULL_SERVER_RESPONSE: u8 = 0b1001;
    const MSG_AUDIO_ONLY_RESPONSE: u8 = 0b1011;
    const MSG_ERROR_RESPONSE: u8 = 0b1111;
    const FLAG_WITH_EVENT: u8 = 0b0100;
    const SER_JSON: u8 = 0b0001;

    pub enum VolcengineFrame {
        Meta {
            event: i32,
            _session_id: String,
            payload: serde_json::Value,
        },
        Audio {
            event: i32,
            _session_id: String,
            data: Vec<u8>,
        },
        Error {
            code: u32,
            message: String,
        },
    }

    pub fn build_meta_frame(
        event: i32,
        session_id: &str,
        payload: &serde_json::Value,
    ) -> Result<Vec<u8>, TtsError> {
        let payload = serde_json::to_vec(payload).map_err(|e| {
            TtsError::new(
                TtsErrorCode::InvalidRequest,
                format!("serialize Volcengine payload: {e}"),
            )
        })?;
        Ok(build_frame(
            MSG_FULL_CLIENT_REQUEST,
            SER_JSON,
            event,
            session_id.as_bytes(),
            &payload,
        ))
    }

    pub fn parse_frame(data: &[u8]) -> Result<VolcengineFrame, TtsError> {
        if data.len() < 8 {
            return Err(TtsError::new(
                TtsErrorCode::ProviderStreamError,
                "Volcengine frame too short",
            ));
        }
        let msg_type = (data[1] >> 4) & 0x0f;
        match msg_type {
            MSG_FULL_SERVER_RESPONSE => parse_meta(data),
            MSG_AUDIO_ONLY_RESPONSE => parse_audio(data),
            MSG_ERROR_RESPONSE => parse_error(data),
            other => Err(TtsError::new(
                TtsErrorCode::ProviderStreamError,
                format!("unexpected Volcengine frame type {other}"),
            )),
        }
    }

    fn build_frame(
        msg_type: u8,
        serialization: u8,
        event: i32,
        session_id: &[u8],
        payload: &[u8],
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + 4 + 4 + session_id.len() + 4 + payload.len());
        out.extend_from_slice(&[
            0b0001_0001,
            (msg_type << 4) | FLAG_WITH_EVENT,
            serialization << 4,
            0,
        ]);
        out.extend_from_slice(&event.to_be_bytes());
        out.extend_from_slice(&(session_id.len() as u32).to_be_bytes());
        out.extend_from_slice(session_id);
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn parse_meta(data: &[u8]) -> Result<VolcengineFrame, TtsError> {
        let (event, session_id, payload) = parse_event_session_payload(data)?;
        let payload = serde_json::from_slice(&payload).map_err(|e| {
            TtsError::new(
                TtsErrorCode::ProviderStreamError,
                format!("parse Volcengine meta JSON: {e}"),
            )
        })?;
        Ok(VolcengineFrame::Meta {
            event,
            _session_id: session_id,
            payload,
        })
    }

    fn parse_audio(data: &[u8]) -> Result<VolcengineFrame, TtsError> {
        let (event, session_id, payload) = parse_event_session_payload(data)?;
        Ok(VolcengineFrame::Audio {
            event,
            _session_id: session_id,
            data: payload,
        })
    }

    fn parse_event_session_payload(data: &[u8]) -> Result<(i32, String, Vec<u8>), TtsError> {
        if data.len() < 12 {
            return Err(TtsError::new(
                TtsErrorCode::ProviderStreamError,
                "Volcengine event frame too short",
            ));
        }
        let event = i32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let session_len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let session_start = 12;
        let payload_len_start = session_start + session_len;
        if data.len() < payload_len_start + 4 {
            return Err(TtsError::new(
                TtsErrorCode::ProviderStreamError,
                "Volcengine frame missing payload length",
            ));
        }
        let session_id =
            String::from_utf8_lossy(&data[session_start..payload_len_start]).to_string();
        let payload_len = u32::from_be_bytes([
            data[payload_len_start],
            data[payload_len_start + 1],
            data[payload_len_start + 2],
            data[payload_len_start + 3],
        ]) as usize;
        let payload_start = payload_len_start + 4;
        if data.len() < payload_start + payload_len {
            return Err(TtsError::new(
                TtsErrorCode::ProviderStreamError,
                "Volcengine frame payload truncated",
            ));
        }
        Ok((
            event,
            session_id,
            data[payload_start..payload_start + payload_len].to_vec(),
        ))
    }

    fn parse_error(data: &[u8]) -> Result<VolcengineFrame, TtsError> {
        if data.len() < 12 {
            return Err(TtsError::new(
                TtsErrorCode::ProviderStreamError,
                "Volcengine error frame too short",
            ));
        }
        let code = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let message = if data.len() >= 12 + len {
            String::from_utf8_lossy(&data[12..12 + len]).to_string()
        } else {
            String::new()
        };
        Ok(VolcengineFrame::Error { code, message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CapturingTransport;

    #[async_trait]
    impl VolcengineTtsTransport for CapturingTransport {
        async fn synthesize(
            &self,
            request: VolcengineSynthesisRequest,
            text: Vec<TextChunk>,
        ) -> Result<VolcengineSynthesisResponse, TtsError> {
            assert_eq!(request.model, "seed-tts-2.0");
            assert_eq!(text[0].text, "hello");
            Ok(VolcengineSynthesisResponse {
                audio: Bytes::from_static(b"provider-audio"),
                duration_ms: Some(20),
                provider_metadata: serde_json::json!({"transport": "captured"}),
            })
        }

        async fn stream(
            &self,
            request: VolcengineSynthesisRequest,
            _text: Vec<TextChunk>,
        ) -> Result<TtsOutputStream, TtsError> {
            let (tx, rx) = mpsc::channel(4);
            tokio::spawn(async move {
                let voice = VoiceInfo {
                    provider: "volcengine".to_owned(),
                    model: request.model.clone(),
                    id: request.voice_id.clone(),
                    display_name: request.voice_id.clone(),
                    kind: VoiceKind::System,
                    gender: None,
                    languages: vec![],
                    is_custom: false,
                    supports_instruction: false,
                    supports_emotion: false,
                    supports_style: false,
                    supports_cloning: false,
                    supports_design: false,
                    source: VoiceCatalogSource::ConservativeAssumption,
                    provider_metadata: serde_json::Value::Null,
                };
                let _ = tx
                    .send(TtsStreamEvent::Started {
                        trace_id: request.trace_id.clone(),
                        provider: "volcengine".to_owned(),
                        model: request.model.clone(),
                        voice,
                    })
                    .await;
                let _ = tx
                    .send(TtsStreamEvent::Completed {
                        trace_id: request.trace_id.clone(),
                        summary: TtsStreamSummary {
                            format: request.output_format,
                            duration_ms: Some(10),
                            usage: TtsUsage::for_text("hello", Some(0)),
                            option_adjustments: Vec::new(),
                            provider_metadata: serde_json::Value::Null,
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

    #[tokio::test]
    async fn synthesize_uses_transport_audio_not_synthetic_payload() {
        let adapter = VolcengineTtsAdapter::with_transport(
            VolcengineTtsConfig {
                model: "seed-tts-2.0".to_owned(),
                api_key: "key".to_owned(),
                ws_url: DEFAULT_WS_URL.to_owned(),
                timeout: Duration::from_secs(1),
                provider_options: serde_json::Value::Null,
            },
            Arc::new(CapturingTransport),
        );
        let result = adapter
            .synthesize(SynthesizeRequest {
                model: Some("volcengine/seed-tts-2.0".to_owned()),
                input: TtsInput::Text("hello".to_owned()),
                voice: crate::types::VoiceSelection::by_id("v"),
                output: crate::types::AudioOutputConfig::new(AudioFormat::Mp3),
                controls: crate::types::SpeechControls::default(),
                compatibility: crate::types::CompatibilityPolicy::Strict,
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
        let adapter = VolcengineTtsAdapter::with_transport(
            VolcengineTtsConfig {
                model: "seed-tts-2.0".to_owned(),
                api_key: "key".to_owned(),
                ws_url: DEFAULT_WS_URL.to_owned(),
                timeout: Duration::from_secs(1),
                provider_options: serde_json::Value::Null,
            },
            Arc::new(CapturingTransport),
        );
        let mut stream = adapter
            .stream_synthesize(crate::types::StreamSynthesizeRequest {
                model: Some("volcengine/seed-tts-2.0".to_owned()),
                input: TtsInput::Text("hello".to_owned()),
                voice: crate::types::VoiceSelection::by_id("v"),
                output: crate::types::AudioOutputConfig::new(AudioFormat::Mp3),
                controls: crate::types::SpeechControls::default(),
                compatibility: crate::types::CompatibilityPolicy::Strict,
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
        let mut controls = crate::types::SpeechControls::default();
        controls.speed = 1.5;
        controls.pitch = 3.4;
        let request = VolcengineSynthesisRequest {
            trace_id: "trace".to_owned(),
            model: "seed-tts-2.0".to_owned(),
            api_key: "key".to_owned(),
            ws_url: DEFAULT_WS_URL.to_owned(),
            timeout: Duration::from_secs(1),
            voice_id: "voice".to_owned(),
            output_format: AudioFormat::Mp3,
            controls,
            provider_options: serde_json::Value::Null,
        };
        let payload = build_session_payload(&request);
        assert_eq!(payload["req_params"]["audio_params"]["speech_rate"], 50);
        assert_eq!(
            payload["req_params"]["additions"]["post_process"]["pitch"],
            3
        );
    }
}
