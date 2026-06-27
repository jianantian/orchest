//! The Volcengine **openspeech binary protocol** — defined here **once**, shared
//! by the asr, tts, and omni dialects (Issue 006 acceptance #1).
//!
//! Every openspeech frame opens with the same 4-byte header:
//!
//! ```text
//! byte 0: (protocol_version << 4) | header_size     // 0x11
//! byte 1: (message_type    << 4) | flags
//! byte 2: (serialization   << 4) | compression
//! byte 3: reserved (0x00)
//! ```
//!
//! What comes *after* the header is dialect-specific and stays in the dialect
//! modules: asr length-prefixes a (optionally gzipped) JSON/audio payload and
//! uses the sequence flags; tts/omni set `FLAG_WITH_EVENT` and prepend an `event`
//! (i32) (+ optional `session_id`). This module owns only the genuinely shared
//! bits — the header codec, the field constants, and gzip — so the three
//! dialects no longer each carry their own copy.

use orchest_protocol::{ErrorCode, ProtocolError};

// ---------------------------------------------------------------------------
// Field constants (the union across asr/tts/omni — all three agree on values)
// ---------------------------------------------------------------------------

pub const PROTOCOL_VERSION: u8 = 0b0001;
pub const HEADER_SIZE_4B: u8 = 0b0001;

pub const MSG_FULL_CLIENT_REQUEST: u8 = 0b0001;
pub const MSG_AUDIO_ONLY_REQUEST: u8 = 0b0010;
pub const MSG_FULL_SERVER_RESPONSE: u8 = 0b1001;
pub const MSG_AUDIO_ONLY_RESPONSE: u8 = 0b1011;
pub const MSG_ERROR_RESPONSE: u8 = 0b1111;

pub const FLAG_NO_SEQUENCE: u8 = 0b0000;
pub const FLAG_SEQUENCE_POSITIVE: u8 = 0b0001;
pub const FLAG_LAST_NO_SEQUENCE: u8 = 0b0010;
pub const FLAG_SEQUENCE_NEGATIVE: u8 = 0b0011;
/// tts/omni event-framing flag (asr never sets it).
pub const FLAG_WITH_EVENT: u8 = 0b0100;

pub const SER_NONE: u8 = 0b0000;
pub const SER_JSON: u8 = 0b0001;

pub const COMP_NONE: u8 = 0b0000;
pub const COMP_GZIP: u8 = 0b0001;

// ---------------------------------------------------------------------------
// Header codec
// ---------------------------------------------------------------------------

/// Build the 4-byte openspeech header.
pub fn build_header(msg_type: u8, flags: u8, serialization: u8, compression: u8) -> [u8; 4] {
    [
        (PROTOCOL_VERSION << 4) | HEADER_SIZE_4B,
        (msg_type << 4) | flags,
        (serialization << 4) | compression,
        0x00,
    ]
}

/// The decoded fields of a frame header (the high/low nibbles of bytes 1–2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub msg_type: u8,
    pub flags: u8,
    pub serialization: u8,
    pub compression: u8,
}

/// Decode the header from the front of a received frame. Returns the fields the
/// dialects branch on; the dialect is responsible for the payload that follows.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_header(data: &[u8]) -> Result<Header, ProtocolError> {
    if data.len() < 4 {
        return Err(ProtocolError::new(
            ErrorCode::ProviderStreamError,
            "openspeech frame too short for header",
        ));
    }
    Ok(Header {
        msg_type: (data[1] >> 4) & 0x0F,
        flags: data[1] & 0x0F,
        serialization: (data[2] >> 4) & 0x0F,
        compression: data[2] & 0x0F,
    })
}

// ---------------------------------------------------------------------------
// Compression (asr gzips its JSON; tts/omni use COMP_NONE but share the codec)
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidRequest, format!("gzip write: {e}")))?;
    encoder
        .finish()
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidRequest, format!("gzip finish: {e}")))
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf).map_err(|e| {
        ProtocolError::new(
            ErrorCode::ProviderStreamError,
            format!("gzip decompress: {e}"),
        )
    })?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_byte_layout_matches_legacy() {
        // Same bytes the asr/tts dialects asserted before unification.
        let h = build_header(
            MSG_FULL_CLIENT_REQUEST,
            FLAG_NO_SEQUENCE,
            SER_JSON,
            COMP_GZIP,
        );
        assert_eq!(h, [0x11, 0x10, 0x11, 0x00]);
    }

    #[test]
    fn event_framing_header_matches_tts() {
        // tts/omni: MSG_FULL_CLIENT_REQUEST | FLAG_WITH_EVENT, SER_JSON, no comp.
        let h = build_header(
            MSG_FULL_CLIENT_REQUEST,
            FLAG_WITH_EVENT,
            SER_JSON,
            COMP_NONE,
        );
        assert_eq!(h, [0x11, 0x14, 0x10, 0x00]);
    }

    #[test]
    fn parse_header_roundtrips_build() {
        let h = build_header(
            MSG_FULL_SERVER_RESPONSE,
            FLAG_SEQUENCE_NEGATIVE,
            SER_JSON,
            COMP_GZIP,
        );
        let parsed = parse_header(&h).unwrap();
        assert_eq!(
            parsed,
            Header {
                msg_type: MSG_FULL_SERVER_RESPONSE,
                flags: FLAG_SEQUENCE_NEGATIVE,
                serialization: SER_JSON,
                compression: COMP_GZIP,
            }
        );
    }

    #[test]
    fn parse_header_rejects_short_frame() {
        assert!(parse_header(&[0x11, 0x10, 0x11]).is_err());
    }

    #[test]
    fn gzip_roundtrip() {
        let original = b"hello world test data for compression";
        let compressed = compress_gzip(original).unwrap();
        let decompressed = decompress_gzip(&compressed).unwrap();
        assert_eq!(&decompressed, original);
    }
}
