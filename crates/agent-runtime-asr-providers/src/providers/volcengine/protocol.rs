use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::error::{AsrError, AsrErrorCode};

// ---------------------------------------------------------------------------
// Protocol constants
// ---------------------------------------------------------------------------

pub const PROTOCOL_VERSION: u8 = 0b0001;
pub const HEADER_SIZE_4B: u8 = 0b0001;

pub const MSG_FULL_CLIENT_REQUEST: u8 = 0b0001;
pub const MSG_AUDIO_ONLY_REQUEST: u8 = 0b0010;
pub const MSG_FULL_SERVER_RESPONSE: u8 = 0b1001;
pub const MSG_ERROR_RESPONSE: u8 = 0b1111;

pub const FLAG_NO_SEQUENCE: u8 = 0b0000;
pub const FLAG_SEQUENCE_POSITIVE: u8 = 0b0001;
pub const FLAG_LAST_NO_SEQUENCE: u8 = 0b0010;
pub const FLAG_SEQUENCE_NEGATIVE: u8 = 0b0011;

pub const SER_NONE: u8 = 0b0000;
pub const SER_JSON: u8 = 0b0001;

pub const COMP_NONE: u8 = 0b0000;
pub const COMP_GZIP: u8 = 0b0001;

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

pub fn build_header(msg_type: u8, flags: u8, serialization: u8, compression: u8) -> [u8; 4] {
    [
        (PROTOCOL_VERSION << 4) | HEADER_SIZE_4B,
        (msg_type << 4) | flags,
        (serialization << 4) | compression,
        0x00,
    ]
}

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

pub fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, AsrError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(|e| {
        AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!("gzip compression failed: {e}"),
        )
    })?;
    encoder.finish().map_err(|e| {
        AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!("gzip finish failed: {e}"),
        )
    })
}

pub fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, AsrError> {
    let mut decoder = GzDecoder::new(data);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf).map_err(|e| {
        AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!("gzip decompression failed: {e}"),
        )
    })?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Frame building
// ---------------------------------------------------------------------------

pub fn build_full_client_request(payload_json: &serde_json::Value) -> Result<Vec<u8>, AsrError> {
    let json_bytes = serde_json::to_vec(payload_json).map_err(|e| {
        AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!("JSON serialization failed: {e}"),
        )
    })?;
    let compressed = compress_gzip(&json_bytes)?;

    let header = build_header(
        MSG_FULL_CLIENT_REQUEST,
        FLAG_NO_SEQUENCE,
        SER_JSON,
        COMP_GZIP,
    );
    let payload_size = (compressed.len() as u32).to_be_bytes();

    let mut frame = Vec::with_capacity(4 + 4 + compressed.len());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&payload_size);
    frame.extend_from_slice(&compressed);
    Ok(frame)
}

pub fn build_audio_frame(data: &[u8], is_last: bool) -> Vec<u8> {
    let flags = if is_last {
        FLAG_LAST_NO_SEQUENCE
    } else {
        FLAG_NO_SEQUENCE
    };
    let header = build_header(MSG_AUDIO_ONLY_REQUEST, flags, SER_NONE, COMP_NONE);
    let payload_size = (data.len() as u32).to_be_bytes();

    let mut frame = Vec::with_capacity(4 + 4 + data.len());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&payload_size);
    frame.extend_from_slice(data);
    frame
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum VolcengineFrame {
    ServerResponse {
        sequence: i32,
        payload: VolcenginePayload,
        is_last: bool,
    },
    ErrorResponse {
        code: u32,
        message: String,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VolcenginePayload {
    pub result: Option<VolcengineResult>,
    pub audio_info: Option<AudioInfo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VolcengineResult {
    pub text: String,
    pub utterances: Option<Vec<VolcengineUtterance>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VolcengineUtterance {
    pub text: String,
    pub definite: bool,
    pub start_time: i32,
    pub end_time: i32,
    pub words: Option<Vec<VolcengineWord>>,
    #[serde(default)]
    pub additions: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VolcengineWord {
    pub text: String,
    pub start_time: i32,
    pub end_time: i32,
    #[serde(default)]
    pub blank_duration: Option<i32>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AudioInfo {
    pub duration: Option<u64>,
}

pub fn parse_response(data: &[u8]) -> Result<VolcengineFrame, AsrError> {
    if data.len() < 4 {
        return Err(AsrError::new(
            AsrErrorCode::ProviderStreamError,
            "response too short for header",
        ));
    }

    let msg_type = (data[1] >> 4) & 0x0F;
    let flags = data[1] & 0x0F;
    let compression = data[2] & 0x0F;

    match msg_type {
        MSG_ERROR_RESPONSE => parse_error_response(data),
        MSG_FULL_SERVER_RESPONSE => parse_server_response(data, flags, compression),
        other => Err(AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!("unexpected message type: 0x{other:X}"),
        )),
    }
}

fn parse_error_response(data: &[u8]) -> Result<VolcengineFrame, AsrError> {
    if data.len() < 12 {
        return Err(AsrError::new(
            AsrErrorCode::ProviderStreamError,
            "error response too short",
        ));
    }
    let code = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let msg_size = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let message = if data.len() >= 12 + msg_size {
        String::from_utf8_lossy(&data[12..12 + msg_size]).to_string()
    } else {
        String::new()
    };
    Ok(VolcengineFrame::ErrorResponse { code, message })
}

fn parse_server_response(
    data: &[u8],
    flags: u8,
    compression: u8,
) -> Result<VolcengineFrame, AsrError> {
    let has_sequence = flags == FLAG_SEQUENCE_POSITIVE || flags == FLAG_SEQUENCE_NEGATIVE;
    let is_last = flags == FLAG_LAST_NO_SEQUENCE || flags == FLAG_SEQUENCE_NEGATIVE;

    let seq_offset = if has_sequence { 4 } else { 0 };
    let sequence = if has_sequence && data.len() >= 8 {
        i32::from_be_bytes([data[4], data[5], data[6], data[7]])
    } else {
        0
    };

    let size_offset = 4 + seq_offset;
    if data.len() < size_offset + 4 {
        return Err(AsrError::new(
            AsrErrorCode::ProviderStreamError,
            "response too short for payload size",
        ));
    }

    let payload_size = u32::from_be_bytes([
        data[size_offset],
        data[size_offset + 1],
        data[size_offset + 2],
        data[size_offset + 3],
    ]) as usize;

    let payload_start = size_offset + 4;
    let payload_end = payload_start + payload_size;
    if data.len() < payload_end {
        return Err(AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!(
                "response truncated: expected {} bytes, got {}",
                payload_end,
                data.len()
            ),
        ));
    }

    let payload_bytes = &data[payload_start..payload_end];
    let json_bytes = if compression == COMP_GZIP {
        decompress_gzip(payload_bytes)?
    } else {
        payload_bytes.to_vec()
    };

    let payload: VolcenginePayload = serde_json::from_slice(&json_bytes).map_err(|e| {
        AsrError::new(
            AsrErrorCode::ProviderStreamError,
            format!("failed to parse response JSON: {e}"),
        )
    })?;

    Ok(VolcengineFrame::ServerResponse {
        sequence,
        payload,
        is_last,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn header_byte_layout() {
        let h = build_header(
            MSG_FULL_CLIENT_REQUEST,
            FLAG_NO_SEQUENCE,
            SER_JSON,
            COMP_GZIP,
        );
        assert_eq!(h[0], 0x11);
        assert_eq!(h[1], 0x10);
        assert_eq!(h[2], 0x11);
        assert_eq!(h[3], 0x00);
    }

    #[test]
    fn audio_frame_normal() {
        let frame = build_audio_frame(&[1, 2, 3, 4], false);
        assert_eq!(frame[1] & 0x0F, FLAG_NO_SEQUENCE);
        let size = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]);
        assert_eq!(size, 4);
        assert_eq!(&frame[8..], &[1, 2, 3, 4]);
    }

    #[test]
    fn audio_frame_last() {
        let frame = build_audio_frame(&[1, 2], true);
        assert_eq!(frame[1] & 0x0F, FLAG_LAST_NO_SEQUENCE);
    }

    #[test]
    fn full_client_request_roundtrip() {
        let payload = json!({
            "user": {"uid": "test"},
            "audio": {"format": "pcm", "rate": 16000, "bits": 16, "channel": 1},
            "request": {"model_name": "bigmodel", "show_utterances": true}
        });

        let frame = build_full_client_request(&payload).unwrap();
        assert_eq!(frame[0], 0x11);
        assert_eq!((frame[1] >> 4) & 0x0F, MSG_FULL_CLIENT_REQUEST);
        assert_eq!((frame[2] >> 4) & 0x0F, SER_JSON);
        assert_eq!(frame[2] & 0x0F, COMP_GZIP);

        let payload_size = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]) as usize;
        let compressed = &frame[8..8 + payload_size];
        let decompressed = decompress_gzip(compressed).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&decompressed).unwrap();
        assert_eq!(parsed["request"]["model_name"], "bigmodel");
        assert_eq!(parsed["request"]["show_utterances"], true);
    }

    #[test]
    fn parse_server_response_with_sequence() {
        let response_json = json!({
            "result": {
                "text": "测试",
                "utterances": [{
                    "text": "测试",
                    "definite": true,
                    "start_time": 0,
                    "end_time": 1000,
                    "words": [{"text": "测", "start_time": 0, "end_time": 500, "blank_duration": 0},
                              {"text": "试", "start_time": 500, "end_time": 1000, "blank_duration": 0}]
                }]
            },
            "audio_info": {"duration": 1000}
        });

        let json_bytes = serde_json::to_vec(&response_json).unwrap();
        let compressed = compress_gzip(&json_bytes).unwrap();

        let mut frame = Vec::new();
        let header = build_header(
            MSG_FULL_SERVER_RESPONSE,
            FLAG_SEQUENCE_POSITIVE,
            SER_JSON,
            COMP_GZIP,
        );
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&1i32.to_be_bytes()); // sequence = 1
        frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        frame.extend_from_slice(&compressed);

        match parse_response(&frame).unwrap() {
            VolcengineFrame::ServerResponse {
                sequence,
                payload,
                is_last,
            } => {
                assert_eq!(sequence, 1);
                assert!(!is_last);
                let result = payload.result.unwrap();
                assert_eq!(result.text, "测试");
                let utterances = result.utterances.unwrap();
                assert_eq!(utterances.len(), 1);
                assert!(utterances[0].definite);
                assert_eq!(utterances[0].words.as_ref().unwrap().len(), 2);
            }
            _ => panic!("expected ServerResponse"),
        }
    }

    #[test]
    fn parse_last_response() {
        let response_json = json!({"result": {"text": "end"}, "audio_info": {"duration": 500}});
        let json_bytes = serde_json::to_vec(&response_json).unwrap();
        let compressed = compress_gzip(&json_bytes).unwrap();

        let mut frame = Vec::new();
        let header = build_header(
            MSG_FULL_SERVER_RESPONSE,
            FLAG_SEQUENCE_NEGATIVE,
            SER_JSON,
            COMP_GZIP,
        );
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&(-1i32).to_be_bytes());
        frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        frame.extend_from_slice(&compressed);

        match parse_response(&frame).unwrap() {
            VolcengineFrame::ServerResponse {
                is_last, sequence, ..
            } => {
                assert!(is_last);
                assert_eq!(sequence, -1);
            }
            _ => panic!("expected ServerResponse"),
        }
    }

    #[test]
    fn parse_error_response_frame() {
        let mut frame = Vec::new();
        let header = build_header(MSG_ERROR_RESPONSE, FLAG_NO_SEQUENCE, SER_JSON, COMP_NONE);
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&45000001u32.to_be_bytes());
        let msg = b"invalid parameters";
        frame.extend_from_slice(&(msg.len() as u32).to_be_bytes());
        frame.extend_from_slice(msg);

        match parse_response(&frame).unwrap() {
            VolcengineFrame::ErrorResponse { code, message } => {
                assert_eq!(code, 45000001);
                assert_eq!(message, "invalid parameters");
            }
            _ => panic!("expected ErrorResponse"),
        }
    }

    #[test]
    fn gzip_roundtrip() {
        let original = b"hello world test data for compression";
        let compressed = compress_gzip(original).unwrap();
        let decompressed = decompress_gzip(&compressed).unwrap();
        assert_eq!(&decompressed, original);
    }

    #[test]
    fn parse_response_no_sequence() {
        let response_json = json!({"result": {"text": "ok"}, "audio_info": {}});
        let json_bytes = serde_json::to_vec(&response_json).unwrap();
        let compressed = compress_gzip(&json_bytes).unwrap();

        let mut frame = Vec::new();
        let header = build_header(
            MSG_FULL_SERVER_RESPONSE,
            FLAG_NO_SEQUENCE,
            SER_JSON,
            COMP_GZIP,
        );
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        frame.extend_from_slice(&compressed);

        match parse_response(&frame).unwrap() {
            VolcengineFrame::ServerResponse {
                sequence,
                payload,
                is_last,
            } => {
                assert_eq!(sequence, 0);
                assert!(!is_last);
                assert_eq!(payload.result.unwrap().text, "ok");
            }
            _ => panic!("expected ServerResponse"),
        }
    }
}
