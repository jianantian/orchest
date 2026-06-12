use crate::error::{TtsError, TtsErrorCode};
use crate::types::{Language, ListVoicesRequest, VoiceInfo, VoiceKind, VoiceSelection};

pub fn filter_voices(mut voices: Vec<VoiceInfo>, request: &ListVoicesRequest) -> Vec<VoiceInfo> {
    voices.retain(|voice| {
        if !request.include_custom && voice.is_custom {
            return false;
        }
        if let Some(ref language) = request.language {
            if !voice
                .languages
                .iter()
                .any(|candidate| candidate == language)
            {
                return false;
            }
        }
        if let Some(ref kind) = request.kind {
            if &voice.kind != kind {
                return false;
            }
        }
        true
    });
    voices.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then(a.model.cmp(&b.model))
            .then(a.kind_label().cmp(b.kind_label()))
            .then(a.id.cmp(&b.id))
    });
    voices
}

pub fn resolve_voice_for_request(
    voices: &[VoiceInfo],
    selection: &VoiceSelection,
) -> Result<Option<VoiceInfo>, TtsError> {
    let Some(voice) = voices.iter().find(|voice| voice.id == selection.id) else {
        return Ok(None);
    };

    if let Some(ref requested_kind) = selection.kind {
        if &voice.kind != requested_kind {
            return Err(TtsError::new(
                TtsErrorCode::UnsupportedOption,
                format!(
                    "voice '{}' is {:?}, not requested {:?}",
                    voice.id, voice.kind, requested_kind
                ),
            ));
        }
    }

    if let Some(ref requested_language) = selection.language {
        ensure_voice_supports_language(voice, requested_language)?;
    }

    Ok(Some(voice.clone()))
}

fn ensure_voice_supports_language(voice: &VoiceInfo, language: &Language) -> Result<(), TtsError> {
    if voice
        .languages
        .iter()
        .any(|candidate| candidate == language)
    {
        return Ok(());
    }
    Err(TtsError::new(
        TtsErrorCode::UnsupportedLanguage,
        format!(
            "voice '{}' does not support language '{}'",
            voice.id, language.0
        ),
    ))
}

trait VoiceSortKind {
    fn kind_label(&self) -> &'static str;
}

impl VoiceSortKind for VoiceInfo {
    fn kind_label(&self) -> &'static str {
        match self.kind {
            VoiceKind::System => "0-system",
            VoiceKind::Cloned => "1-cloned",
            VoiceKind::Designed => "2-designed",
            VoiceKind::Custom => "3-custom",
        }
    }
}
