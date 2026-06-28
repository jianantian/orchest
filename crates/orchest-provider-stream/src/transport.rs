//! Shared WebSocket transport for the streaming dialects (Issue 006).
//!
//! The dialect loops (asr/tts/omni) are written against [`ByteDuplex`] — a duplex
//! of WebSocket [`WsFrame`]s (binary **or** text) — rather than a concrete
//! socket. openspeech is all-binary; deepgram/soniox/minimax interleave binary
//! audio with text JSON/control frames, so the transport carries both. In
//! production this is [`WsDuplex`] over `tokio-tungstenite`; in tests it is an
//! in-memory channel pair. Keeping the loops generic over this makes the
//! streaming behavior testable without a network and keeps every dialect on one
//! WS stack (through `orchest-provider-core`).

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use orchest_protocol::{ErrorCode, ProtocolError};
use orchest_provider_core::ws::tungstenite;

/// A WebSocket frame in either direction: binary payload or text payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsFrame {
    Binary(Vec<u8>),
    Text(String),
}

impl WsFrame {
    /// The binary payload, if this is a binary frame.
    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            WsFrame::Binary(b) => Some(b),
            WsFrame::Text(_) => None,
        }
    }

    /// The text payload, if this is a text frame.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            WsFrame::Text(t) => Some(t),
            WsFrame::Binary(_) => None,
        }
    }
}

/// A duplex WebSocket-frame transport a streaming session runs over.
#[async_trait]
pub trait ByteDuplex: Send {
    async fn send(&mut self, frame: WsFrame) -> Result<(), ProtocolError>;
    async fn recv(&mut self) -> Option<WsFrame>;
}

/// A [`ByteDuplex`] over any `tungstenite` WebSocket sink/stream — the live
/// transport the dialect loops run on. Generic over the stream so this crate
/// names no concrete `tokio-tungstenite` type and keeps one WS stack through
/// `orchest-provider-core`.
pub struct WsDuplex<S> {
    inner: S,
}

impl<S> WsDuplex<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<S> ByteDuplex for WsDuplex<S>
where
    S: futures_util::Sink<tungstenite::Message>
        + futures_util::Stream<Item = Result<tungstenite::Message, tungstenite::Error>>
        + Send
        + Unpin,
    <S as futures_util::Sink<tungstenite::Message>>::Error: std::fmt::Display,
{
    async fn send(&mut self, frame: WsFrame) -> Result<(), ProtocolError> {
        let message = match frame {
            WsFrame::Binary(bytes) => tungstenite::Message::Binary(bytes),
            WsFrame::Text(text) => tungstenite::Message::Text(text),
        };
        self.inner.send(message).await.map_err(|e| {
            ProtocolError::new(ErrorCode::ProviderStreamError, format!("ws send: {e}"))
        })
    }

    async fn recv(&mut self) -> Option<WsFrame> {
        while let Some(message) = self.inner.next().await {
            match message {
                Ok(tungstenite::Message::Binary(bytes)) => return Some(WsFrame::Binary(bytes)),
                Ok(tungstenite::Message::Text(text)) => return Some(WsFrame::Text(text)),
                Ok(_) => continue, // ping/pong/close/frame control — skip
                Err(_) => return None,
            }
        }
        None
    }
}
