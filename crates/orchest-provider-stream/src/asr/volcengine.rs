//! Volcengine openspeech **ASR** wire codec, on the shared [`crate::openspeech`]
//! core (Issue 006). Ported from `agent-runtime-asr-providers`'s
//! `providers/volcengine/protocol.rs` — same frame layout, now over the unified
//! header/constants/gzip and [`ProtocolError`] instead of a provider-local copy.
//!
//! ASR frames the (gzipped JSON) full-client-request and raw audio-only requests
//! with a big-endian `u32` payload-length prefix, and uses the sequence flags to
//! mark the final server response. The event-tagged `FLAG_WITH_EVENT` framing is
//! tts/omni-only and lives there.

use orchest_protocol::{ErrorCode, ProtocolError};
use serde::Deserialize;
use serde_json::Value;

use crate::openspeech::{
    build_header, compress_gzip, decompress_gzip, parse_header, COMP_GZIP, COMP_NONE,
    FLAG_LAST_NO_SEQUENCE, FLAG_NO_SEQUENCE, FLAG_SEQUENCE_NEGATIVE, FLAG_SEQUENCE_POSITIVE,
    MSG_AUDIO_ONLY_REQUEST, MSG_ERROR_RESPONSE, MSG_FULL_CLIENT_REQUEST, MSG_FULL_SERVER_RESPONSE,
    SER_JSON, SER_NONE,
};

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn stream_err(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::ProviderStreamError, message)
}

// ---------------------------------------------------------------------------
// Frame building
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn build_full_client_request(payload_json: &Value) -> Result<Vec<u8>, ProtocolError> {
    let json_bytes = serde_json::to_vec(payload_json)
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidRequest, format!("serialize: {e}")))?;
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

#[derive(Debug, Clone, Deserialize)]
pub struct VolcenginePayload {
    pub result: Option<VolcengineResult>,
    pub audio_info: Option<AudioInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolcengineResult {
    pub text: String,
    pub utterances: Option<Vec<VolcengineUtterance>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolcengineUtterance {
    pub text: String,
    pub definite: bool,
    pub start_time: i32,
    pub end_time: i32,
    pub words: Option<Vec<VolcengineWord>>,
    #[serde(default)]
    pub additions: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolcengineWord {
    pub text: String,
    pub start_time: i32,
    pub end_time: i32,
    #[serde(default)]
    pub blank_duration: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AudioInfo {
    pub duration: Option<u64>,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_response(data: &[u8]) -> Result<VolcengineFrame, ProtocolError> {
    let header = parse_header(data)?;
    match header.msg_type {
        MSG_ERROR_RESPONSE => parse_error_response(data),
        MSG_FULL_SERVER_RESPONSE => parse_server_response(data, header.flags, header.compression),
        other => Err(stream_err(format!("unexpected message type: 0x{other:X}"))),
    }
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_error_response(data: &[u8]) -> Result<VolcengineFrame, ProtocolError> {
    if data.len() < 12 {
        return Err(stream_err("error response too short"));
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

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_server_response(
    data: &[u8],
    flags: u8,
    compression: u8,
) -> Result<VolcengineFrame, ProtocolError> {
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
        return Err(stream_err("response too short for payload size"));
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
        return Err(stream_err(format!(
            "response truncated: expected {payload_end} bytes, got {}",
            data.len()
        )));
    }

    let payload_bytes = &data[payload_start..payload_end];
    let json_bytes = if compression == COMP_GZIP {
        decompress_gzip(payload_bytes)?
    } else {
        payload_bytes.to_vec()
    };

    let payload: VolcenginePayload = serde_json::from_slice(&json_bytes)
        .map_err(|e| stream_err(format!("failed to parse response JSON: {e}")))?;

    Ok(VolcengineFrame::ServerResponse {
        sequence,
        payload,
        is_last,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audio_frame_normal_and_last() {
        let frame = build_audio_frame(&[1, 2, 3, 4], false);
        assert_eq!(frame[1] & 0x0F, FLAG_NO_SEQUENCE);
        let size = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]);
        assert_eq!(size, 4);
        assert_eq!(&frame[8..], &[1, 2, 3, 4]);
        assert_eq!(
            build_audio_frame(&[1, 2], true)[1] & 0x0F,
            FLAG_LAST_NO_SEQUENCE
        );
    }

    #[test]
    fn full_client_request_roundtrip() {
        let payload = json!({
            "request": {"model_name": "bigmodel", "show_utterances": true}
        });
        let frame = build_full_client_request(&payload).unwrap();
        assert_eq!(frame[0], 0x11);
        assert_eq!((frame[1] >> 4) & 0x0F, MSG_FULL_CLIENT_REQUEST);
        assert_eq!((frame[2] >> 4) & 0x0F, SER_JSON);
        assert_eq!(frame[2] & 0x0F, COMP_GZIP);

        let payload_size = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]) as usize;
        let decompressed = decompress_gzip(&frame[8..8 + payload_size]).unwrap();
        let parsed: Value = serde_json::from_slice(&decompressed).unwrap();
        assert_eq!(parsed["request"]["model_name"], "bigmodel");
    }

    fn server_frame(flags: u8, sequence: Option<i32>, body: &Value) -> Vec<u8> {
        let compressed = compress_gzip(&serde_json::to_vec(body).unwrap()).unwrap();
        let mut frame = build_header(MSG_FULL_SERVER_RESPONSE, flags, SER_JSON, COMP_GZIP).to_vec();
        if let Some(seq) = sequence {
            frame.extend_from_slice(&seq.to_be_bytes());
        }
        frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        frame.extend_from_slice(&compressed);
        frame
    }

    #[test]
    fn parse_server_response_with_sequence() {
        let body = json!({
            "result": {
                "text": "测试",
                "utterances": [{
                    "text": "测试", "definite": true, "start_time": 0, "end_time": 1000,
                    "words": [{"text": "测", "start_time": 0, "end_time": 500}]
                }]
            },
            "audio_info": {"duration": 1000}
        });
        match parse_response(&server_frame(FLAG_SEQUENCE_POSITIVE, Some(1), &body)).unwrap() {
            VolcengineFrame::ServerResponse {
                sequence,
                payload,
                is_last,
            } => {
                assert_eq!(sequence, 1);
                assert!(!is_last);
                let result = payload.result.unwrap();
                assert_eq!(result.text, "测试");
                assert!(result.utterances.unwrap()[0].definite);
            }
            _ => panic!("expected ServerResponse"),
        }
    }

    #[test]
    fn parse_last_response_negative_sequence() {
        let body = json!({"result": {"text": "end"}, "audio_info": {"duration": 500}});
        match parse_response(&server_frame(FLAG_SEQUENCE_NEGATIVE, Some(-1), &body)).unwrap() {
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
    fn parse_response_no_sequence() {
        let body = json!({"result": {"text": "ok"}, "audio_info": {}});
        match parse_response(&server_frame(FLAG_NO_SEQUENCE, None, &body)).unwrap() {
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

    #[test]
    fn parse_error_response_frame() {
        let mut frame =
            build_header(MSG_ERROR_RESPONSE, FLAG_NO_SEQUENCE, SER_JSON, COMP_NONE).to_vec();
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
}
