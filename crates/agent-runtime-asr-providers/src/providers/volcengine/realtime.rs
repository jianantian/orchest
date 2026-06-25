//! Experimental Doubao / Volcengine realtime omni scaffolding for v0.9.11.
//!
//! This module is intentionally provider-local and feature-gated by the existing
//! `volcengine` feature. It captures the first session lifecycle surface needed
//! for v0.9.11 evidence gathering without introducing a stable provider-core
//! abstraction.

use std::env;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::error::{AsrError, AsrErrorCode};

pub const DEFAULT_REALTIME_WS_URL: &str = "wss://openspeech.bytedance.com/api/v3/realtime/dialogue";
pub const DEFAULT_REALTIME_RESOURCE_ID: &str = "volc.speech.dialog";
pub const DEFAULT_REALTIME_APP_KEY: &str = "PlgvMymc7f3tQnJ6";
pub const DEFAULT_REALTIME_MODEL: &str = "1.2.1.1";
pub const DEFAULT_REALTIME_SPEAKER: &str = "zh_female_vv_jupiter_bigtts";
const EVENT_START_CONNECTION: u32 = 1;
const EVENT_FINISH_CONNECTION: u32 = 2;
const EVENT_START_SESSION: u32 = 100;
const EVENT_FINISH_SESSION: u32 = 102;
const EVENT_TASK_REQUEST: u32 = 200;
const EVENT_CLIENT_INTERRUPT: u32 = 515;

const MSG_FULL_CLIENT_REQUEST: u8 = 0b0001;
const MSG_AUDIO_ONLY_REQUEST: u8 = 0b0010;
const MSG_FULL_SERVER_RESPONSE: u8 = 0b1001;
const MSG_AUDIO_ONLY_RESPONSE: u8 = 0b1011;
const MSG_ERROR_RESPONSE: u8 = 0b1111;
const FLAG_EVENT: u8 = 0b0100;
const SER_NONE: u8 = 0b0000;
const SER_JSON: u8 = 0b0001;
const COMP_NONE: u8 = 0b0000;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolcengineRealtimeConfig {
    pub ws_url: String,
    pub app_id: String,
    pub access_key: String,
    pub resource_id: String,
    pub app_key: String,
    pub connect_id: Option<String>,
    pub model: String,
    pub speaker: String,
}

impl VolcengineRealtimeConfig {
    pub fn new(app_id: impl Into<String>, access_key: impl Into<String>) -> Self {
        Self {
            ws_url: DEFAULT_REALTIME_WS_URL.to_string(),
            app_id: app_id.into(),
            access_key: access_key.into(),
            resource_id: DEFAULT_REALTIME_RESOURCE_ID.to_string(),
            app_key: DEFAULT_REALTIME_APP_KEY.to_string(),
            connect_id: None,
            model: DEFAULT_REALTIME_MODEL.to_string(),
            speaker: DEFAULT_REALTIME_SPEAKER.to_string(),
        }
    }

    pub fn from_env() -> Result<Self, AsrError> {
        let app_id = required_env("VOLCENGINE_REALTIME_APP_ID")?;
        let access_key = required_env("VOLCENGINE_REALTIME_ACCESS_KEY")?;
        let mut config = Self::new(app_id, access_key);
        config.resource_id = optional_env("VOLCENGINE_REALTIME_RESOURCE_ID")
            .unwrap_or_else(|| DEFAULT_REALTIME_RESOURCE_ID.to_string());
        config.app_key = optional_env("VOLCENGINE_REALTIME_APP_KEY")
            .unwrap_or_else(|| DEFAULT_REALTIME_APP_KEY.to_string());
        config.connect_id = optional_env("VOLCENGINE_REALTIME_CONNECT_ID");
        config.model = optional_env("VOLCENGINE_REALTIME_MODEL")
            .unwrap_or_else(|| DEFAULT_REALTIME_MODEL.to_string());
        config.speaker = optional_env("VOLCENGINE_REALTIME_SPEAKER")
            .unwrap_or_else(|| DEFAULT_REALTIME_SPEAKER.to_string());
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), AsrError> {
        if !self.ws_url.starts_with("wss://") {
            return Err(AsrError::new(
                AsrErrorCode::InvalidRequest,
                "Volcengine realtime WebSocket URL must use wss://",
            ));
        }
        if self.app_id.trim().is_empty() {
            return Err(AsrError::new(
                AsrErrorCode::MissingApiKey,
                "VOLCENGINE_REALTIME_APP_ID must not be empty",
            ));
        }
        if self.access_key.trim().is_empty() {
            return Err(AsrError::new(
                AsrErrorCode::MissingApiKey,
                "VOLCENGINE_REALTIME_ACCESS_KEY must not be empty",
            ));
        }
        Ok(())
    }

    pub fn effective_connect_id(&self) -> String {
        self.connect_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string())
    }

    pub fn start_session_payload(&self) -> Value {
        serde_json::json!({
            "asr": {
                "audio_info": {
                    "format": "pcm_s16le",
                    "sample_rate": 16000,
                    "channel": 1
                }
            },
            "dialog": {
                "bot_name": "Doubao",
                "dialog_id": "",
                "extra": {
                    "input_mod": "audio_file",
                    "model": self.model,
                    "strict_audit": true
                }
            },
            "tts": {
                "speaker": self.speaker,
                "audio_config": {
                    "channel": 1,
                    "format": "pcm_s16le",
                    "sample_rate": 24000
                }
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolcengineRealtimeState {
    Created,
    Started,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VolcengineRealtimeEvent {
    SessionStarted {
        session_id: String,
    },
    AudioInputAccepted {
        bytes: usize,
    },
    ClientInterrupted {
        session_id: String,
    },
    SessionClosed {
        session_id: String,
    },
    ProviderError {
        message: String,
    },
    ServerEvent {
        event: VolcengineRealtimeMappedEvent,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolcengineRealtimeInputMode {
    AudioFile,
    Text,
    PushToTalk,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolcengineRealtimeErrorCategory {
    Authentication,
    Transport,
    Protocol,
    Provider,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VolcengineRealtimeMappedEvent {
    Lifecycle {
        event_id: u16,
        name: String,
    },
    AudioOutput {
        event_id: u16,
        bytes: Vec<u8>,
    },
    Transcript {
        event_id: u16,
        text: String,
        is_interim: bool,
    },
    ModelText {
        event_id: u16,
        content: String,
    },
    Error {
        event_id: u16,
        message: String,
    },
    Metadata {
        event_id: u16,
        name: String,
        payload: Value,
    },
    Unsupported {
        event_id: u16,
        name: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolcengineRealtimeFixtureEvent {
    pub event_id: u16,
    pub name: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug)]
enum VolcengineRealtimeCommand {
    Audio(Vec<u8>),
    Interrupt,
    CloseSession,
    CloseConnection,
}

pub struct VolcengineRealtimeSession {
    session_id: String,
    state: VolcengineRealtimeState,
    events: mpsc::Sender<VolcengineRealtimeEvent>,
    commands: Option<mpsc::Sender<VolcengineRealtimeCommand>>,
}

impl VolcengineRealtimeSession {
    pub fn fake(session_id: impl Into<String>) -> (Self, mpsc::Receiver<VolcengineRealtimeEvent>) {
        let (events, rx) = mpsc::channel(16);
        (
            Self {
                session_id: session_id.into(),
                state: VolcengineRealtimeState::Created,
                events,
                commands: None,
            },
            rx,
        )
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn state(&self) -> &VolcengineRealtimeState {
        &self.state
    }

    pub async fn start(&mut self) -> Result<(), AsrError> {
        match self.state {
            VolcengineRealtimeState::Created => {
                self.state = VolcengineRealtimeState::Started;
                self.emit(VolcengineRealtimeEvent::SessionStarted {
                    session_id: self.session_id.clone(),
                })
                .await
            }
            VolcengineRealtimeState::Started => Err(AsrError::new(
                AsrErrorCode::InvalidRequest,
                "Volcengine realtime session is already started",
            )),
            VolcengineRealtimeState::Closed => Err(AsrError::new(
                AsrErrorCode::InvalidRequest,
                "Volcengine realtime session is already closed",
            )),
        }
    }

    pub async fn send_audio_chunk(&self, chunk: &[u8]) -> Result<(), AsrError> {
        if self.state != VolcengineRealtimeState::Started {
            return Err(AsrError::new(
                AsrErrorCode::InvalidRequest,
                "Volcengine realtime session must be started before sending audio",
            ));
        }
        if chunk.is_empty() {
            return Err(AsrError::new(
                AsrErrorCode::InvalidAudio,
                "audio chunk must not be empty",
            ));
        }
        if let Some(commands) = &self.commands {
            commands
                .send(VolcengineRealtimeCommand::Audio(chunk.to_vec()))
                .await
                .map_err(|_| {
                    AsrError::new(
                        AsrErrorCode::ProviderStreamError,
                        "Volcengine realtime command loop was closed",
                    )
                })?;
        }
        self.emit(VolcengineRealtimeEvent::AudioInputAccepted { bytes: chunk.len() })
            .await
    }

    pub async fn interrupt(&self, input_mode: VolcengineRealtimeInputMode) -> Result<(), AsrError> {
        if self.state != VolcengineRealtimeState::Started {
            return Err(AsrError::new(
                AsrErrorCode::InvalidRequest,
                "Volcengine realtime session must be started before interrupting",
            ));
        }
        if input_mode != VolcengineRealtimeInputMode::PushToTalk {
            return Err(AsrError::new(
                AsrErrorCode::UnsupportedOperation,
                "Volcengine ClientInterrupt is documented only for push_to_talk mode",
            ));
        }
        if let Some(commands) = &self.commands {
            commands
                .send(VolcengineRealtimeCommand::Interrupt)
                .await
                .map_err(|_| {
                    AsrError::new(
                        AsrErrorCode::ProviderStreamError,
                        "Volcengine realtime command loop was closed",
                    )
                })?;
        }
        self.emit(VolcengineRealtimeEvent::ClientInterrupted {
            session_id: self.session_id.clone(),
        })
        .await
    }

    pub async fn close(&mut self) -> Result<(), AsrError> {
        match self.state {
            VolcengineRealtimeState::Started | VolcengineRealtimeState::Created => {
                self.state = VolcengineRealtimeState::Closed;
                if let Some(commands) = &self.commands {
                    commands
                        .send(VolcengineRealtimeCommand::CloseSession)
                        .await
                        .map_err(|_| {
                            AsrError::new(
                                AsrErrorCode::ProviderStreamError,
                                "Volcengine realtime command loop was closed",
                            )
                        })?;
                    let _ = commands
                        .send(VolcengineRealtimeCommand::CloseConnection)
                        .await;
                }
                self.emit(VolcengineRealtimeEvent::SessionClosed {
                    session_id: self.session_id.clone(),
                })
                .await
            }
            VolcengineRealtimeState::Closed => Err(AsrError::new(
                AsrErrorCode::InvalidRequest,
                "Volcengine realtime session is already closed",
            )),
        }
    }

    async fn emit(&self, event: VolcengineRealtimeEvent) -> Result<(), AsrError> {
        self.events.send(event).await.map_err(|_| {
            AsrError::new(
                AsrErrorCode::ProviderStreamError,
                "Volcengine realtime event receiver was dropped",
            )
        })
    }
}

pub fn map_realtime_server_event(
    event_id: u16,
    name: impl Into<String>,
    payload: Value,
    audio_payload: Option<Vec<u8>>,
) -> VolcengineRealtimeMappedEvent {
    let name = name.into();
    match event_id {
        50 | 52 | 150 | 152 | 359 | 459 | 559 => {
            VolcengineRealtimeMappedEvent::Lifecycle { event_id, name }
        }
        51 | 153 => VolcengineRealtimeMappedEvent::Error {
            event_id,
            message: payload
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Volcengine realtime provider error")
                .to_string(),
        },
        350 | 351 => VolcengineRealtimeMappedEvent::Metadata {
            event_id,
            name,
            payload,
        },
        352 => VolcengineRealtimeMappedEvent::AudioOutput {
            event_id,
            bytes: audio_payload.unwrap_or_default(),
        },
        450 => VolcengineRealtimeMappedEvent::Metadata {
            event_id,
            name,
            payload,
        },
        451 => {
            let first = payload
                .get("results")
                .and_then(Value::as_array)
                .and_then(|results| results.first());
            VolcengineRealtimeMappedEvent::Transcript {
                event_id,
                text: first
                    .and_then(|result| result.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                is_interim: first
                    .and_then(|result| result.get("is_interim"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }
        }
        550 => VolcengineRealtimeMappedEvent::ModelText {
            event_id,
            content: payload
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        _ => VolcengineRealtimeMappedEvent::Unsupported {
            event_id,
            name,
            reason: "event is not in the v0.9.11 Volcengine realtime evidence set".to_string(),
        },
    }
}

pub fn classify_realtime_error(
    code: Option<u32>,
    message: &str,
) -> VolcengineRealtimeErrorCategory {
    let lower = message.to_ascii_lowercase();
    if lower.contains("auth")
        || lower.contains("appkey")
        || lower.contains("access")
        || lower.contains("token")
        || matches!(code, Some(401) | Some(403))
    {
        VolcengineRealtimeErrorCategory::Authentication
    } else if lower.contains("websocket")
        || lower.contains("connect")
        || lower.contains("timeout")
        || lower.contains("transport")
    {
        VolcengineRealtimeErrorCategory::Transport
    } else if lower.contains("payload")
        || lower.contains("frame")
        || lower.contains("protocol")
        || lower.contains("session")
    {
        VolcengineRealtimeErrorCategory::Protocol
    } else {
        VolcengineRealtimeErrorCategory::Provider
    }
}

fn required_env(name: &str) -> Result<String, AsrError> {
    env::var(name).map_err(|_| {
        AsrError::new(
            AsrErrorCode::MissingApiKey,
            format!("missing required environment variable {name}"),
        )
    })
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

#[path = "realtime_live.rs"]
mod live;

#[cfg(test)]
#[path = "realtime_tests.rs"]
mod tests;
