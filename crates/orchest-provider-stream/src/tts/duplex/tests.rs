use super::*;
use crate::openspeech::{
    build_header, COMP_NONE, FLAG_WITH_EVENT, MSG_AUDIO_ONLY_RESPONSE, MSG_FULL_SERVER_RESPONSE,
    SER_JSON, SER_NONE,
};
use crate::tts::{
    aliyun::duplex::AliyunWire, minimax::duplex::MinimaxWire, volcengine::duplex::VolcengineWire,
};
use async_trait::async_trait;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dialect {
    Aliyun,
    Minimax,
    Volcengine,
}

struct FakeTransport {
    sent: mpsc::Sender<WsFrame>,
    receive: mpsc::Receiver<WsFrame>,
}

#[async_trait]
impl ByteDuplex for FakeTransport {
    async fn send(&mut self, frame: WsFrame) -> Result<(), ProtocolError> {
        self.sent
            .send(frame)
            .await
            .map_err(|_| stream_error("fixture disconnected"))
    }

    async fn recv(&mut self) -> Option<WsFrame> {
        self.receive.recv().await
    }
}

struct Fixture {
    dialect: Dialect,
    sent: mpsc::Receiver<WsFrame>,
    receive: mpsc::Sender<WsFrame>,
    id: String,
}

impl Fixture {
    fn new(dialect: Dialect) -> (Self, FakeTransport) {
        let (send, sent) = mpsc::channel(16);
        let (receive, incoming) = mpsc::channel(16);
        (
            Self {
                dialect,
                sent,
                receive,
                id: String::new(),
            },
            FakeTransport {
                sent: send,
                receive: incoming,
            },
        )
    }

    async fn next_command(&mut self) -> WsFrame {
        self.sent.recv().await.expect("missing provider command")
    }

    async fn send(&self, frame: WsFrame) {
        self.receive.send(frame).await.unwrap();
    }

    async fn event(&self, aliyun: &str, minimax: &str, volcengine: i32) {
        let frame = match self.dialect {
            Dialect::Aliyun => WsFrame::Text(
                json!({
                    "header": {"event": aliyun, "task_id": self.id},
                    "payload": {"usage": {"characters": 7}},
                })
                .to_string(),
            ),
            Dialect::Minimax => WsFrame::Text(
                json!({
                    "event": minimax, "base_resp": {"status_code": 0},
                })
                .to_string(),
            ),
            Dialect::Volcengine => server_frame(
                volcengine,
                &self.id,
                br#"{"usage":{"text_words":7}}"#,
                false,
            ),
        };
        self.send(frame).await;
    }

    async fn audio(&self, byte: u8) {
        let frame = match self.dialect {
            Dialect::Aliyun => WsFrame::Binary(vec![byte]),
            Dialect::Minimax => WsFrame::Text(
                json!({
                    "event": "task_continued", "base_resp": {"status_code": 0},
                    "data": {"audio": format!("{byte:02x}")}, "is_final": false,
                })
                .to_string(),
            ),
            Dialect::Volcengine => server_frame(352, &self.id, &[byte], true),
        };
        self.send(frame).await;
    }

    async fn assert_command(
        &mut self,
        aliyun: &str,
        minimax: &str,
        volcengine: i32,
    ) -> Option<Value> {
        let frame = self.next_command().await;
        if self.dialect == Dialect::Volcengine {
            let frame = frame.as_binary().unwrap();
            assert_eq!(
                i32::from_be_bytes(frame[4..8].try_into().unwrap()),
                volcengine
            );
            if volcengine >= 100 {
                let id_len = u32::from_be_bytes(frame[8..12].try_into().unwrap()) as usize;
                let id = std::str::from_utf8(&frame[12..12 + id_len]).unwrap();
                if volcengine == 100 {
                    assert_ne!(self.id, id);
                    self.id = id.to_owned();
                } else {
                    assert_eq!(self.id, id);
                }
                return Some(serde_json::from_slice(&frame[16 + id_len..]).unwrap());
            }
            return None;
        }
        let value: Value = serde_json::from_str(frame.as_text().unwrap()).unwrap();
        if self.dialect == Dialect::Aliyun {
            assert_eq!(value["header"]["action"], aliyun);
            let task = value["header"]["task_id"].as_str().unwrap();
            if aliyun == "run-task" {
                assert_ne!(self.id, task);
                self.id = task.to_owned();
            } else {
                assert_eq!(self.id, task);
            }
        } else {
            assert_eq!(value["event"], minimax);
        }
        Some(value)
    }

    async fn start(&mut self) {
        self.assert_command("run-task", "task_start", 100).await;
        self.event("task-started", "task_started", 150).await;
    }

    async fn append(&mut self, sender: &mpsc::Sender<TtsInput>, text: &str, restart: bool) {
        sender.send(TtsInput::Append(text.into())).await.unwrap();
        if restart && self.dialect != Dialect::Minimax {
            self.start().await;
        }
        let request = self
            .assert_command("continue-task", "task_continue", 200)
            .await
            .unwrap();
        let path = match self.dialect {
            Dialect::Aliyun => "/payload/input/text",
            Dialect::Minimax => "/text",
            Dialect::Volcengine => "/req_params/text",
        };
        assert_eq!(request.pointer(path).and_then(Value::as_str), Some(text));
    }
}

fn server_frame(event: i32, id: &str, payload: &[u8], audio: bool) -> WsFrame {
    let (message, serialization) = if audio {
        (MSG_AUDIO_ONLY_RESPONSE, SER_NONE)
    } else {
        (MSG_FULL_SERVER_RESPONSE, SER_JSON)
    };
    let mut bytes = build_header(message, FLAG_WITH_EVENT, serialization, COMP_NONE).to_vec();
    bytes.extend(event.to_be_bytes());
    bytes.extend((id.len() as u32).to_be_bytes());
    bytes.extend(id.as_bytes());
    bytes.extend((payload.len() as u32).to_be_bytes());
    bytes.extend(payload);
    WsFrame::Binary(bytes)
}

async fn open_fixture(
    dialect: Dialect,
    transport: FakeTransport,
) -> Result<TtsDuplexHandle, ProtocolError> {
    let config = StreamConfig {
        provider: "fixture",
        model: "test",
        format: AudioFormat::Mp3,
        price: None,
    };
    match dialect {
        Dialect::Aliyun => {
            open(
                transport,
                AliyunWire::new(json!({"header":{"action":"run-task"}})),
                config,
            )
            .await
        }
        Dialect::Minimax => {
            open(
                transport,
                MinimaxWire::new(json!({"event":"task_start"})),
                config,
            )
            .await
        }
        Dialect::Volcengine => {
            open(
                transport,
                VolcengineWire::new(json!({"user":{},"req_params":{}})),
                config,
            )
            .await
        }
    }
}

async fn connected_fixture(dialect: Dialect) -> (Fixture, TtsDuplexHandle) {
    let (mut fixture, transport) = Fixture::new(dialect);
    let opening = tokio::spawn(open_fixture(dialect, transport));
    match dialect {
        Dialect::Aliyun => {}
        Dialect::Minimax => fixture.event("", "connected_success", 0).await,
        Dialect::Volcengine => {
            fixture.assert_command("", "", 1).await;
            fixture.event("", "", 50).await;
        }
    }
    fixture.start().await;
    (fixture, opening.await.unwrap().unwrap())
}

async fn assert_audio(
    events: &mut EventStream,
    byte: u8,
    sequence: u64,
    id: u64,
    flush_id: Option<u64>,
) {
    assert_eq!(
        events.next().await,
        Some(StreamEvent::AudioDelta {
            data: Bytes::from(vec![byte]),
            format: AudioFormat::Mp3,
            sequence,
            tts: Some(TtsAudioContext {
                context_id: id,
                flush_id
            }),
        })
    );
}

async fn check_contract(dialect: Dialect) {
    let (mut fixture, TtsDuplexHandle { input, mut events }) = connected_fixture(dialect).await;
    fixture.append(&input, "First.", false).await;
    if dialect == Dialect::Minimax {
        fixture.event("", "sentence_start", 0).await;
    }
    fixture.audio(1).await;
    assert_audio(&mut events, 1, 1, 1, None).await;

    fixture.append(&input, " tail", false).await;
    input.send(TtsInput::Flush).await.unwrap();
    fixture
        .assert_command("finish-task", "task_flush", 102)
        .await;
    assert_eq!(
        events.next().await,
        Some(StreamEvent::Tts(TtsEvent::FlushAccepted {
            context_id: 1,
            flush_id: 1
        }))
    );
    fixture.audio(2).await;
    assert_audio(&mut events, 2, 2, 1, Some(1)).await;
    fixture.event("task-finished", "task_flushed", 152).await;
    assert_eq!(
        events.next().await,
        Some(StreamEvent::Tts(TtsEvent::Flushed {
            context_id: 1,
            flush_id: 1
        }))
    );

    fixture.append(&input, "Cancel me.", true).await;
    if dialect == Dialect::Minimax {
        fixture.event("", "sentence_start", 0).await;
    }
    input.send(TtsInput::Cancel).await.unwrap();
    let command = fixture
        .assert_command("finish-task", "task_cancel", 101)
        .await;
    if dialect == Dialect::Aliyun {
        assert_eq!(command.unwrap()["payload"]["input"]["directive"], "cancel");
    }
    let old_context = fixture.id.clone();
    fixture.audio(9).await;
    fixture.event("task-finished", "task_canceled", 151).await;
    assert_eq!(
        events.next().await,
        Some(StreamEvent::Tts(TtsEvent::Cancelled { context_id: 2 }))
    );

    fixture.append(&input, "Next.", true).await;
    match dialect {
        Dialect::Volcengine => {
            fixture
                .send(server_frame(352, &old_context, &[9], true))
                .await
        }
        Dialect::Minimax => {
            fixture.audio(9).await;
            fixture.event("", "sentence_start", 0).await;
        }
        Dialect::Aliyun => {}
    }
    fixture.audio(3).await;
    assert_audio(&mut events, 3, 3, 3, None).await;

    input.send(TtsInput::Close).await.unwrap();
    let _ = input.send(TtsInput::Close).await;
    fixture
        .assert_command("finish-task", "task_finish", 102)
        .await;
    fixture.event("task-finished", "task_finished", 152).await;
    assert_eq!(
        events.next().await,
        Some(StreamEvent::Tts(TtsEvent::ContextEnded { context_id: 3 }))
    );
    if dialect == Dialect::Volcengine {
        fixture.assert_command("", "", 2).await;
        fixture.event("", "", 52).await;
    }
    assert_eq!(
        events.next().await,
        Some(StreamEvent::Lifecycle(LifecycleEvent::SessionClosed))
    );
    assert!(events.next().await.is_none());
    let usage = events.take_voice_usage().await.unwrap();
    assert_eq!(usage.input_characters, Some(26));
    assert_eq!(usage.outcome, orchest_protocol::VoiceOutcome::Completed);
    assert!(fixture.sent.recv().await.is_none());
}

#[tokio::test]
async fn three_dialects_obey_duplex_boundaries() {
    for dialect in [Dialect::Aliyun, Dialect::Minimax, Dialect::Volcengine] {
        timeout(Duration::from_secs(3), check_contract(dialect))
            .await
            .unwrap_or_else(|_| panic!("{dialect:?} fixture timed out"));
    }
}

#[tokio::test]
async fn handshake_and_unconfirmed_close_fail_explicitly() {
    timeout(Duration::from_secs(3), async {
        let (mut fixture, transport) = Fixture::new(Dialect::Aliyun);
        let opening = tokio::spawn(open_fixture(Dialect::Aliyun, transport));
        fixture.assert_command("run-task", "", 0).await;
        fixture.event("task-failed", "", 0).await;
        assert_eq!(
            opening.await.unwrap().unwrap_err().code,
            ErrorCode::ProviderTaskFailed
        );

        let (mut fixture, TtsDuplexHandle { input, mut events }) =
            connected_fixture(Dialect::Volcengine).await;
        drop(input);
        fixture.assert_command("", "", 102).await;
        drop(fixture.receive);
        assert!(matches!(
            events.next().await,
            Some(StreamEvent::Error { fatal: true, .. })
        ));
        assert_eq!(
            events.take_voice_usage().await.unwrap().outcome,
            orchest_protocol::VoiceOutcome::Failed
        );
        assert!(fixture.sent.recv().await.is_none());
    })
    .await
    .unwrap();
}
