use orchest_protocol::{TtsDuplexHandle, TtsDuplexRequest};

use super::*;
use crate::tts::duplex::{self, Boundary, Reply, Signal, StreamConfig, Wire};

const GLOBAL_DUPLEX_URL: &str = "wss://api.minimax.io/ws/v1/t2a_v2_bidi";
const CHINA_DUPLEX_URL: &str = "wss://api.minimaxi.com/ws/v1/t2a_v2_bidi";

pub(in crate::tts) struct MinimaxWire {
    setup: Option<Value>,
    sentence_started: bool,
    billed: Option<u64>,
}

impl MinimaxWire {
    pub(in crate::tts) fn new(mut setup: Value) -> Self {
        setup["session_id"] = json!(uuid::Uuid::new_v4().to_string());
        Self {
            setup: Some(setup),
            sentence_started: false,
            billed: Some(0),
        }
    }
}

impl Wire for MinimaxWire {
    fn expects_hello(&self) -> bool {
        true
    }

    fn start(&mut self) -> Result<Option<WsFrame>, ProtocolError> {
        Ok(self
            .setup
            .take()
            .map(|setup| WsFrame::Text(setup.to_string())))
    }

    fn append(&self, text: &str) -> Result<WsFrame, ProtocolError> {
        Ok(WsFrame::Text(build_task_continue_body(text).to_string()))
    }

    fn boundary(&self, operation: Boundary) -> Result<(WsFrame, Signal), ProtocolError> {
        let (event, expected) = match operation {
            Boundary::Flush => ("task_flush", Signal::Flushed),
            Boundary::Cancel => ("task_cancel", Signal::Cancelled),
            Boundary::Close => ("task_finish", Signal::Ended),
        };
        Ok((WsFrame::Text(json!({"event": event}).to_string()), expected))
    }

    fn close_connection(
        &self,
        context_closed: bool,
    ) -> Result<Option<(WsFrame, Signal)>, ProtocolError> {
        if context_closed {
            return Ok(None);
        }
        self.boundary(Boundary::Close).map(Some)
    }

    fn decode(&mut self, frame: WsFrame) -> Result<Reply, ProtocolError> {
        let Some(text) = frame.as_text() else {
            return Ok(Signal::Ignored.into());
        };
        let inbound: InboundFrame = serde_json::from_str(text).map_err(|error| {
            duplex::stream_error(format!("invalid MiniMax duplex event: {error}"))
        })?;
        if let Some(error) = map_base_resp(&inbound.base_resp) {
            return Err(error);
        }
        let signal = match inbound.event.as_str() {
            "connected_success" => Signal::Connected,
            "task_started" => Signal::Started,
            "sentence_start" => {
                self.sentence_started = true;
                Signal::Ignored
            }
            "sentence_end" => {
                self.sentence_started = false;
                Signal::Ignored
            }
            "task_continued" if self.sentence_started => {
                if inbound.is_final {
                    let characters = inbound
                        .extra_info
                        .as_ref()
                        .and_then(|extra| extra.get("usage_characters"))
                        .and_then(Value::as_u64);
                    self.billed = self
                        .billed
                        .zip(characters)
                        .and_then(|(total, characters)| total.checked_add(characters));
                }
                Signal::Audio(decode_hex_audio(
                    inbound.data.as_ref().and_then(|data| data.audio.as_deref()),
                )?)
            }
            "task_flushed" => Signal::Flushed,
            "task_canceled" => Signal::Cancelled,
            "task_finished" => Signal::Ended,
            "task_failed" => {
                return Err(ProtocolError::new(
                    ErrorCode::ProviderTaskFailed,
                    "MiniMax duplex task failed",
                ))
            }
            _ => Signal::Ignored,
        };
        let mut billed_characters = None;
        if matches!(signal, Signal::Flushed | Signal::Cancelled | Signal::Ended) {
            self.sentence_started = false;
            let billed = self.billed.replace(0);
            if signal != Signal::Cancelled {
                billed_characters = billed.filter(|count| *count > 0);
            }
        }
        Ok(Reply {
            signal,
            billed_characters,
        })
    }
}

fn connection_error(error: tungstenite::Error) -> ProtocolError {
    if let tungstenite::Error::Http(response) = &error {
        let body = response
            .body()
            .as_deref()
            .and_then(|body| serde_json::from_slice::<Value>(body).ok());
        let resp = body.and_then(|value| {
            serde_json::from_value::<BaseResp>(value.get("base_resp")?.clone()).ok()
        });
        if let Some(error) = resp.as_ref().and_then(map_base_resp) {
            return error.with_provider("minimax");
        }
    }
    duplex::stream_error(format!("MiniMax duplex connect: {error}")).with_provider("minimax")
}

impl MinimaxTts {
    pub(super) async fn open_duplex(
        &self,
        mut request: TtsDuplexRequest,
    ) -> Result<TtsDuplexHandle, ProtocolError> {
        CATALOG_EXT.validate_duplex_options(&mut request.options)?;
        if !matches!(
            request.format,
            AudioFormat::Mp3 | AudioFormat::Pcm | AudioFormat::Pcm16Le | AudioFormat::Flac
        ) {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedAudioFormat,
                "unsupported MiniMax duplex audio format",
            ));
        }
        if request.voice.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "MiniMax duplex requires a voice",
            ));
        }
        let url = match self.config.ws_url.as_str() {
            DEFAULT_WSS_URL => CHINA_DUPLEX_URL,
            GLOBAL_WSS_URL => GLOBAL_DUPLEX_URL,
            url => url,
        };
        let mut setup = build_task_start_body(
            &self.config.model,
            &request.voice,
            &request.format,
            None,
            &request.options,
        );
        setup["voice_setting"]["voice_id"] = json!(request.voice);
        let socket = self
            .connector
            .connect(self.websocket_request(url)?)
            .await
            .map_err(connection_error)?;
        duplex::open(
            WsDuplex::new(socket),
            MinimaxWire::new(setup),
            StreamConfig {
                provider: "minimax",
                model: &self.config.model,
                format: request.format,
                price: (self.config.model == "speech-2.8-hd" && url == GLOBAL_DUPLEX_URL)
                    .then_some(SPEECH_HD_PRICE),
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest_protocol::{LifecycleEvent, TtsEvent, TtsInput};
    use std::time::Duration;

    #[tokio::test]
    #[ignore = "requires a MiniMax TTS API key and bills a short speech sample"]
    async fn live_duplex_flush_and_resume() {
        let key = std::env::var("MINIMAX_API_KEY").expect("MiniMax TTS API key");
        let url = if std::env::var("ORCHEST_MINIMAX_TTS_REGION").as_deref() == Ok("china") {
            CHINA_DUPLEX_URL
        } else {
            GLOBAL_DUPLEX_URL
        };
        let harness = async {
            let adapter = MinimaxTts::new(MinimaxTtsConfig {
                model: "speech-2.8-hd".into(),
                ws_url: url.into(),
                api_key: key,
            });
            let TtsDuplexHandle { input, mut events } = adapter
                .start_duplex_stream(TtsDuplexRequest {
                    voice: "English_expressive_narrator".into(),
                    format: AudioFormat::Mp3,
                    options: Value::Null,
                })
                .await
                .unwrap();
            input
                .send(TtsInput::Append(
                    "Hello. This is a short voice test.".into(),
                ))
                .await
                .unwrap();
            loop {
                match events.next().await.expect("provider closed before audio") {
                    StreamEvent::AudioDelta {
                        tts: Some(context), ..
                    } => {
                        assert_eq!(context.context_id, 1);
                        break;
                    }
                    StreamEvent::Error { error, .. } => panic!("provider failed: {error}"),
                    _ => {}
                }
            }
            input.send(TtsInput::Flush).await.unwrap();
            loop {
                match events.next().await.expect("provider closed before Flush") {
                    StreamEvent::Tts(TtsEvent::Flushed { context_id: 1, .. }) => break,
                    StreamEvent::Error { error, .. } => panic!("provider failed: {error}"),
                    _ => {}
                }
            }
            input
                .send(TtsInput::Append("This is the next turn.".into()))
                .await
                .unwrap();
            input.send(TtsInput::Close).await.unwrap();
            let mut heard_next = false;
            let mut closed = false;
            while let Some(event) = events.next().await {
                match event {
                    StreamEvent::AudioDelta {
                        tts: Some(context), ..
                    } if context.context_id == 2 => heard_next = true,
                    StreamEvent::Lifecycle(LifecycleEvent::SessionClosed) => closed = true,
                    StreamEvent::Error { error, .. } => panic!("provider failed: {error}"),
                    _ => {}
                }
            }
            assert!(heard_next && closed);
        };
        tokio::time::timeout(Duration::from_secs(35), harness)
            .await
            .expect("MiniMax live duplex timeout");
    }
}
