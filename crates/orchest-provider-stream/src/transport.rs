//! Shared WebSocket byte transport for the openspeech dialects (Issue 006).
//!
//! The streaming loops (asr/tts/omni) are written against [`ByteDuplex`] — a
//! minimal duplex of length-agnostic binary frames — rather than a concrete
//! socket. In production that is [`WsDuplex`] over a `tokio-tungstenite`
//! connection; in tests it is an in-memory channel pair. Keeping the loops
//! generic over this is what makes the streaming behavior testable without a
//! network, and keeps every dialect on one WS stack (through
//! `orchest-provider-core`) without naming a concrete `tokio-tungstenite` type.

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use orchest_protocol::{ErrorCode, ProtocolError};
use orchest_provider_core::ws::tungstenite;

/// A minimal duplex byte transport a streaming session runs over.
#[async_trait]
pub trait ByteDuplex: Send {
    async fn send(&mut self, frame: Vec<u8>) -> Result<(), ProtocolError>;
    async fn recv(&mut self) -> Option<Vec<u8>>;
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
    async fn send(&mut self, frame: Vec<u8>) -> Result<(), ProtocolError> {
        self.inner
            .send(tungstenite::Message::Binary(frame))
            .await
            .map_err(|e| {
                ProtocolError::new(ErrorCode::ProviderStreamError, format!("ws send: {e}"))
            })
    }

    async fn recv(&mut self) -> Option<Vec<u8>> {
        while let Some(message) = self.inner.next().await {
            match message {
                Ok(tungstenite::Message::Binary(bytes)) => return Some(bytes),
                Ok(_) => continue, // ignore text/ping/pong/close control frames
                Err(_) => return None,
            }
        }
        None
    }
}
