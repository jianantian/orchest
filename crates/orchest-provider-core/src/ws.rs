//! Bidirectional websocket scaffold + binary-frame codec (behind the `ws`
//! feature — pulls `tokio-tungstenite`).
//!
//! The openspeech dialect (Volcengine asr/tts/omni) frames messages as
//! big-endian length-prefixed binary payloads after a small header. This module
//! provides the reusable framing primitive; the dialect-specific header bytes
//! and event mapping land in `orchest-provider-stream` (Issue 006).

/// A length-prefixed binary frame codec: `[u32 big-endian payload length][payload]`.
/// The shared wire-framing primitive under the openspeech binary protocol.
pub struct BinaryFrameCodec;

impl BinaryFrameCodec {
    /// Encode `payload` as a big-endian `u32` length prefix followed by the bytes.
    pub fn encode(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + payload.len());
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    /// Try to decode one frame from the front of `buf`. On success returns the
    /// payload and the number of bytes consumed; returns `None` if `buf` does not
    /// yet hold a complete frame.
    pub fn decode(buf: &[u8]) -> Option<(Vec<u8>, usize)> {
        if buf.len() < 4 {
            return None;
        }
        let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        let end = 4 + len;
        if buf.len() < end {
            return None;
        }
        Some((buf[4..end].to_vec(), end))
    }
}

/// Re-export the tungstenite types the dialect crates build their sessions on,
/// so impl crates depend on one websocket stack through core.
pub use tokio_tungstenite::{connect_async, tungstenite};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrips() {
        let payload = b"openspeech-frame";
        let encoded = BinaryFrameCodec::encode(payload);
        let (decoded, consumed) = BinaryFrameCodec::decode(&encoded).unwrap();
        assert_eq!(decoded, payload);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn partial_frame_returns_none() {
        let encoded = BinaryFrameCodec::encode(b"data");
        assert!(BinaryFrameCodec::decode(&encoded[..3]).is_none()); // header incomplete
        assert!(BinaryFrameCodec::decode(&encoded[..5]).is_none()); // payload incomplete
    }
}
