//! Aliyun DashScope streaming-ASR (FunASR / Qwen-ASR) on the spine (Issue 006).
//! Ported from `agent-runtime-asr-providers`'s `providers/aliyun`, over the spine
//! [`ProtocolError`] / [`StreamEvent`].
//!
//! DashScope frames control as **text** JSON (`run-task` → `result-generated*` →
//! `finish-task` → `task-finished`) and audio as **binary**. Each
//! `result-generated` carries a `sentence` whose `sentence_end` flags whether it
//! is committed.

use async_trait::async_trait;
use orchest_protocol::{
    Asr, Capability, CapabilityDescriptor, ErrorCode, EventStream, Language, LifecycleEvent,
    Modality, ProtocolError, RealtimeHandle, SessionInput, StreamEvent, StreamingTranscribeRequest,
    TranscribeRequest, TranscribeResult, TranscriptStability,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::ws::{connect_async, tungstenite};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::transport::{ByteDuplex, WsDuplex, WsFrame};

const DEFAULT_WS_URL: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference/";

#[derive(Serialize)]
struct ClientMessage {
    header: ClientHeader,
    payload: Value,
}

#[derive(Serialize)]
struct ClientHeader {
    action: String,
    task_id: String,
    streaming: String,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ServerEvent {
    pub header: ServerHeader,
    #[serde(default)]
    pub payload: Option<ServerPayload>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ServerHeader {
    pub event: String,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ServerPayload {
    #[serde(default)]
    pub output: Option<Output>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Output {
    #[serde(default)]
    pub sentence: Option<Sentence>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Sentence {
    pub text: String,
    #[serde(default)]
    pub sentence_end: bool,
}

/// Build the `run-task` text frame opening a recognition session.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn build_run_task(
    task_id: &str,
    model: &str,
    options: &Value,
) -> Result<String, ProtocolError> {
    let mut parameters = json!({ "sample_rate": 16000, "format": "pcm" });
    if let Some(obj) = options.as_object() {
        for (k, v) in obj {
            parameters[k] = v.clone();
        }
    }
    let msg = ClientMessage {
        header: ClientHeader {
            action: "run-task".into(),
            task_id: task_id.into(),
            streaming: "duplex".into(),
        },
        payload: json!({
            "task_group": "audio",
            "task": "asr",
            "function": "recognition",
            "model": model,
            "parameters": parameters,
            "input": {},
        }),
    };
    serde_json::to_string(&msg)
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidRequest, format!("serialize: {e}")))
}

/// Build the `finish-task` text frame closing the session.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn build_finish_task(task_id: &str) -> Result<String, ProtocolError> {
    let msg = ClientMessage {
        header: ClientHeader {
            action: "finish-task".into(),
            task_id: task_id.into(),
            streaming: "duplex".into(),
        },
        payload: json!({ "input": {} }),
    };
    serde_json::to_string(&msg)
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidRequest, format!("serialize: {e}")))
}

/// Parse a DashScope server event text frame.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_server_event(text: &str) -> Result<ServerEvent, ProtocolError> {
    serde_json::from_str(text).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("failed to parse DashScope event: {e}"),
        )
    })
}

/// Project a DashScope event onto unified events: `result-generated` →
/// `Transcript` (`Committed` once `sentence_end`, else `Provisional`);
/// `task-finished` → `EndOfSpeech`; `task-failed` → a fatal `Error`.
pub fn map_event(event: ServerEvent) -> Vec<StreamEvent> {
    match event.header.event.as_str() {
        "result-generated" => {
            let sentence = event
                .payload
                .and_then(|p| p.output)
                .and_then(|o| o.sentence);
            match sentence {
                Some(s) if !s.text.is_empty() => vec![StreamEvent::Transcript {
                    text: s.text,
                    stability: if s.sentence_end {
                        TranscriptStability::Committed
                    } else {
                        TranscriptStability::Provisional
                    },
                    segment: None,
                }],
                _ => Vec::new(),
            }
        }
        "task-finished" => vec![StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment: None,
        })],
        "task-failed" => {
            let message = event
                .header
                .error_message
                .or(event.header.error_code)
                .unwrap_or_else(|| "aliyun task failed".to_string());
            vec![StreamEvent::Error {
                error: ProtocolError::new(ErrorCode::ProviderTaskFailed, message),
                fatal: true,
            }]
        }
        _ => Vec::new(),
    }
}

/// Drive one DashScope session: send `run_task` (text), client audio as
/// **binary**, `finish_task` (text) on input end; inbound **text** events project
/// via [`map_event`]. Ends on `task-finished` / `task-failed` / transport EOF.
pub async fn run_aliyun_stream<T: ByteDuplex>(
    mut transport: T,
    run_task: String,
    finish_task: String,
    mut input: mpsc::Receiver<SessionInput>,
    events: mpsc::Sender<StreamEvent>,
) {
    if transport.send(WsFrame::Text(run_task)).await.is_err() {
        return;
    }
    let mut input_open = true;
    loop {
        tokio::select! {
            maybe_input = input.recv(), if input_open => match maybe_input {
                Some(SessionInput::Audio(bytes)) => {
                    if transport.send(WsFrame::Binary(bytes.to_vec())).await.is_err() {
                        break;
                    }
                }
                None | Some(SessionInput::Interrupt) => {
                    input_open = false;
                    let _ = transport.send(WsFrame::Text(finish_task.clone())).await;
                }
                Some(_) => {}
            },
            maybe_frame = transport.recv() => match maybe_frame.as_ref().and_then(WsFrame::as_text) {
                Some(text) => match parse_server_event(text) {
                    Ok(event) => {
                        let terminal = matches!(
                            event.header.event.as_str(),
                            "task-finished" | "task-failed"
                        );
                        for unified in map_event(event) {
                            if events.send(unified).await.is_err() {
                                return;
                            }
                        }
                        if terminal {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = events.send(StreamEvent::Error { error, fatal: true }).await;
                        break;
                    }
                },
                None => match maybe_frame {
                    Some(_) => continue,
                    None => break,
                },
            },
        }
    }
}

/// Aliyun streaming-ASR configuration.
#[derive(Debug, Clone)]
pub struct AliyunAsrConfig {
    pub model: String,
    pub ws_url: String,
    pub api_key: String,
}

/// The aliyun DashScope streaming ASR provider as the spine [`Asr`].
pub struct AliyunAsr {
    config: AliyunAsrConfig,
}

impl AliyunAsr {
    pub fn new(config: AliyunAsrConfig) -> Self {
        Self { config }
    }
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split(['/', '?']).next())
        .unwrap_or("dashscope.aliyuncs.com")
}

/// The static descriptor the registry filters on for the aliyun dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("aliyun", "paraformer-realtime-v2", Capability::Asr)
        .streaming(true)
        .duplex(true)
        .with_input_modalities([Modality::Audio])
        .with_output_modalities([Modality::Text])
}

/// Build an [`AliyunAsr`] from a registry [`ProviderConfig`].
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<AliyunAsr, ProtocolError> {
    let api_key = cfg
        .api_key
        .clone()
        .ok_or_else(|| ProtocolError::new(ErrorCode::MissingApiKey, "aliyun requires api_key"))?;
    let model = if cfg.model.is_empty() {
        "paraformer-realtime-v2".to_string()
    } else {
        cfg.model.clone()
    };
    let ws_url = cfg
        .api_url
        .clone()
        .unwrap_or_else(|| DEFAULT_WS_URL.to_string());
    Ok(AliyunAsr::new(AliyunAsrConfig {
        model,
        ws_url,
        api_key,
    }))
}

#[async_trait]
impl Asr for AliyunAsr {
    fn provider_name(&self) -> &str {
        "aliyun"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("aliyun", self.config.model.clone(), Capability::Asr)
            .streaming(true)
            .duplex(true)
            .with_input_modalities([Modality::Audio])
            .with_output_modalities([Modality::Text])
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(
        &self,
        _request: TranscribeRequest,
    ) -> Result<TranscribeResult, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "aliyun ASR is streaming-only; use start_stream",
        ))
    }

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError> {
        if !self.config.ws_url.starts_with("wss://") {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "WebSocket URL must use wss:// for secure credential transport",
            ));
        }
        let task_id = uuid::Uuid::new_v4().simple().to_string();
        let run_task = build_run_task(&task_id, &self.config.model, &request.options)?;
        let finish_task = build_finish_task(&task_id)?;

        let ws_request = tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
            .header("Authorization", format!("bearer {}", self.config.api_key))
            .header("Host", host_of(&self.config.ws_url))
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
            })?;

        let (ws_stream, _response) = connect_async(ws_request).await.map_err(|e| {
            ProtocolError::new(
                ErrorCode::ProviderStreamError,
                format!("Aliyun WebSocket connection failed: {e}"),
            )
        })?;

        let (input_tx, input_rx) = mpsc::channel(32);
        let (events_tx, events) = EventStream::channel(64);
        tokio::spawn(run_aliyun_stream(
            WsDuplex::new(ws_stream),
            run_task,
            finish_task,
            input_rx,
            events_tx,
        ));
        Ok(RealtimeHandle {
            input: input_tx,
            events,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: &str, text: Option<&str>, sentence_end: bool) -> String {
        let payload = text
            .map(|t| json!({"output": {"sentence": {"text": t, "sentence_end": sentence_end}}}));
        json!({"header": {"event": name}, "payload": payload}).to_string()
    }

    #[test]
    fn run_task_and_finish_task_shapes() {
        let run = build_run_task("t1", "paraformer-realtime-v2", &json!({"vad": true})).unwrap();
        assert!(run.contains("run-task") && run.contains("paraformer-realtime-v2"));
        assert!(build_finish_task("t1").unwrap().contains("finish-task"));
    }

    #[test]
    fn maps_result_generated_with_stability() {
        let provisional =
            parse_server_event(&event("result-generated", Some("hi"), false)).unwrap();
        assert!(matches!(
            map_event(provisional).as_slice(),
            [StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                ..
            }]
        ));
        let committed = parse_server_event(&event("result-generated", Some("hi"), true)).unwrap();
        assert!(matches!(
            map_event(committed).as_slice(),
            [StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }]
        ));
    }

    #[test]
    fn maps_finished_and_failed() {
        let finished = parse_server_event(&event("task-finished", None, false)).unwrap();
        assert!(matches!(
            map_event(finished).as_slice(),
            [StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { .. })]
        ));
        let failed = parse_server_event(
            &json!({"header": {"event": "task-failed", "error_message": "boom"}}).to_string(),
        )
        .unwrap();
        assert!(matches!(
            map_event(failed).as_slice(),
            [StreamEvent::Error { fatal: true, .. }]
        ));
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
    async fn run_aliyun_stream_sends_run_task_audio_and_finishes() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let run = build_run_task("t", "m", &json!({})).unwrap();
        let finish = build_finish_task("t").unwrap();
        let handle = tokio::spawn(run_aliyun_stream(
            transport, run, finish, input_rx, events_tx,
        ));

        // first frame is the run-task text
        match out_rx.recv().await.unwrap() {
            WsFrame::Text(t) => assert!(t.contains("run-task")),
            other => panic!("expected run-task, got {other:?}"),
        }
        // audio in -> binary
        input_tx
            .send(SessionInput::Audio(bytes::Bytes::from_static(b"pcm")))
            .await
            .unwrap();
        assert!(matches!(out_rx.recv().await.unwrap(), WsFrame::Binary(_)));

        // a committed result, then task-finished -> Transcript + EndOfSpeech, then ends
        in_tx
            .send(WsFrame::Text(event("result-generated", Some("done"), true)))
            .await
            .unwrap();
        in_tx
            .send(WsFrame::Text(event("task-finished", None, false)))
            .await
            .unwrap();
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }
        ));
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { .. })
        ));
        handle.await.unwrap();
    }
}
