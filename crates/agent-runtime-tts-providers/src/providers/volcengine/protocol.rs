use crate::error::{TtsError, TtsErrorCode};

pub const EVENT_START_CONNECTION: i32 = 1;
pub const EVENT_CONNECTION_STARTED: i32 = 50;
pub const EVENT_CONNECTION_FAILED: i32 = 51;
pub const EVENT_CONNECTION_FINISHED: i32 = 52;
pub const EVENT_START_SESSION: i32 = 100;
pub const EVENT_SESSION_STARTED: i32 = 150;
pub const EVENT_FINISH_SESSION: i32 = 102;
pub const EVENT_CANCEL_SESSION: i32 = 101;
pub const EVENT_SESSION_FAILED: i32 = 153;
pub const EVENT_SESSION_FINISHED: i32 = 152;
pub const EVENT_TASK_REQUEST: i32 = 200;
pub const EVENT_TTS_RESPONSE: i32 = 352;

pub const MSG_FULL_CLIENT_REQUEST: u8 = 0b0001;
const MSG_FULL_SERVER_RESPONSE: u8 = 0b1001;
const MSG_AUDIO_ONLY_RESPONSE: u8 = 0b1011;
const MSG_ERROR_RESPONSE: u8 = 0b1111;
pub const FLAG_WITH_EVENT: u8 = 0b0100;
pub const SER_JSON: u8 = 0b0001;

pub enum VolcengineFrame {
    Meta {
        event: i32,
        _session_id: String,
        payload: serde_json::Value,
    },
    Audio {
        event: i32,
        _session_id: String,
        data: Vec<u8>,
    },
    Error {
        code: u32,
        message: String,
    },
}

/// Connection-level frame (StartConnection / FinishConnection): event number, no session_id.
pub fn build_connect_frame(event: i32, payload: &serde_json::Value) -> Result<Vec<u8>, TtsError> {
    let payload = serde_json::to_vec(payload).map_err(|e| {
        TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("serialize Volcengine payload: {e}"),
        )
    })?;
    let mut out = Vec::with_capacity(12 + payload.len());
    out.extend_from_slice(&[
        0b0001_0001,
        (MSG_FULL_CLIENT_REQUEST << 4) | FLAG_WITH_EVENT,
        SER_JSON << 4,
        0,
    ]);
    out.extend_from_slice(&event.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Session / task frame (StartSession, FinishSession, TaskRequest): event + session_id + payload.
pub fn build_meta_frame(
    event: i32,
    session_id: &str,
    payload: &serde_json::Value,
) -> Result<Vec<u8>, TtsError> {
    let payload = serde_json::to_vec(payload).map_err(|e| {
        TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("serialize Volcengine payload: {e}"),
        )
    })?;
    Ok(build_session_frame(
        MSG_FULL_CLIENT_REQUEST,
        SER_JSON,
        event,
        session_id.as_bytes(),
        &payload,
    ))
}

pub fn parse_frame(data: &[u8]) -> Result<VolcengineFrame, TtsError> {
    if data.len() < 8 {
        return Err(TtsError::new(
            TtsErrorCode::ProviderStreamError,
            "Volcengine frame too short",
        ));
    }
    let msg_type = (data[1] >> 4) & 0x0f;
    match msg_type {
        t if t == MSG_FULL_SERVER_RESPONSE => parse_meta(data),
        t if t == MSG_AUDIO_ONLY_RESPONSE => parse_audio(data),
        t if t == MSG_ERROR_RESPONSE => parse_error(data),
        other => Err(TtsError::new(
            TtsErrorCode::ProviderStreamError,
            format!("unexpected Volcengine frame type {other}"),
        )),
    }
}

fn build_session_frame(
    msg_type: u8,
    serialization: u8,
    event: i32,
    session_id: &[u8],
    payload: &[u8],
) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(4 + 4 + 4 + session_id.len() + 4 + payload.len());
    out.extend_from_slice(&[
        0b0001_0001,
        (msg_type << 4) | FLAG_WITH_EVENT,
        serialization << 4,
        0,
    ]);
    out.extend_from_slice(&event.to_be_bytes());
    out.extend_from_slice(&(session_id.len() as u32).to_be_bytes());
    out.extend_from_slice(session_id);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn parse_meta(data: &[u8]) -> Result<VolcengineFrame, TtsError> {
    let (event, session_id, payload_bytes) = parse_event_session_payload(data)?;
    let payload = serde_json::from_slice(&payload_bytes).map_err(|e| {
        TtsError::new(
            TtsErrorCode::ProviderStreamError,
            format!("parse Volcengine meta JSON: {e}"),
        )
    })?;
    Ok(VolcengineFrame::Meta {
        event,
        _session_id: session_id,
        payload,
    })
}

fn parse_audio(data: &[u8]) -> Result<VolcengineFrame, TtsError> {
    let (event, session_id, payload) = parse_event_session_payload(data)?;
    Ok(VolcengineFrame::Audio {
        event,
        _session_id: session_id,
        data: payload,
    })
}

fn parse_event_session_payload(data: &[u8]) -> Result<(i32, String, Vec<u8>), TtsError> {
    if data.len() < 12 {
        return Err(TtsError::new(
            TtsErrorCode::ProviderStreamError,
            "Volcengine event frame too short",
        ));
    }
    let event = i32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let session_len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let session_start = 12;
    let payload_len_start = session_start + session_len;
    if data.len() < payload_len_start + 4 {
        return Err(TtsError::new(
            TtsErrorCode::ProviderStreamError,
            "Volcengine frame missing payload length",
        ));
    }
    let session_id =
        String::from_utf8_lossy(&data[session_start..payload_len_start]).to_string();
    let payload_len = u32::from_be_bytes([
        data[payload_len_start],
        data[payload_len_start + 1],
        data[payload_len_start + 2],
        data[payload_len_start + 3],
    ]) as usize;
    let payload_start = payload_len_start + 4;
    if data.len() < payload_start + payload_len {
        return Err(TtsError::new(
            TtsErrorCode::ProviderStreamError,
            "Volcengine frame payload truncated",
        ));
    }
    Ok((
        event,
        session_id,
        data[payload_start..payload_start + payload_len].to_vec(),
    ))
}

fn parse_error(data: &[u8]) -> Result<VolcengineFrame, TtsError> {
    if data.len() < 12 {
        return Err(TtsError::new(
            TtsErrorCode::ProviderStreamError,
            "Volcengine error frame too short",
        ));
    }
    let code = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let message = if data.len() >= 12 + len {
        String::from_utf8_lossy(&data[12..12 + len]).to_string()
    } else {
        String::new()
    };
    Ok(VolcengineFrame::Error { code, message })
}
