use orchest_protocol::{TtsDuplexHandle, TtsDuplexRequest};
use serde_json::json;

use super::*;
use crate::tts::duplex::{self, Boundary, Reply, Signal, StreamConfig, Wire};

const DEFAULT_DUPLEX_URL: &str = "wss://openspeech.bytedance.com/api/v3/tts/bidirection";
const EVENT_FINISH_CONNECTION: i32 = 2;
const EVENT_SESSION_CANCELLED: i32 = 151;

pub(in crate::tts) struct VolcengineWire {
    setup: Value,
    session_id: String,
}

impl VolcengineWire {
    pub(in crate::tts) fn new(mut setup: Value) -> Self {
        setup["event"] = json!(EVENT_START_SESSION);
        setup["namespace"] = json!("BidirectionalTTS");
        if let Some(params) = setup.get_mut("req_params").and_then(Value::as_object_mut) {
            params.remove("text");
        }
        Self {
            setup,
            session_id: String::new(),
        }
    }
}

impl Wire for VolcengineWire {
    fn connect(&self) -> Result<Option<WsFrame>, ProtocolError> {
        build_connect_frame(EVENT_START_CONNECTION, &json!({}))
            .map(WsFrame::Binary)
            .map(Some)
    }

    fn start(&mut self) -> Result<Option<WsFrame>, ProtocolError> {
        self.session_id = uuid::Uuid::new_v4().to_string();
        build_meta_frame(EVENT_START_SESSION, &self.session_id, &self.setup)
            .map(WsFrame::Binary)
            .map(Some)
    }

    fn append(&self, text: &str) -> Result<WsFrame, ProtocolError> {
        build_meta_frame(
            EVENT_TASK_REQUEST,
            &self.session_id,
            &json!({
                "user": self.setup["user"],
                "event": EVENT_TASK_REQUEST,
                "namespace": "BidirectionalTTS",
                "req_params": { "text": text },
            }),
        )
        .map(WsFrame::Binary)
    }

    fn boundary(&self, operation: Boundary) -> Result<(WsFrame, Signal), ProtocolError> {
        let (event, expected) = if operation == Boundary::Cancel {
            (EVENT_CANCEL_SESSION, Signal::Cancelled)
        } else {
            (EVENT_FINISH_SESSION, Signal::Ended)
        };
        Ok((
            WsFrame::Binary(build_meta_frame(event, &self.session_id, &json!({}))?),
            expected,
        ))
    }

    fn close_connection(&self, _: bool) -> Result<Option<(WsFrame, Signal)>, ProtocolError> {
        Ok(Some((
            WsFrame::Binary(build_connect_frame(EVENT_FINISH_CONNECTION, &json!({}))?),
            Signal::Closed,
        )))
    }

    fn decode(&mut self, frame: WsFrame) -> Result<Reply, ProtocolError> {
        let Some(bytes) = frame.as_binary() else {
            return Ok(Signal::Ignored.into());
        };
        match parse_frame(bytes)? {
            VolcengineFrame::Audio {
                event: EVENT_TTS_RESPONSE,
                session_id,
                data,
            } if session_id == self.session_id => Ok(Signal::Audio(Bytes::from(data)).into()),
            VolcengineFrame::Audio { .. } => Ok(Signal::Ignored.into()),
            VolcengineFrame::Error { code, message } => Err(ProtocolError::new(
                ErrorCode::ProviderTaskFailed,
                format!("Volcengine TTS {code}: {message}"),
            )),
            VolcengineFrame::Meta {
                event,
                session_id,
                payload,
            } => {
                let signal = match event {
                    EVENT_CONNECTION_STARTED => Signal::Connected,
                    EVENT_CONNECTION_FINISHED => Signal::Closed,
                    EVENT_CONNECTION_FAILED => {
                        return Err(duplex::stream_error("Volcengine rejected StartConnection"))
                    }
                    _ if session_id != self.session_id => return Ok(Signal::Ignored.into()),
                    EVENT_SESSION_STARTED => Signal::Started,
                    EVENT_SESSION_FINISHED => Signal::Ended,
                    EVENT_SESSION_CANCELLED => Signal::Cancelled,
                    EVENT_SESSION_FAILED => {
                        return Err(ProtocolError::new(
                            ErrorCode::ProviderTaskFailed,
                            payload
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("Volcengine TTS session failed"),
                        ))
                    }
                    _ => Signal::Ignored,
                };
                Ok(Reply {
                    signal,
                    billed_characters: payload.pointer("/usage/text_words").and_then(Value::as_u64),
                })
            }
        }
    }
}

impl VolcengineTts {
    pub(super) async fn open_duplex(
        &self,
        mut request: TtsDuplexRequest,
    ) -> Result<TtsDuplexHandle, ProtocolError> {
        CATALOG_EXT.validate_duplex_options(&mut request.options)?;
        if !matches!(
            request.format,
            AudioFormat::Mp3
                | AudioFormat::OggOpus
                | AudioFormat::Ogg
                | AudioFormat::Pcm
                | AudioFormat::Pcm16Le
        ) {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedAudioFormat,
                "unsupported Volcengine duplex audio format",
            ));
        }
        if request.voice.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "Volcengine duplex requires a voice",
            ));
        }
        let url = if self.config.ws_url == DEFAULT_UNIDIRECTIONAL_WS_URL {
            DEFAULT_DUPLEX_URL
        } else {
            &self.config.ws_url
        };
        let resource_id = if self.config.resource_id == "tts" && url == DEFAULT_DUPLEX_URL {
            "seed-tts-2.0"
        } else {
            &self.config.resource_id
        };
        let format = request.format;
        let request = SynthesizeRequest {
            text: String::new(),
            voice: Some(request.voice),
            format,
            options: request.options,
        };
        let voice = require_explicit_voice(&request)?;
        let mut setup = build_synthesis_payload(&request, voice);
        setup["req_params"]["speaker"] = json!(voice);
        let audio = setup["req_params"]["audio_params"]
            .as_object_mut()
            .ok_or_else(|| {
                ProtocolError::new(
                    ErrorCode::InvalidRequest,
                    "Volcengine audio_params must be an object",
                )
            })?;
        audio.insert("format".into(), json!(audio_format_name(&format)));

        let socket = self
            .connector
            .connect(self.websocket_request(url, resource_id)?)
            .await
            .map_err(|error| duplex::stream_error(format!("Volcengine duplex connect: {error}")))?;
        duplex::open(
            WsDuplex::new(socket),
            VolcengineWire::new(setup),
            StreamConfig {
                provider: "volcengine",
                model: resource_id,
                format,
                price: None,
            },
        )
        .await
    }
}
