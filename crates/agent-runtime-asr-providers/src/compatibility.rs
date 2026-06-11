use serde_json::Value;

use crate::error::{AsrError, AsrErrorCode};
use crate::types::{
    AsrModelCapabilities, AudioTimelineMode, CompatibilityPolicy, EndpointingMode,
    EndpointingOptions, OptionAdjustment, StreamingAudioFormat, StreamingTranscribeRequest,
    TranscribeOptions,
};

#[derive(Debug)]
pub struct CompatibilityResult {
    pub adjustments: Vec<OptionAdjustment>,
}

pub fn validate_streaming_request(
    request: &StreamingTranscribeRequest,
    caps: &AsrModelCapabilities,
) -> Result<CompatibilityResult, AsrError> {
    match &request.compatibility {
        CompatibilityPolicy::Strict => validate_strict(request, caps),
        CompatibilityPolicy::Coerce => validate_coerce(request, caps),
    }
}

fn validate_strict(
    request: &StreamingTranscribeRequest,
    caps: &AsrModelCapabilities,
) -> Result<CompatibilityResult, AsrError> {
    validate_streaming_format_strict(&request.format, caps)?;
    validate_timeline_strict(&request.timeline, caps)?;
    validate_options_strict(&request.options, caps)?;
    validate_provider_options_strict(&request.provider_options, caps)?;
    Ok(CompatibilityResult {
        adjustments: vec![],
    })
}

fn validate_streaming_format_strict(
    format: &StreamingAudioFormat,
    caps: &AsrModelCapabilities,
) -> Result<(), AsrError> {
    match format {
        StreamingAudioFormat::Pcm16 {
            sample_rate_hz,
            channels,
        } => {
            let supported = caps.streaming_inputs.iter().any(|cap| {
                cap.format == crate::types::AudioFormat::Pcm
                    && match &cap.sample_rates_hz {
                        crate::types::SampleRateSupport::Any => true,
                        crate::types::SampleRateSupport::Exact(rates) => {
                            rates.contains(sample_rate_hz)
                        }
                        crate::types::SampleRateSupport::Range { min, max } => {
                            sample_rate_hz >= min && sample_rate_hz <= max
                        }
                    }
                    && match &cap.channels {
                        crate::types::ChannelSupport::Any => true,
                        crate::types::ChannelSupport::Exact(ch) => ch.contains(channels),
                    }
            });
            if !supported {
                return Err(AsrError::new(
                    AsrErrorCode::UnsupportedAudioFormat,
                    format!(
                        "unsupported streaming audio format: PCM16 {}Hz {}ch",
                        sample_rate_hz, channels
                    ),
                ));
            }
        }
        StreamingAudioFormat::Encoded { format: fmt } => {
            let supported = caps.streaming_inputs.iter().any(|cap| cap.format == *fmt);
            if !supported {
                return Err(AsrError::new(
                    AsrErrorCode::UnsupportedAudioFormat,
                    format!("unsupported streaming audio format: {:?}", fmt),
                ));
            }
        }
    }
    Ok(())
}

fn validate_timeline_strict(
    timeline: &AudioTimelineMode,
    caps: &AsrModelCapabilities,
) -> Result<(), AsrError> {
    if !caps.audio_timeline_modes.contains(timeline) {
        return Err(AsrError::new(
            AsrErrorCode::UnsupportedOption,
            format!("unsupported audio timeline mode: {:?}", timeline),
        ));
    }
    Ok(())
}

fn validate_options_strict(
    options: &TranscribeOptions,
    caps: &AsrModelCapabilities,
) -> Result<(), AsrError> {
    if let Some(ref ep) = options.endpointing {
        validate_endpointing_strict(ep, caps)?;
    }

    if options.word_timestamps && !caps.word_timestamps {
        return Err(AsrError::new(
            AsrErrorCode::UnsupportedOption,
            "word_timestamps not supported by this provider/model",
        ));
    }

    if options.speaker_diarization && !caps.speaker_diarization {
        return Err(AsrError::new(
            AsrErrorCode::UnsupportedOption,
            "speaker_diarization not supported by this provider/model",
        ));
    }

    if !options.hot_words.is_empty() && !caps.hot_words {
        return Err(AsrError::new(
            AsrErrorCode::UnsupportedOption,
            "hot_words not supported by this provider/model",
        ));
    }

    if options.context_prompt.is_some() && !caps.context_prompt {
        return Err(AsrError::new(
            AsrErrorCode::UnsupportedOption,
            "context_prompt not supported by this provider/model",
        ));
    }

    if options.code_switching && !caps.code_switching {
        return Err(AsrError::new(
            AsrErrorCode::UnsupportedOption,
            "code_switching not supported by this provider/model",
        ));
    }

    Ok(())
}

fn validate_endpointing_strict(
    ep: &EndpointingOptions,
    caps: &AsrModelCapabilities,
) -> Result<(), AsrError> {
    if ep.mode != EndpointingMode::ProviderDefault && !caps.endpointing_modes.contains(&ep.mode) {
        return Err(AsrError::new(
            AsrErrorCode::UnsupportedOption,
            format!("unsupported endpointing mode: {:?}", ep.mode),
        ));
    }

    if ep.silence_timeout.is_some() {
        let supports_silence = caps
            .endpointing_modes
            .contains(&EndpointingMode::AcousticSilence);
        if !supports_silence || ep.mode != EndpointingMode::AcousticSilence {
            return Err(AsrError::new(
                AsrErrorCode::UnsupportedOption,
                "silence_timeout only supported with AcousticSilence endpointing mode",
            ));
        }
    }

    Ok(())
}

fn validate_provider_options_strict(
    provider_options: &Value,
    caps: &AsrModelCapabilities,
) -> Result<(), AsrError> {
    if let Value::Object(map) = provider_options {
        for key in map.keys() {
            if !caps.provider_option_keys.contains(key) {
                return Err(AsrError::new(
                    AsrErrorCode::UnsupportedOption,
                    format!(
                        "unsupported provider_option key '{}' for this provider/model",
                        key
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_coerce(
    request: &StreamingTranscribeRequest,
    caps: &AsrModelCapabilities,
) -> Result<CompatibilityResult, AsrError> {
    let mut adjustments = Vec::new();

    // Format — coerce can't transcode, still reject unsupported format
    if let Err(_e) = validate_streaming_format_strict(&request.format, caps) {
        return Err(AsrError::new(
            AsrErrorCode::UnsupportedAudioFormat,
            "unsupported streaming audio format (coerce mode cannot transcode)",
        ));
    }

    // Timeline — coerce can't fix this either
    if let Err(_e) = validate_timeline_strict(&request.timeline, caps) {
        return Err(AsrError::new(
            AsrErrorCode::UnsupportedOption,
            "unsupported audio timeline mode (coerce mode cannot adjust)",
        ));
    }

    coerce_options(&request.options, caps, &mut adjustments);
    coerce_provider_options(&request.provider_options, caps, &mut adjustments);

    Ok(CompatibilityResult { adjustments })
}

fn coerce_options(
    options: &TranscribeOptions,
    caps: &AsrModelCapabilities,
    adjustments: &mut Vec<OptionAdjustment>,
) {
    if options.word_timestamps && !caps.word_timestamps {
        adjustments.push(OptionAdjustment {
            option: "word_timestamps".into(),
            requested: serde_json::json!(true),
            applied: serde_json::json!(false),
            reason: "not supported by provider/model".into(),
        });
    }

    if options.speaker_diarization && !caps.speaker_diarization {
        adjustments.push(OptionAdjustment {
            option: "speaker_diarization".into(),
            requested: serde_json::json!(true),
            applied: serde_json::json!(false),
            reason: "not supported by provider/model".into(),
        });
    }

    if !options.hot_words.is_empty() && !caps.hot_words {
        adjustments.push(OptionAdjustment {
            option: "hot_words".into(),
            requested: serde_json::json!(options.hot_words),
            applied: serde_json::json!([]),
            reason: "not supported by provider/model".into(),
        });
    }

    if options.context_prompt.is_some() && !caps.context_prompt {
        adjustments.push(OptionAdjustment {
            option: "context_prompt".into(),
            requested: serde_json::json!(options.context_prompt),
            applied: serde_json::json!(null),
            reason: "not supported by provider/model".into(),
        });
    }

    if options.code_switching && !caps.code_switching {
        adjustments.push(OptionAdjustment {
            option: "code_switching".into(),
            requested: serde_json::json!(true),
            applied: serde_json::json!(false),
            reason: "not supported by provider/model".into(),
        });
    }

    if let Some(ref ep) = options.endpointing {
        if ep.mode != EndpointingMode::ProviderDefault && !caps.endpointing_modes.contains(&ep.mode)
        {
            adjustments.push(OptionAdjustment {
                option: "endpointing.mode".into(),
                requested: serde_json::json!(format!("{:?}", ep.mode)),
                applied: serde_json::json!("ProviderDefault"),
                reason: "unsupported mode, falling back to provider default".into(),
            });
        }

        if ep.silence_timeout.is_some() {
            let supports_silence = caps
                .endpointing_modes
                .contains(&EndpointingMode::AcousticSilence);
            if !supports_silence || ep.mode != EndpointingMode::AcousticSilence {
                adjustments.push(OptionAdjustment {
                    option: "endpointing.silence_timeout".into(),
                    requested: serde_json::json!(ep.silence_timeout.map(|d| d.as_millis())),
                    applied: serde_json::json!(null),
                    reason: "silence_timeout only supported with AcousticSilence mode".into(),
                });
            }
        }
    }
}

fn coerce_provider_options(
    provider_options: &Value,
    caps: &AsrModelCapabilities,
    adjustments: &mut Vec<OptionAdjustment>,
) {
    if let Value::Object(map) = provider_options {
        for (key, value) in map {
            if !caps.provider_option_keys.contains(key) {
                adjustments.push(OptionAdjustment {
                    option: format!("provider_options.{}", key),
                    requested: value.clone(),
                    applied: serde_json::json!(null),
                    reason: "unsupported provider option key, ignored".into(),
                });
            }
        }
    }
}
