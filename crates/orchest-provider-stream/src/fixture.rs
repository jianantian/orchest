//! Recorded WebSocket sessions for offline ASR contract tests (v1.2 Issue 003).
//!
//! A fixture is a JSON Lines file: one [`FixtureLine`] per line, in the order
//! things happened in the session. Client inputs ([`FixtureInput`]) and the
//! kind of each uplink frame are kept; uplink payloads are not, so audio, API
//! keys and config frames never land in a fixture. Downlink frames are kept in
//! full, minus the identifiers in [`REDACTED_KEYS`]. Openspeech (volcengine)
//! binary frames are stored decoded as [`OpenspeechFrame`] so the file stays
//! readable.
//!
//! [`ReplayDuplex`] plays a fixture back as a [`ByteDuplex`], so the real
//! dialect loops run against it without a network. With the `record-fixtures`
//! feature, the live dialects wrap their socket in a recorder that writes the
//! same format (see `examples/record_asr_fixture.rs`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use orchest_protocol::{ErrorCode, ProtocolError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Notify;

use crate::openspeech::{
    build_header, compress_gzip, decompress_gzip, parse_header, COMP_GZIP, FLAG_LAST_NO_SEQUENCE,
    FLAG_NO_SEQUENCE, FLAG_SEQUENCE_NEGATIVE, FLAG_SEQUENCE_POSITIVE, MSG_FULL_SERVER_RESPONSE,
    SER_JSON,
};
use crate::transport::{ByteDuplex, WsFrame};

/// Downlink object keys whose values are replaced with `"redacted"`: request
/// and session identifiers that tie a fixture to an account.
pub const REDACTED_KEYS: &[&str] = &[
    "api_key",
    "log_id",
    "logid",
    "request_id",
    "session_id",
    "sha256",
    "task_id",
];

/// One line of a fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureLine {
    /// Free-form provenance (dialect, model, whether the file was recorded or
    /// hand-written, the source audio).
    Meta(Value),
    /// A client input given to the session.
    Input(FixtureInput),
    /// The session sent one uplink frame of this kind.
    Send(FrameKind),
    /// The server sent this JSON text frame.
    Recv(Value),
    /// The server sent this openspeech binary frame.
    RecvOpenspeech(OpenspeechFrame),
}

/// A client input, without its payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixtureInput {
    /// An audio chunk of `bytes` bytes; replay sends silence of that length.
    Audio { bytes: usize },
    /// `SessionInput::Finalize`.
    Finalize,
    /// End of input (the input channel closed).
    End,
}

/// The kind of a WebSocket frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    Text,
    Binary,
}

impl FrameKind {
    pub fn of(frame: &WsFrame) -> Self {
        match frame {
            WsFrame::Text(_) => Self::Text,
            WsFrame::Binary(_) => Self::Binary,
        }
    }
}

/// A decoded openspeech full-server-response frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenspeechFrame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i32>,
    #[serde(default)]
    pub last: bool,
    pub payload: Value,
}

impl OpenspeechFrame {
    /// Encode as a gzip-compressed JSON full server response.
    pub fn encode(&self) -> Vec<u8> {
        let flags = match (self.sequence, self.last) {
            (Some(_), false) => FLAG_SEQUENCE_POSITIVE,
            (Some(_), true) => FLAG_SEQUENCE_NEGATIVE,
            (None, false) => FLAG_NO_SEQUENCE,
            (None, true) => FLAG_LAST_NO_SEQUENCE,
        };
        // Invariant: gzip into an in-memory Vec has no I/O to fail.
        let body = compress_gzip(self.payload.to_string().as_bytes())
            .expect("gzip into a Vec cannot fail");
        let mut frame = build_header(MSG_FULL_SERVER_RESPONSE, flags, SER_JSON, COMP_GZIP).to_vec();
        if let Some(sequence) = self.sequence {
            frame.extend_from_slice(&sequence.to_be_bytes());
        }
        frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frame.extend_from_slice(&body);
        frame
    }

    /// Decode a full server response; `None` for any other frame.
    pub fn decode(data: &[u8]) -> Option<Self> {
        let header = parse_header(data).ok()?;
        if header.msg_type != MSG_FULL_SERVER_RESPONSE {
            return None;
        }
        let has_sequence = matches!(
            header.flags,
            FLAG_SEQUENCE_POSITIVE | FLAG_SEQUENCE_NEGATIVE
        );
        let last = matches!(header.flags, FLAG_LAST_NO_SEQUENCE | FLAG_SEQUENCE_NEGATIVE);
        let mut offset = 4;
        let sequence = if has_sequence {
            let bytes = data.get(offset..offset + 4)?;
            offset += 4;
            Some(i32::from_be_bytes(bytes.try_into().ok()?))
        } else {
            None
        };
        let size = u32::from_be_bytes(data.get(offset..offset + 4)?.try_into().ok()?) as usize;
        let body = data.get(offset + 4..offset + 4 + size)?;
        let json = if header.compression == COMP_GZIP {
            decompress_gzip(body).ok()?
        } else {
            body.to_vec()
        };
        Some(Self {
            sequence,
            last,
            payload: serde_json::from_slice(&json).ok()?,
        })
    }
}

/// Replace the values of [`REDACTED_KEYS`] anywhere in `value`.
pub fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                if REDACTED_KEYS.contains(&key.as_str()) {
                    *inner = Value::String("redacted".into());
                } else {
                    redact(inner);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        _ => {}
    }
}

/// Parse a JSON Lines fixture; blank lines are skipped.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_fixture(text: &str) -> Result<Vec<FixtureLine>, ProtocolError> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(number, line)| {
            serde_json::from_str(line).map_err(|e| {
                ProtocolError::new(
                    ErrorCode::InvalidRequest,
                    format!("fixture line {}: {e}", number + 1),
                )
            })
        })
        .collect()
}

/// Render fixture lines as JSON Lines.
pub fn render_fixture(lines: &[FixtureLine]) -> String {
    lines
        .iter()
        // Invariant: a line holds only JSON values, strings and integers, which
        // always serialize.
        .map(|line| serde_json::to_string(line).expect("fixture lines serialize") + "\n")
        .collect()
}

#[derive(Debug, Default)]
struct Script {
    steps: VecDeque<FixtureLine>,
    /// Uplink frames the fixture did not expect, in order.
    violations: Vec<String>,
}

/// Shared replay state: the [`ReplayDuplex`] consumes `send` / `recv` lines,
/// [`ReplayInputs`] consumes `input` lines, each waiting until the line in
/// front of the script is theirs, so the session replays in recorded order.
#[derive(Debug, Clone, Default)]
struct Shared {
    script: Arc<Mutex<Script>>,
    notify: Arc<Notify>,
}

impl Shared {
    /// Wait until `take` accepts the front of the script, and return its result.
    /// `take` returns `None` to keep waiting; an empty script yields `None`.
    async fn wait_for<R>(&self, mut take: impl FnMut(&mut Script) -> Option<R>) -> Option<R> {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut script = self.script.lock().unwrap_or_else(PoisonError::into_inner);
                if script.steps.is_empty() {
                    return None;
                }
                if let Some(result) = take(&mut script) {
                    drop(script);
                    self.notify.notify_waiters();
                    return Some(result);
                }
            }
            notified.await;
        }
    }
}

/// Build the two halves of a replay from a fixture. `meta` lines are dropped.
pub fn replay(lines: Vec<FixtureLine>) -> (ReplayDuplex, ReplayInputs) {
    let shared = Shared::default();
    shared
        .script
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .steps = lines
        .into_iter()
        .filter(|line| !matches!(line, FixtureLine::Meta(_)))
        .collect();
    (
        ReplayDuplex {
            shared: shared.clone(),
        },
        ReplayInputs { shared },
    )
}

/// The transport half of a replay: serves the recorded downlink frames and
/// checks each uplink frame against the recorded kind.
#[derive(Debug)]
pub struct ReplayDuplex {
    shared: Shared,
}

impl ReplayDuplex {
    /// A handle that reports uplink frames the fixture did not expect.
    pub fn violations(&self) -> ReplayViolations {
        ReplayViolations {
            shared: self.shared.clone(),
        }
    }
}

/// Reads the uplink mismatches of a replay after the session ends.
#[derive(Debug, Clone)]
pub struct ReplayViolations {
    shared: Shared,
}

impl ReplayViolations {
    pub fn take(&self) -> Vec<String> {
        let mut script = self
            .shared
            .script
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut violations = std::mem::take(&mut script.violations);
        if !script.steps.is_empty() {
            violations.push(format!(
                "{} fixture line(s) were not replayed, next: {:?}",
                script.steps.len(),
                script.steps.front()
            ));
        }
        violations
    }
}

#[async_trait]
impl ByteDuplex for ReplayDuplex {
    /// An uplink frame matches the first `send` line, which may come after
    /// `recv` lines not yet read: the server can answer later than recorded.
    async fn send(&mut self, frame: WsFrame) -> Result<(), ProtocolError> {
        let kind = FrameKind::of(&frame);
        let mut script = self
            .shared
            .script
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let next = script.steps.iter().position(|line| {
            !matches!(line, FixtureLine::Recv(_) | FixtureLine::RecvOpenspeech(_))
        });
        match next.map(|index| (index, &script.steps[index])) {
            Some((index, FixtureLine::Send(expected))) if *expected == kind => {
                script.steps.remove(index);
                drop(script);
                self.shared.notify.notify_waiters();
                Ok(())
            }
            other => {
                let message =
                    format!("unexpected {kind:?} uplink frame; fixture expected {other:?}");
                script.violations.push(message.clone());
                Err(ProtocolError::new(ErrorCode::ProviderStreamError, message))
            }
        }
    }

    async fn recv(&mut self) -> Option<WsFrame> {
        self.shared
            .wait_for(|script| {
                let frame = match script.steps.front()? {
                    FixtureLine::Recv(value) => WsFrame::Text(value.to_string()),
                    FixtureLine::RecvOpenspeech(frame) => WsFrame::Binary(frame.encode()),
                    _ => return None,
                };
                script.steps.pop_front();
                Some(frame)
            })
            .await
    }
}

/// The client half of a replay: yields the recorded inputs in order.
#[derive(Debug)]
pub struct ReplayInputs {
    shared: Shared,
}

impl ReplayInputs {
    /// The next recorded input, once every line before it has been replayed;
    /// `None` when the script is done.
    pub async fn next(&mut self) -> Option<FixtureInput> {
        self.shared
            .wait_for(|script| match script.steps.front()? {
                FixtureLine::Input(_) => match script.steps.pop_front() {
                    Some(FixtureLine::Input(input)) => Some(input),
                    _ => unreachable!("front was an input"),
                },
                _ => None,
            })
            .await
    }
}

#[cfg(feature = "record-fixtures")]
pub use recording::{install_recorder, record_input, Recording};

/// Wrap a live transport for recording (`record-fixtures` feature only).
#[cfg(feature = "record-fixtures")]
pub fn live<T: ByteDuplex>(inner: T) -> Recording<T> {
    Recording::new(inner)
}

/// Wrap a live transport for recording: a no-op without `record-fixtures`.
#[cfg(not(feature = "record-fixtures"))]
pub fn live<T: ByteDuplex>(inner: T) -> T {
    inner
}

#[cfg(feature = "record-fixtures")]
mod recording {
    use std::sync::{Arc, Mutex, OnceLock, PoisonError};

    use async_trait::async_trait;
    use orchest_protocol::ProtocolError;
    use serde_json::Value;

    use super::{redact, FixtureInput, FixtureLine, FrameKind, OpenspeechFrame};
    use crate::transport::{ByteDuplex, WsFrame};

    type Sink = Arc<Mutex<Vec<FixtureLine>>>;

    static SINK: OnceLock<Sink> = OnceLock::new();

    /// Start recording every live ASR session of this process into one log.
    pub fn install_recorder() -> Sink {
        SINK.get_or_init(Sink::default).clone()
    }

    fn push(line: FixtureLine) {
        if let Some(sink) = SINK.get() {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(line);
        }
    }

    /// Log a client input in session order.
    pub fn record_input(input: FixtureInput) {
        push(FixtureLine::Input(input));
    }

    /// A transport that logs frame kinds up and redacted frames down.
    pub struct Recording<T> {
        inner: T,
    }

    impl<T> Recording<T> {
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }

    #[async_trait]
    impl<T: ByteDuplex> ByteDuplex for Recording<T> {
        async fn send(&mut self, frame: WsFrame) -> Result<(), ProtocolError> {
            push(FixtureLine::Send(FrameKind::of(&frame)));
            self.inner.send(frame).await
        }

        async fn recv(&mut self) -> Option<WsFrame> {
            let frame = self.inner.recv().await?;
            let line = match &frame {
                WsFrame::Text(text) => serde_json::from_str::<Value>(text).ok().map(|mut value| {
                    redact(&mut value);
                    FixtureLine::Recv(value)
                }),
                WsFrame::Binary(bytes) => OpenspeechFrame::decode(bytes).map(|mut frame| {
                    redact(&mut frame.payload);
                    FixtureLine::RecvOpenspeech(frame)
                }),
            };
            push(line.unwrap_or_else(|| {
                FixtureLine::Meta(serde_json::json!({"unrecorded_frame": FrameKind::of(&frame)}))
            }));
            Some(frame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lines_round_trip_through_json_lines() {
        let lines = vec![
            FixtureLine::Meta(json!({"dialect": "deepgram"})),
            FixtureLine::Input(FixtureInput::Audio { bytes: 3200 }),
            FixtureLine::Input(FixtureInput::Finalize),
            FixtureLine::Send(FrameKind::Binary),
            FixtureLine::Recv(json!({"type": "Results"})),
            FixtureLine::RecvOpenspeech(OpenspeechFrame {
                sequence: Some(-2),
                last: true,
                payload: json!({"result": {"text": "hi"}}),
            }),
        ];
        let text = render_fixture(&lines);
        assert!(text.contains(r#"{"input":{"kind":"audio","bytes":3200}}"#));
        assert!(text.contains(r#"{"send":"binary"}"#));
        assert_eq!(parse_fixture(&text).unwrap(), lines);
    }

    #[test]
    fn openspeech_frames_encode_and_decode() {
        for (sequence, last) in [
            (Some(3), false),
            (Some(-4), true),
            (None, false),
            (None, true),
        ] {
            let frame = OpenspeechFrame {
                sequence,
                last,
                payload: json!({"result": {"text": "测试"}}),
            };
            assert_eq!(OpenspeechFrame::decode(&frame.encode()), Some(frame));
        }
        assert_eq!(OpenspeechFrame::decode(b"\x11\xf0\x10\x00"), None);
    }

    #[test]
    fn redaction_reaches_nested_identifiers() {
        let mut value =
            json!({"header": {"task_id": "t-1", "event": "x"}, "items": [{"request_id": 7}]});
        redact(&mut value);
        assert_eq!(value["header"]["task_id"], "redacted");
        assert_eq!(value["header"]["event"], "x");
        assert_eq!(value["items"][0]["request_id"], "redacted");
    }

    #[tokio::test]
    async fn replay_orders_inputs_uplink_and_downlink() {
        let (mut duplex, mut inputs) = replay(vec![
            FixtureLine::Input(FixtureInput::Audio { bytes: 2 }),
            FixtureLine::Send(FrameKind::Binary),
            FixtureLine::Recv(json!({"n": 1})),
            FixtureLine::Input(FixtureInput::End),
        ]);
        let violations = duplex.violations();
        assert_eq!(inputs.next().await, Some(FixtureInput::Audio { bytes: 2 }));
        duplex.send(WsFrame::Binary(vec![0, 0])).await.unwrap();
        assert_eq!(
            duplex.recv().await,
            Some(WsFrame::Text(r#"{"n":1}"#.into()))
        );
        assert_eq!(inputs.next().await, Some(FixtureInput::End));
        assert_eq!(duplex.recv().await, None);
        assert!(violations.take().is_empty());
    }

    #[tokio::test]
    async fn replay_reports_an_unexpected_uplink_frame() {
        let (mut duplex, _inputs) = replay(vec![FixtureLine::Send(FrameKind::Binary)]);
        assert!(duplex.send(WsFrame::Text("x".into())).await.is_err());
        let violations = duplex.violations().take();
        assert_eq!(violations.len(), 2, "{violations:?}");
    }
}
