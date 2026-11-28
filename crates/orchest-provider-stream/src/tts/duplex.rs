//! Ordered duplex synthesis over a provider WebSocket.

use std::time::Duration;

use bytes::Bytes;
use orchest_protocol::{
    AudioFormat, ErrorCode, EventStream, LifecycleEvent, ProtocolError, StreamEvent,
    TtsAudioContext, TtsDuplexHandle, TtsEvent, TtsInput,
};
use orchest_provider_core::voice::VoicePrice;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::metering::VoiceMeter;
use crate::transport::{ByteDuplex, WsFrame};

const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Boundary {
    Flush,
    Cancel,
    Close,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Signal {
    Connected,
    Started,
    Ended,
    Flushed,
    Cancelled,
    Closed,
    Audio(Bytes),
    Ignored,
}

pub(super) struct Reply {
    pub signal: Signal,
    /// Latest cumulative billable characters within a provider context.
    pub billed_characters: Option<u64>,
}

impl From<Signal> for Reply {
    fn from(signal: Signal) -> Self {
        Self {
            signal,
            billed_characters: None,
        }
    }
}

pub(super) trait Wire: Send + 'static {
    fn connect(&self) -> Result<Option<WsFrame>, ProtocolError> {
        Ok(None)
    }

    fn expects_hello(&self) -> bool {
        false
    }

    fn start(&mut self) -> Result<Option<WsFrame>, ProtocolError>;
    fn append(&self, text: &str) -> Result<WsFrame, ProtocolError>;

    /// Ending with Close may finish the provider connection as well.
    fn boundary(&self, operation: Boundary) -> Result<(WsFrame, Signal), ProtocolError>;

    fn close_connection(
        &self,
        context_closed: bool,
    ) -> Result<Option<(WsFrame, Signal)>, ProtocolError>;
    fn decode(&mut self, frame: WsFrame) -> Result<Reply, ProtocolError>;
}

pub(super) struct StreamConfig<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub format: AudioFormat,
    pub price: Option<VoicePrice>,
}

pub(super) async fn open(
    mut transport: impl ByteDuplex + 'static,
    mut wire: impl Wire,
    config: StreamConfig<'_>,
) -> Result<TtsDuplexHandle, ProtocolError> {
    // A dialect may begin with either a client frame or an unsolicited hello.
    if let Some(connect) = wire.connect()? {
        transport.send(connect).await?;
        expect(&mut transport, &mut wire, Signal::Connected).await?;
    } else if wire.expects_hello() {
        expect(&mut transport, &mut wire, Signal::Connected).await?;
    }
    start_context(&mut transport, &mut wire).await?;
    let (input, commands) = mpsc::channel(32);
    let (output, events) = EventStream::channel(64);
    let (meter, events) =
        VoiceMeter::attach(events, config.provider, config.model, None, config.price);
    let mut driver = Driver {
        transport,
        wire,
        commands,
        output,
        meter,
        format: config.format,
        context_id: 1,
        sequence: 0,
        context_open: true,
        has_text: false,
        context_billed: None,
        total_billed: Some(0),
    };
    tokio::spawn(async move {
        if let Err(error) = driver.run().await {
            let _ = driver
                .output
                .send(StreamEvent::Error { error, fatal: true })
                .await;
        }
    });
    Ok(TtsDuplexHandle { input, events })
}

async fn start_context(
    transport: &mut impl ByteDuplex,
    wire: &mut impl Wire,
) -> Result<(), ProtocolError> {
    if let Some(frame) = wire.start()? {
        transport.send(frame).await?;
        expect(transport, wire, Signal::Started).await?;
    }
    Ok(())
}

async fn expect(
    transport: &mut impl ByteDuplex,
    wire: &mut impl Wire,
    expected: Signal,
) -> Result<(), ProtocolError> {
    timeout(CONTROL_TIMEOUT, async {
        loop {
            let reply = receive(transport, wire).await?;
            if reply.signal == expected {
                return Ok(());
            }
            if !matches!(reply.signal, Signal::Ignored | Signal::Audio(_)) {
                return Err(stream_error(format!(
                    "unexpected TTS handshake event: {:?}",
                    reply.signal
                )));
            }
        }
    })
    .await
    .map_err(|_| stream_error("TTS handshake timed out"))?
}

async fn receive(
    transport: &mut impl ByteDuplex,
    wire: &mut impl Wire,
) -> Result<Reply, ProtocolError> {
    let frame = transport
        .recv()
        .await
        .ok_or_else(|| stream_error("TTS transport closed before confirmation"))?;
    wire.decode(frame)
}

struct Driver<T, W> {
    transport: T,
    wire: W,
    commands: mpsc::Receiver<TtsInput>,
    output: mpsc::Sender<StreamEvent>,
    meter: VoiceMeter,
    format: AudioFormat,
    context_id: u64,
    sequence: u64,
    context_open: bool,
    has_text: bool,
    context_billed: Option<u64>,
    total_billed: Option<u64>,
}

impl<T: ByteDuplex, W: Wire> Driver<T, W> {
    async fn run(&mut self) -> Result<(), ProtocolError> {
        loop {
            tokio::select! {
                () = self.output.closed() => return Ok(()),
                input = self.commands.recv() => match input {
                    Some(TtsInput::Append(text)) if !text.is_empty() => self.append(text).await?,
                    Some(TtsInput::Append(_)) => {}
                    Some(TtsInput::Flush) => self.finish_context(Boundary::Flush).await?,
                    Some(TtsInput::Cancel) => self.finish_context(Boundary::Cancel).await?,
                    Some(TtsInput::Close) | None => return self.close().await,
                    Some(_) => self.publish(StreamEvent::Error {
                        error: ProtocolError::new(ErrorCode::UnsupportedOption, "unsupported TTS input"),
                        fatal: false,
                    }).await?,
                },
                reply = receive(&mut self.transport, &mut self.wire) => {
                    let reply = reply?;
                    match reply.signal {
                        Signal::Audio(_) | Signal::Ignored => self.publish_reply(reply, None).await?,
                        _ => return Err(stream_error(format!("unexpected TTS event: {:?}", reply.signal))),
                    }
                }
            }
        }
    }

    async fn append(&mut self, text: String) -> Result<(), ProtocolError> {
        if !self.context_open {
            start_context(&mut self.transport, &mut self.wire).await?;
            self.context_open = true;
        }
        self.transport.send(self.wire.append(&text)?).await?;
        self.meter.add_text(&text);
        self.has_text = true;
        Ok(())
    }

    async fn finish_context(&mut self, operation: Boundary) -> Result<(), ProtocolError> {
        let id = self.context_id;
        if self.context_open && (self.has_text || operation == Boundary::Close) {
            let (frame, expected) = self.wire.boundary(operation)?;
            self.transport.send(frame).await?;
            if operation == Boundary::Flush {
                self.publish(StreamEvent::Tts(TtsEvent::FlushAccepted {
                    context_id: id,
                    flush_id: id,
                }))
                .await?;
            }
            if operation == Boundary::Cancel {
                self.context_billed = None;
            }
            self.wait_boundary(expected, operation).await?;
            self.context_open = false;
        } else if operation == Boundary::Flush {
            self.publish(StreamEvent::Tts(TtsEvent::FlushAccepted {
                context_id: id,
                flush_id: id,
            }))
            .await?;
        }
        self.settle_context();
        self.has_text = false;
        self.context_id = self.context_id.saturating_add(1);
        let event = match operation {
            Boundary::Flush => TtsEvent::Flushed {
                context_id: id,
                flush_id: id,
            },
            Boundary::Cancel => TtsEvent::Cancelled { context_id: id },
            Boundary::Close => TtsEvent::ContextEnded { context_id: id },
        };
        self.publish(StreamEvent::Tts(event)).await
    }

    async fn wait_boundary(
        &mut self,
        expected: Signal,
        operation: Boundary,
    ) -> Result<(), ProtocolError> {
        loop {
            let reply = tokio::select! {
                () = self.output.closed() => return Err(stream_error("TTS event consumer closed")),
                result = timeout(CONTROL_TIMEOUT, receive(&mut self.transport, &mut self.wire)) =>
                    result.map_err(|_| stream_error("TTS boundary acknowledgement timed out"))??,
            };
            if reply.signal == expected {
                self.update_usage(reply.billed_characters);
                return Ok(());
            }
            match reply.signal {
                Signal::Audio(_) | Signal::Ignored => {
                    if operation != Boundary::Cancel {
                        let flush_id = (operation == Boundary::Flush).then_some(self.context_id);
                        self.publish_reply(reply, flush_id).await?;
                    }
                }
                _ => {
                    return Err(stream_error(format!(
                        "unexpected TTS boundary event: {:?}",
                        reply.signal
                    )))
                }
            }
        }
    }

    async fn close(&mut self) -> Result<(), ProtocolError> {
        let context_closed = self.context_open;
        if context_closed {
            self.finish_context(Boundary::Close).await?;
        }
        if let Some((frame, expected)) = self.wire.close_connection(context_closed)? {
            self.transport.send(frame).await?;
            self.wait_boundary(expected, Boundary::Close).await?;
        }
        self.meter.complete();
        self.publish(StreamEvent::Lifecycle(LifecycleEvent::SessionClosed))
            .await
    }

    async fn publish_reply(
        &mut self,
        reply: Reply,
        flush_id: Option<u64>,
    ) -> Result<(), ProtocolError> {
        self.update_usage(reply.billed_characters);
        if let Signal::Audio(data) = reply.signal {
            if self.context_open && self.has_text && !data.is_empty() {
                self.sequence = self.sequence.saturating_add(1);
                self.publish(StreamEvent::AudioDelta {
                    data,
                    format: self.format,
                    sequence: self.sequence,
                    tts: Some(TtsAudioContext {
                        context_id: self.context_id,
                        flush_id,
                    }),
                })
                .await?;
            }
        }
        Ok(())
    }

    fn update_usage(&mut self, billed: Option<u64>) {
        if self.has_text && billed.is_some() {
            self.context_billed = billed;
        }
    }

    fn settle_context(&mut self) {
        if self.has_text {
            self.total_billed = self
                .total_billed
                .zip(self.context_billed.take())
                .and_then(|(total, context)| total.checked_add(context));
            self.meter.billed_characters_if_known(self.total_billed);
        }
    }

    async fn publish(&mut self, event: StreamEvent) -> Result<(), ProtocolError> {
        self.output
            .send(event)
            .await
            .map_err(|_| stream_error("TTS event consumer closed"))
    }
}

pub(super) fn stream_error(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::ProviderStreamError, message)
}

#[cfg(test)]
mod tests;
