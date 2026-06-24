//! Minimax `VoiceManager` impl — Voice Clone / Voice Design / Delete Voice.
//!
//! Three endpoints:
//! - `POST /v1/voice_clone`      (`voice_clone/clone.md`)
//! - `POST /v1/voice_design`     (`voice_design.md`)
//! - `POST /v1/delete_voice`     (`delete_voice.md`)
//!
//! Auth: `Authorization: Bearer ${api_key}`. All three accept JSON.

use async_trait::async_trait;
use bytes::Bytes;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{TtsError, TtsErrorCode};
use crate::traits::VoiceManager;
use crate::types::{
    AudioData, CloneVoiceRequest, CloneVoiceResponse, DesignVoiceRequest, DesignVoiceResponse,
    VoiceCatalogSource, VoiceInfo, VoiceKind,
};

use super::protocol::{decode_hex_audio, map_base_resp, BaseResp};
use super::MinimaxTtsAdapter;

pub(crate) fn build_clone_request_body(req: &CloneVoiceRequest) -> Value {
    let mut body = json!({
        "file_id": req.file_id,
        "voice_id": req.voice_id,
        "need_noise_reduction": req.need_noise_reduction,
        "need_volume_normalization": req.need_volume_normalization,
        "aigc_watermark": req.aigc_watermark,
    });
    if let Some(prompt) = &req.clone_prompt {
        body["clone_prompt"] = json!({
            "prompt_audio": prompt.prompt_audio,
            "prompt_text": prompt.prompt_text,
        });
    }
    // Trial fields: per spec §5b both `trial_text` and `trial_model` must be
    // present for the upstream to render demo audio.
    if let (Some(text), Some(model)) = (&req.trial_text, &req.trial_model) {
        body["text"] = json!(text);
        body["model"] = json!(model);
    }
    if let Some(lb) = &req.language_boost {
        body["language_boost"] = json!(lb);
    }
    body
}

pub(crate) fn build_design_request_body(req: &DesignVoiceRequest) -> Value {
    let mut body = json!({
        "prompt": req.prompt,
        "preview_text": req.preview_text,
    });
    if let Some(id) = &req.voice_id {
        body["voice_id"] = json!(id);
    }
    body
}

pub(crate) fn build_delete_request_body(
    voice_id: &str,
    kind: VoiceKind,
) -> Result<Value, TtsError> {
    let voice_type = match kind {
        VoiceKind::Cloned => "voice_cloning",
        VoiceKind::Designed => "voice_generation",
        VoiceKind::System | VoiceKind::Custom => {
            return Err(TtsError::new(
                TtsErrorCode::UnsupportedOperation,
                format!("Minimax delete_voice does not support VoiceKind::{kind:?}"),
            ));
        }
    };
    Ok(json!({ "voice_type": voice_type, "voice_id": voice_id }))
}

#[derive(Debug, Deserialize)]
struct CloneRespRaw {
    #[serde(default)]
    demo_audio: Option<String>,
    #[serde(default)]
    input_sensitive: Option<Value>,
    base_resp: BaseResp,
}

pub(crate) fn parse_clone_response(
    body: &str,
    voice_id: &str,
    adapter_model: &str,
) -> Result<CloneVoiceResponse, TtsError> {
    let raw: CloneRespRaw = serde_json::from_str(body).map_err(|err| {
        TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("Minimax /v1/voice_clone response parse failed: {err}; body: {body}"),
        )
    })?;
    if let Some(err) = map_base_resp(&raw.base_resp) {
        return Err(err);
    }
    let input_sensitive = extract_input_sensitive(raw.input_sensitive.as_ref());
    let demo_audio = raw.demo_audio.filter(|s| !s.is_empty());
    let voice = VoiceInfo {
        provider: "minimax".into(),
        model: adapter_model.into(),
        id: voice_id.into(),
        display_name: voice_id.into(),
        kind: VoiceKind::Cloned,
        gender: None,
        languages: Vec::new(),
        is_custom: true,
        supports_instruction: false,
        supports_emotion: true,
        supports_style: false,
        supports_cloning: true,
        supports_design: false,
        source: VoiceCatalogSource::CallerConfig,
        provider_metadata: Value::Null,
    };
    Ok(CloneVoiceResponse {
        voice,
        demo_audio,
        input_sensitive,
    })
}

fn extract_input_sensitive(value: Option<&Value>) -> u8 {
    let Some(v) = value else {
        return 0;
    };
    if let Some(t) = v.get("type").and_then(|x| x.as_u64()) {
        return t.min(255) as u8;
    }
    if let Some(t) = v.as_u64() {
        return t.min(255) as u8;
    }
    0
}

#[derive(Debug, Deserialize)]
struct DesignRespRaw {
    #[serde(default)]
    voice_id: Option<String>,
    #[serde(default)]
    trial_audio: Option<String>,
    base_resp: BaseResp,
}

pub(crate) fn parse_design_response(
    body: &str,
    adapter_model: &str,
) -> Result<DesignVoiceResponse, TtsError> {
    let raw: DesignRespRaw = serde_json::from_str(body).map_err(|err| {
        TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("Minimax /v1/voice_design response parse failed: {err}; body: {body}"),
        )
    })?;
    if let Some(err) = map_base_resp(&raw.base_resp) {
        return Err(err);
    }
    let voice_id = raw.voice_id.ok_or_else(|| {
        TtsError::new(
            TtsErrorCode::ProviderTaskFailed,
            "Minimax /v1/voice_design response missing voice_id",
        )
    })?;
    let audio_bytes: Bytes = decode_hex_audio(raw.trial_audio.as_deref())?;
    let voice = VoiceInfo {
        provider: "minimax".into(),
        model: adapter_model.into(),
        id: voice_id.clone(),
        display_name: voice_id,
        kind: VoiceKind::Designed,
        gender: None,
        languages: Vec::new(),
        is_custom: true,
        supports_instruction: false,
        supports_emotion: true,
        supports_style: false,
        supports_cloning: false,
        supports_design: true,
        source: VoiceCatalogSource::CallerConfig,
        provider_metadata: Value::Null,
    };
    Ok(DesignVoiceResponse {
        voice,
        trial_audio: AudioData::Bytes(audio_bytes),
    })
}

#[async_trait]
impl VoiceManager for MinimaxTtsAdapter {
    async fn clone_voice(&self, req: CloneVoiceRequest) -> Result<CloneVoiceResponse, TtsError> {
        let body = build_clone_request_body(&req);
        let resp = self.post_json("/v1/voice_clone", &body).await?;
        parse_clone_response(&resp, &req.voice_id, self.model_name_raw())
    }

    async fn design_voice(&self, req: DesignVoiceRequest) -> Result<DesignVoiceResponse, TtsError> {
        let body = build_design_request_body(&req);
        let resp = self.post_json("/v1/voice_design", &body).await?;
        parse_design_response(&resp, self.model_name_raw())
    }

    async fn delete_voice(&self, voice_id: &str, kind: VoiceKind) -> Result<(), TtsError> {
        let body = build_delete_request_body(voice_id, kind)?;
        let resp = self.post_json("/v1/delete_voice", &body).await?;
        #[derive(Deserialize)]
        struct DeleteResp {
            base_resp: BaseResp,
        }
        let parsed: DeleteResp = serde_json::from_str(&resp).map_err(|err| {
            TtsError::new(
                TtsErrorCode::InvalidRequest,
                format!("Minimax /v1/delete_voice response parse failed: {err}; body: {resp}"),
            )
        })?;
        if let Some(err) = map_base_resp(&parsed.base_resp) {
            return Err(err);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ClonePrompt;

    fn empty_clone_req(voice_id: &str) -> CloneVoiceRequest {
        CloneVoiceRequest {
            file_id: 1,
            voice_id: voice_id.into(),
            clone_prompt: None,
            trial_text: None,
            trial_model: None,
            language_boost: None,
            need_noise_reduction: false,
            need_volume_normalization: false,
            aigc_watermark: false,
        }
    }

    #[test]
    fn clone_request_body_omits_trial_without_both_text_and_model() {
        let mut req = empty_clone_req("V1");
        req.trial_text = Some("hello".into());
        // No trial_model — spec §5b requires both, so neither must appear.
        let body = build_clone_request_body(&req);
        assert!(body.get("text").is_none());
        assert!(body.get("model").is_none());
    }

    #[test]
    fn clone_request_body_with_trial_and_prompt() {
        let req = CloneVoiceRequest {
            file_id: 12345,
            voice_id: "MyVoice001".into(),
            clone_prompt: Some(ClonePrompt {
                prompt_audio: 42,
                prompt_text: "hi".into(),
            }),
            trial_text: Some("hello".into()),
            trial_model: Some("speech-2.8-hd".into()),
            language_boost: Some("English".into()),
            need_noise_reduction: true,
            need_volume_normalization: true,
            aigc_watermark: false,
        };
        let body = build_clone_request_body(&req);
        assert_eq!(body["file_id"], 12345);
        assert_eq!(body["voice_id"], "MyVoice001");
        assert_eq!(body["clone_prompt"]["prompt_audio"], 42);
        assert_eq!(body["clone_prompt"]["prompt_text"], "hi");
        assert_eq!(body["text"], "hello");
        assert_eq!(body["model"], "speech-2.8-hd");
        assert_eq!(body["language_boost"], "English");
        assert_eq!(body["need_noise_reduction"], true);
    }

    #[test]
    fn parse_clone_response_with_demo_audio() {
        let raw = r#"{
            "demo_audio": "https://example/demo.mp3",
            "input_sensitive": {"type": 0},
            "base_resp": {"status_code": 0, "status_msg": "success"}
        }"#;
        let resp = parse_clone_response(raw, "MyVoice001", "speech-2.8-hd").unwrap();
        assert_eq!(resp.voice.id, "MyVoice001");
        assert_eq!(resp.voice.kind, VoiceKind::Cloned);
        assert_eq!(resp.demo_audio.as_deref(), Some("https://example/demo.mp3"));
        assert_eq!(resp.input_sensitive, 0);
    }

    #[test]
    fn parse_clone_response_without_demo_audio() {
        let raw = r#"{
            "demo_audio": "",
            "input_sensitive": {"type": 3},
            "base_resp": {"status_code": 0, "status_msg": ""}
        }"#;
        let resp = parse_clone_response(raw, "V", "speech-2.8-hd").unwrap();
        assert!(resp.demo_audio.is_none());
        assert_eq!(resp.input_sensitive, 3);
    }

    #[test]
    fn parse_clone_response_propagates_base_resp_error() {
        let raw = r#"{"base_resp":{"status_code":1004,"status_msg":"auth"}}"#;
        let err = parse_clone_response(raw, "V", "speech-2.8-hd").unwrap_err();
        assert_eq!(err.code, TtsErrorCode::InvalidApiKey);
    }

    #[test]
    fn design_request_body_omits_optional_voice_id() {
        let req = DesignVoiceRequest {
            prompt: "悬疑播音员".into(),
            preview_text: "夜深了".into(),
            voice_id: None,
        };
        let body = build_design_request_body(&req);
        assert_eq!(body["prompt"], "悬疑播音员");
        assert_eq!(body["preview_text"], "夜深了");
        assert!(body.get("voice_id").is_none());
    }

    #[test]
    fn parse_design_response_decodes_trial_audio() {
        // hex "0102" → bytes [0x01, 0x02]
        let raw = r#"{
            "voice_id": "ttv-voice-1",
            "trial_audio": "0102",
            "base_resp": {"status_code": 0, "status_msg": "success"}
        }"#;
        let resp = parse_design_response(raw, "speech-2.8-hd").unwrap();
        assert_eq!(resp.voice.id, "ttv-voice-1");
        assert_eq!(resp.voice.kind, VoiceKind::Designed);
        match resp.trial_audio {
            AudioData::Bytes(b) => assert_eq!(b.as_ref(), &[0x01, 0x02]),
            AudioData::Url { .. } => panic!("expected Bytes"),
        }
    }

    #[test]
    fn parse_design_response_requires_voice_id() {
        let raw = r#"{"trial_audio": "01","base_resp": {"status_code": 0, "status_msg": ""}}"#;
        let err = parse_design_response(raw, "speech-2.8-hd").unwrap_err();
        assert_eq!(err.code, TtsErrorCode::ProviderTaskFailed);
        assert!(err.message.contains("voice_id"));
    }

    #[test]
    fn delete_request_body_voice_kind_mapping() {
        // Cloned → voice_cloning
        let body = build_delete_request_body("v1", VoiceKind::Cloned).unwrap();
        assert_eq!(body["voice_type"], "voice_cloning");
        assert_eq!(body["voice_id"], "v1");
        // Designed → voice_generation
        let body = build_delete_request_body("v2", VoiceKind::Designed).unwrap();
        assert_eq!(body["voice_type"], "voice_generation");
    }

    #[test]
    fn delete_request_body_rejects_system_voice() {
        let err = build_delete_request_body("v", VoiceKind::System).unwrap_err();
        assert_eq!(err.code, TtsErrorCode::UnsupportedOperation);
    }

    #[test]
    fn delete_request_body_rejects_custom_voice() {
        let err = build_delete_request_body("v", VoiceKind::Custom).unwrap_err();
        assert_eq!(err.code, TtsErrorCode::UnsupportedOperation);
    }
}
