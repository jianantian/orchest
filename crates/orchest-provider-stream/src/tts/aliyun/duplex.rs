use orchest_protocol::{TtsDuplexHandle, TtsDuplexRequest};

use super::*;
use crate::tts::duplex::{self, Boundary, Reply, Signal, StreamConfig, Wire};

pub(in crate::tts) struct AliyunWire {
    setup: Value,
    task_id: String,
}

impl AliyunWire {
    pub(in crate::tts) fn new(setup: Value) -> Self {
        Self {
            setup,
            task_id: String::new(),
        }
    }
}

impl Wire for AliyunWire {
    fn start(&mut self) -> Result<Option<WsFrame>, ProtocolError> {
        self.task_id = uuid::Uuid::new_v4().to_string();
        self.setup["header"]["task_id"] = json!(&self.task_id);
        Ok(Some(WsFrame::Text(self.setup.to_string())))
    }

    fn append(&self, text: &str) -> Result<WsFrame, ProtocolError> {
        Ok(WsFrame::Text(
            build_continue_task(&self.task_id, text).to_string(),
        ))
    }

    fn boundary(&self, operation: Boundary) -> Result<(WsFrame, Signal), ProtocolError> {
        let mut command = build_finish_task(&self.task_id);
        if operation == Boundary::Cancel {
            command["payload"]["input"]["directive"] = json!("cancel");
        }
        Ok((WsFrame::Text(command.to_string()), Signal::Ended))
    }

    fn close_connection(&self, _: bool) -> Result<Option<(WsFrame, Signal)>, ProtocolError> {
        Ok(None)
    }

    fn decode(&mut self, frame: WsFrame) -> Result<Reply, ProtocolError> {
        let text = match frame {
            WsFrame::Binary(data) => return Ok(Signal::Audio(Bytes::from(data)).into()),
            WsFrame::Text(text) => text,
        };
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| duplex::stream_error(format!("invalid CosyVoice event: {error}")))?;
        if value.pointer("/header/task_id").and_then(Value::as_str) != Some(&self.task_id) {
            return Ok(Signal::Ignored.into());
        }
        let signal = match value.pointer("/header/event").and_then(Value::as_str) {
            Some("task-started") => Signal::Started,
            Some("task-finished") => Signal::Ended,
            Some("task-failed") => {
                return Err(ProtocolError::new(
                    ErrorCode::ProviderTaskFailed,
                    value
                        .pointer("/header/error_message")
                        .and_then(Value::as_str)
                        .unwrap_or("CosyVoice task failed"),
                ))
            }
            _ => Signal::Ignored,
        };
        Ok(Reply {
            signal,
            billed_characters: value
                .pointer("/payload/usage/characters")
                .and_then(Value::as_u64),
        })
    }
}

impl AliyunTts {
    pub(super) async fn open_duplex(
        &self,
        mut request: TtsDuplexRequest,
    ) -> Result<TtsDuplexHandle, ProtocolError> {
        CATALOG_EXT.validate_duplex_options(&mut request.options)?;
        if !matches!(
            request.format,
            AudioFormat::Mp3
                | AudioFormat::Wav
                | AudioFormat::WavPcm16Le
                | AudioFormat::Pcm
                | AudioFormat::Pcm16Le
        ) {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedAudioFormat,
                "unsupported CosyVoice duplex audio format",
            ));
        }
        if request.voice.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "CosyVoice duplex requires a voice",
            ));
        }
        if request.options.get("enable_ssml").and_then(Value::as_bool) == Some(true) {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedOption,
                "CosyVoice SSML only accepts one text append",
            ));
        }
        let mut setup = build_run_task(
            "",
            &self.config.model,
            &request.voice,
            &request.format,
            &request.options,
        );
        setup["payload"]["parameters"]["voice"] = json!(request.voice);
        setup["payload"]["parameters"]["format"] = json!(format_name(&request.format));
        let wire = AliyunWire::new(setup);
        let socket = self
            .connector
            .connect(self.websocket_request(&self.config.ws_url)?)
            .await
            .map_err(|error| duplex::stream_error(format!("CosyVoice connect: {error}")))?;
        duplex::open(
            WsDuplex::new(socket),
            wire,
            StreamConfig {
                provider: "aliyun",
                model: &self.config.model,
                format: request.format,
                price: (self.config.model == "cosyvoice-v2"
                    && self.config.ws_url == DEFAULT_WS_URL)
                    .then_some(COSYVOICE_V2_PRICE),
            },
        )
        .await
    }
}
