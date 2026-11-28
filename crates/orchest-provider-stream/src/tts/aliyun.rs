//! Aliyun DashScope streaming **TTS** (CosyVoice / SpeechSynthesizer) on the
//! spine (Issue 006). Ported from `agent-runtime-tts-providers`'s
//! `providers/aliyun`, over the spine [`ProtocolError`] / [`StreamEvent`].
//!
//! Same DashScope envelope as the ASR dialect, inverted: control is **text** JSON
//! (`run-task` → `continue-task` → `finish-task`) and synthesized audio arrives as
//! **binary** frames; `task-finished` / `task-failed` are text lifecycle events.

use crate::metering::VoiceMeter;
pub(super) mod duplex;

use async_trait::async_trait;
use bytes::Bytes;
use orchest_protocol::VoiceSource;
use orchest_protocol::{
    AudioFormat, Capability, CapabilityDescriptor, ErrorCode, EventStream, Modality, ProtocolError,
    StreamEvent, SynthesizeRequest, SynthesizeResult, TokenUsage, Tts,
};
use orchest_provider_core::catalog::TtsCatalogExt;
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::ws::{connect_async, tungstenite};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::transport::{ByteDuplex, WsDuplex, WsFrame};
use crate::tts::require_explicit_voice;

pub(crate) const COSYVOICE_V2_PRICE: orchest_provider_core::voice::VoicePrice =
    orchest_provider_core::voice::VoicePrice {
        currency: "USD",
        unit: orchest_protocol::VoiceRateUnit::BilledCharacter,
        price_per_unit: 0.286706 / 10_000.0,
        scope: "CosyVoice v2, Beijing",
        source_url: "https://www.alibabacloud.com/help/en/model-studio/model-pricing",
    };

const DEFAULT_WS_URL: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference/";

fn format_name(format: &AudioFormat) -> &'static str {
    match format {
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Wav | AudioFormat::WavPcm16Le => "wav",
        AudioFormat::Pcm | AudioFormat::Pcm16Le => "pcm",
        _ => "mp3",
    }
}

/// `run-task` text frame opening a synthesis session (`task: tts`).
pub fn build_run_task(
    task_id: &str,
    model: &str,
    voice: &str,
    format: &AudioFormat,
    options: &Value,
) -> Value {
    let mut parameters = json!({
        "text_type": "PlainText",
        "voice": voice,
        "format": format_name(format),
        "sample_rate": 24000,
    });
    if let Some(obj) = options.as_object() {
        for (k, v) in obj {
            parameters[k] = v.clone();
        }
    }
    json!({
        "header": { "action": "run-task", "task_id": task_id, "streaming": "duplex" },
        "payload": {
            "task_group": "audio",
            "task": "tts",
            "function": "SpeechSynthesizer",
            "model": model,
            "parameters": parameters,
            "input": {},
        }
    })
}

/// `continue-task` text frame carrying a text chunk to synthesize.
pub fn build_continue_task(task_id: &str, text: &str) -> Value {
    json!({
        "header": { "action": "continue-task", "task_id": task_id, "streaming": "duplex" },
        "payload": { "input": { "text": text } }
    })
}

/// `finish-task` text frame closing the session.
pub fn build_finish_task(task_id: &str) -> Value {
    json!({
        "header": { "action": "finish-task", "task_id": task_id, "streaming": "duplex" },
        "payload": { "input": {} }
    })
}

/// A decoded DashScope TTS lifecycle event.
#[derive(Debug, PartialEq)]
pub enum AliyunTtsEvent {
    Finished,
    Failed(String),
    Other,
}

/// Parse a DashScope server **text** event (`task-finished` / `task-failed`).
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_server_event(text: &str) -> Result<AliyunTtsEvent, ProtocolError> {
    parse_server_event_with_usage(text).map(|(event, _)| event)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context
fn parse_server_event_with_usage(
    text: &str,
) -> Result<(AliyunTtsEvent, Option<u64>), ProtocolError> {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("parse Aliyun event JSON: {e}"),
        )
    })?;
    let header = value.get("header");
    let event = header.and_then(|h| h.get("event")).and_then(Value::as_str);
    let event = match event {
        Some("task-finished") => AliyunTtsEvent::Finished,
        Some("task-failed") => AliyunTtsEvent::Failed(
            header
                .and_then(|h| h.get("error_message"))
                .and_then(Value::as_str)
                .unwrap_or("Aliyun task failed")
                .to_owned(),
        ),
        _ => AliyunTtsEvent::Other,
    };
    Ok((
        event,
        value
            .pointer("/payload/usage/characters")
            .and_then(Value::as_u64),
    ))
}

/// Drive one DashScope synthesis: send `run-task` / `continue-task(text)` /
/// `finish-task` as **text** frames, then stream inbound frames — **binary** →
/// `AudioDelta`, `task-finished` → `Done`, `task-failed` → fatal `Error`.
#[allow(clippy::too_many_arguments)] // justified: the three pre-built command frames + transport/format/events mirror the wire session 1:1
pub async fn run_aliyun_synthesis<T: ByteDuplex>(
    mut transport: T,
    run_task: Value,
    continue_task: Value,
    finish_task: Value,
    format: AudioFormat,
    events: mpsc::Sender<StreamEvent>,
    mut meter: VoiceMeter,
) {
    for (index, command) in [run_task, continue_task, finish_task]
        .into_iter()
        .enumerate()
    {
        if transport
            .send(WsFrame::Text(command.to_string()))
            .await
            .is_err()
        {
            return;
        }
        if index == 1 {
            meter.text_sent();
        }
    }
    let mut sequence = 0u64;
    while let Some(frame) = transport.recv().await {
        match frame {
            WsFrame::Binary(data) if !data.is_empty() => {
                if events
                    .send(StreamEvent::AudioDelta {
                        data: Bytes::from(data),
                        format,
                        sequence,
                        tts: None,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
                sequence += 1;
            }
            WsFrame::Binary(_) => {}
            WsFrame::Text(text) => match parse_server_event_with_usage(&text) {
                Ok((AliyunTtsEvent::Finished, characters)) => {
                    meter.billed_characters(characters);
                    meter.complete();
                    let _ = events
                        .send(StreamEvent::Done {
                            usage: TokenUsage::default(),
                        })
                        .await;
                    break;
                }
                Ok((AliyunTtsEvent::Failed(message), _)) => {
                    let _ = events
                        .send(StreamEvent::Error {
                            error: ProtocolError::new(ErrorCode::ProviderTaskFailed, message),
                            fatal: true,
                        })
                        .await;
                    break;
                }
                Ok((AliyunTtsEvent::Other, characters)) => meter.billed_characters(characters),
                Err(error) => {
                    let _ = events.send(StreamEvent::Error { error, fatal: true }).await;
                    break;
                }
            },
        }
    }
}

/// Aliyun WebSocket TTS configuration.
#[derive(Debug, Clone)]
pub struct AliyunTtsConfig {
    pub model: String,
    pub ws_url: String,
    pub api_key: String,
}

/// The aliyun DashScope streaming TTS provider as the spine [`Tts`].
pub struct AliyunTts {
    config: AliyunTtsConfig,
}

impl AliyunTts {
    pub fn new(config: AliyunTtsConfig) -> Self {
        Self { config }
    }

    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context
    fn websocket_request(
        &self,
        url: &str,
    ) -> Result<tungstenite::http::Request<()>, ProtocolError> {
        if !url.starts_with("wss://") {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "WebSocket URL must use wss:// for secure credential transport",
            ));
        }
        tungstenite::http::Request::builder()
            .uri(url)
            .header("Authorization", format!("bearer {}", self.config.api_key))
            .header("Host", host_of(url))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| {
                ProtocolError::new(ErrorCode::InvalidRequest, format!("build ws request: {e}"))
            })
    }

    async fn open_stream(
        &self,
        mut request: SynthesizeRequest,
    ) -> Result<EventStream, ProtocolError> {
        CATALOG_EXT.validate_options(&mut request.options)?;
        let voice = require_explicit_voice(&request)?;
        let task_id = uuid::Uuid::new_v4().simple().to_string();
        let run_task = build_run_task(
            &task_id,
            &self.config.model,
            voice,
            &request.format,
            &request.options,
        );
        let continue_task = build_continue_task(&task_id, &request.text);
        let finish_task = build_finish_task(&task_id);

        let ws_request = self.websocket_request(&self.config.ws_url)?;

        let (ws_stream, _response) = connect_async(ws_request).await.map_err(|e| {
            ProtocolError::new(
                ErrorCode::ProviderStreamError,
                format!("Aliyun WebSocket connection failed: {e}"),
            )
        })?;

        let (events_tx, events) = EventStream::channel(64);
        let (meter, events) = VoiceMeter::attach(
            events,
            "aliyun",
            &self.config.model,
            None,
            (self.config.model == "cosyvoice-v2" && self.config.ws_url == DEFAULT_WS_URL)
                .then_some(COSYVOICE_V2_PRICE),
        );
        let meter = meter.with_text(Some(&request.text));
        tokio::spawn(run_aliyun_synthesis(
            WsDuplex::new(ws_stream),
            run_task,
            continue_task,
            finish_task,
            request.format,
            events_tx,
            meter,
        ));
        Ok(events)
    }
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split(['/', '?']).next())
        .unwrap_or("dashscope.aliyuncs.com")
}

pub(crate) const CATALOG_EXT: TtsCatalogExt = TtsCatalogExt {
    source: VoiceSource::Direct,
    protocol: "dashscope_tts_ws",
    words: false,
    duplex: true,
    flush: true,
    cancel: true,
};

/// The static descriptor the registry filters on for the aliyun TTS dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("aliyun", "cosyvoice-v2", Capability::Tts)
        .streaming(true)
        .duplex(CATALOG_EXT.duplex)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Audio])
        .with_ext(CATALOG_EXT.to_descriptor_ext())
}

/// Build an [`AliyunTts`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<AliyunTts, ProtocolError> {
    let api_key = cfg
        .api_key
        .clone()
        .ok_or_else(|| ProtocolError::new(ErrorCode::MissingApiKey, "aliyun requires api_key"))?;
    let model = if cfg.model.is_empty() {
        "cosyvoice-v2".to_string()
    } else {
        cfg.model.clone()
    };
    let ws_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_WS_URL.to_string());
    Ok(AliyunTts::new(AliyunTtsConfig {
        model,
        ws_url,
        api_key,
    }))
}

#[async_trait]
impl Tts for AliyunTts {
    fn provider_name(&self) -> &str {
        "aliyun"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        let mut descriptor = entry_descriptor();
        descriptor.model = self.config.model.clone().into();
        descriptor
    }

    async fn synthesize(
        &self,
        request: SynthesizeRequest,
    ) -> Result<SynthesizeResult, ProtocolError> {
        let format = request.format;
        let stream = self.open_stream(request).await?;
        super::collect_synthesis(stream, format, self.provider_name(), self.model_name()).await
    }

    async fn stream_synthesize(
        &self,
        request: SynthesizeRequest,
    ) -> Result<EventStream, ProtocolError> {
        self.open_stream(request).await
    }

    async fn start_duplex_stream(
        &self,
        request: orchest_protocol::TtsDuplexRequest,
    ) -> Result<orchest_protocol::TtsDuplexHandle, ProtocolError> {
        self.open_duplex(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished() -> String {
        json!({"header": {"event": "task-finished"}}).to_string()
    }

    #[test]
    fn command_shapes() {
        let run = build_run_task(
            "t",
            "cosyvoice-v2",
            "longxiaochun",
            &AudioFormat::Mp3,
            &json!({}),
        );
        assert_eq!(run["header"]["action"], "run-task");
        assert_eq!(run["payload"]["task"], "tts");
        assert_eq!(
            build_continue_task("t", "hi")["payload"]["input"]["text"],
            "hi"
        );
        assert_eq!(build_finish_task("t")["header"]["action"], "finish-task");
    }

    #[test]
    fn parses_finished_and_failed() {
        assert_eq!(
            parse_server_event(&finished()).unwrap(),
            AliyunTtsEvent::Finished
        );
        match parse_server_event(
            &json!({"header": {"event": "task-failed", "error_message": "boom"}}).to_string(),
        )
        .unwrap()
        {
            AliyunTtsEvent::Failed(m) => assert_eq!(m, "boom"),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(
            parse_server_event(&json!({"header": {"event": "task-started"}}).to_string()).unwrap(),
            AliyunTtsEvent::Other
        );
    }

    struct ChannelDuplex {
        out: mpsc::Sender<WsFrame>,
        inbound: mpsc::Receiver<WsFrame>,
    }

    #[async_trait]
    impl ByteDuplex for ChannelDuplex {
        async fn send(&mut self, frame: WsFrame) -> Result<(), ProtocolError> {
            self.out
                .send(frame)
                .await
                .map_err(|_| ProtocolError::new(ErrorCode::ProviderStreamError, "closed"))
        }
        async fn recv(&mut self) -> Option<WsFrame> {
            self.inbound.recv().await
        }
    }

    #[tokio::test]
    async fn run_aliyun_synthesis_streams_binary_audio_then_done() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let handle = tokio::spawn(run_aliyun_synthesis(
            transport,
            build_run_task("t", "m", "v", &AudioFormat::Mp3, &json!({})),
            build_continue_task("t", "hi"),
            build_finish_task("t"),
            AudioFormat::Mp3,
            events_tx,
            VoiceMeter::default(),
        ));

        // the loop sends run-task / continue-task / finish-task as text frames
        for expected in ["run-task", "continue-task", "finish-task"] {
            match out_rx.recv().await.unwrap() {
                WsFrame::Text(t) => assert!(t.contains(expected), "missing {expected}"),
                other => panic!("expected text frame, got {other:?}"),
            }
        }

        // server streams a binary audio frame, then task-finished
        in_tx.send(WsFrame::Binary(vec![1, 2, 3])).await.unwrap();
        in_tx.send(WsFrame::Text(finished())).await.unwrap();
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::AudioDelta { .. }
        ));
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Done { .. }
        ));
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_missing_voice_before_network() {
        let tts = AliyunTts::new(AliyunTtsConfig {
            model: "cosyvoice-v2".into(),
            ws_url: "wss://example.invalid/tts".into(),
            api_key: "test-key".into(),
        });
        let err = tts
            .synthesize(SynthesizeRequest {
                text: "hi".into(),
                voice: None,
                format: AudioFormat::Mp3,
                options: Value::Null,
            })
            .await
            .expect_err("missing voice must fail before dialing the provider");
        assert_eq!(err.code, ErrorCode::InvalidRequest);
        assert!(
            err.message.contains("voice"),
            "expected voice-focused message, got {}",
            err.message
        );
    }
}
