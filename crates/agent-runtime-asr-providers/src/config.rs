use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::error::{AsrError, AsrErrorCode};
use crate::traits::AsrProvider;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AsrProviderRuntimeConfig {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_opt_duration_secs"
    )]
    pub timeout: Option<Duration>,
    #[serde(default)]
    pub provider_options: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedAsrProviderModel<'a> {
    pub provider: &'a str,
    pub model: &'a str,
}

pub fn normalize_asr_provider_model(
    model: &str,
) -> Result<NormalizedAsrProviderModel<'_>, AsrError> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(AsrError::new(
            AsrErrorCode::InvalidRequest,
            "model string cannot be empty",
        ));
    }

    let Some((provider, model_name)) = trimmed.split_once('/') else {
        return Err(AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!(
                "invalid ASR model string '{trimmed}': expected 'provider/model', \
                 bare model strings are not supported"
            ),
        ));
    };

    if provider.is_empty() || model_name.is_empty() {
        return Err(AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!("invalid ASR model string '{model}': expected 'provider/model'"),
        ));
    }

    Ok(NormalizedAsrProviderModel {
        provider,
        model: model_name,
    })
}

#[allow(clippy::result_large_err)] // justified: AsrError carries provider-specific detail needed at the call site; boxing would hide the type
pub fn create_asr_provider_from_config(
    config: AsrProviderRuntimeConfig,
) -> Result<Arc<dyn AsrProvider>, AsrError> {
    let normalized = normalize_asr_provider_model(&config.model)?;

    match normalized.provider {
        #[cfg(feature = "volcengine")]
        "volcengine" => {
            use crate::providers::volcengine::{VolcengineAsrAdapter, VolcengineAsrConfig};

            let api_key = resolve_api_key(&config, "VOLCENGINE_API_KEY")?;

            let base = match normalized.model {
                "bigasr" => "volc.bigasr.sauc.",
                "seedasr" => "volc.seedasr.sauc.",
                other => {
                    return Err(AsrError::new(
                        AsrErrorCode::InvalidRequest,
                        format!(
                            "unknown Volcengine ASR model '{other}'; \
                             supported: bigasr, seedasr"
                        ),
                    ))
                }
            };

            let billing = config
                .provider_options
                .get("billing")
                .and_then(|v| v.as_str())
                .unwrap_or("duration");
            let suffix = match billing {
                "duration" => "duration",
                "concurrent" => "concurrent",
                other => {
                    return Err(AsrError::new(
                        AsrErrorCode::InvalidRequest,
                        format!(
                            "unknown billing mode '{other}'; \
                             expected 'duration' (default) or 'concurrent'"
                        ),
                    ))
                }
            };

            let resource_id = format!("{base}{suffix}");
            let ws_url = config.api_url.unwrap_or_else(|| {
                "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async".to_string()
            });
            let access_key = config
                .provider_options
                .get("access_key")
                .and_then(|v| v.as_str())
                .map(String::from);

            Ok(Arc::new(VolcengineAsrAdapter::new(VolcengineAsrConfig {
                model: normalized.model.to_string(),
                ws_url,
                api_key,
                access_key,
                resource_id,
            })))
        }
        #[cfg(feature = "aliyun")]
        "aliyun" => {
            use crate::providers::aliyun::{AliyunAsrAdapter, AliyunAsrConfig};

            let api_key = resolve_api_key(&config, "DASHSCOPE_API_KEY")?;

            if normalized.model != "fun-asr-realtime" {
                return Err(AsrError::new(
                    AsrErrorCode::InvalidRequest,
                    format!(
                        "unknown Aliyun ASR model '{}'; supported: fun-asr-realtime",
                        normalized.model
                    ),
                ));
            }

            let ws_url = config
                .api_url
                .unwrap_or_else(|| "wss://dashscope.aliyuncs.com/api-ws/v1/inference/".to_string());

            Ok(Arc::new(AliyunAsrAdapter::new(AliyunAsrConfig {
                model: normalized.model.to_string(),
                api_key,
                ws_url,
            })))
        }
        #[cfg(feature = "deepgram")]
        "deepgram" => {
            use crate::providers::deepgram::{DeepgramAsrAdapter, DeepgramAsrConfig};

            let api_key = resolve_api_key(&config, "DEEPGRAM_API_KEY")?;
            let ws_url = config
                .api_url
                .unwrap_or_else(|| "wss://api.deepgram.com/v1/listen".to_string());

            Ok(Arc::new(DeepgramAsrAdapter::new(DeepgramAsrConfig {
                model: normalized.model.to_string(),
                api_key,
                ws_url,
            })))
        }
        #[cfg(feature = "elevenlabs")]
        "elevenlabs" => {
            use crate::providers::elevenlabs::{ElevenLabsAsrAdapter, ElevenLabsAsrConfig};

            let api_key = resolve_api_key(&config, "ELEVENLABS_API_KEY")?;
            if normalized.model != "scribe_v2_realtime" {
                return Err(AsrError::new(
                    AsrErrorCode::InvalidRequest,
                    format!(
                        "unknown ElevenLabs ASR model '{}'; supported realtime model: scribe_v2_realtime; batch model scribe_v2 is catalog-only in this iteration",
                        normalized.model
                    ),
                ));
            }
            let ws_url = config
                .api_url
                .unwrap_or_else(|| "wss://api.elevenlabs.io/v1/speech-to-text/stream".to_string());

            Ok(Arc::new(ElevenLabsAsrAdapter::new(ElevenLabsAsrConfig {
                model: normalized.model.to_string(),
                api_key,
                ws_url,
            })))
        }
        #[cfg(feature = "soniox")]
        "soniox" => {
            use crate::providers::soniox::{SonioxAsrAdapter, SonioxAsrConfig};

            let api_key = resolve_api_key(&config, "SONIOX_API_KEY")?;
            match normalized.model {
                "stt-rt-v5" | "stt-rt-v4" => {}
                other => {
                    return Err(AsrError::new(
                        AsrErrorCode::InvalidRequest,
                        format!(
                            "unknown Soniox ASR model '{other}'; supported: stt-rt-v5, stt-rt-v4"
                        ),
                    ))
                }
            }
            let ws_url = config
                .api_url
                .unwrap_or_else(|| "wss://stt-rt.soniox.com/transcribe-websocket".to_string());

            Ok(Arc::new(SonioxAsrAdapter::new(SonioxAsrConfig {
                model: normalized.model.to_string(),
                api_key,
                ws_url,
            })))
        }
        other => Err(AsrError::new(
            AsrErrorCode::UnknownProvider,
            format!("unknown ASR provider '{other}'"),
        )),
    }
}

fn resolve_api_key(
    config: &AsrProviderRuntimeConfig,
    default_env: &str,
) -> Result<String, AsrError> {
    if let Some(key) = &config.api_key {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return Err(AsrError::new(
                AsrErrorCode::InvalidRequest,
                "api_key cannot be empty",
            ));
        }
        return Ok(trimmed.to_string());
    }
    let env_name = config.api_key_env.as_deref().unwrap_or(default_env);
    if env_name.trim().is_empty() {
        return Err(AsrError::new(
            AsrErrorCode::InvalidRequest,
            "api_key_env cannot be empty",
        ));
    }
    let value = std::env::var(env_name).map_err(|_| {
        AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!("API key env var '{env_name}' is not set"),
        )
    })?;
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!("API key env var '{env_name}' is set but empty"),
        ));
    }
    Ok(trimmed)
}

mod serde_opt_duration_secs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(val: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match val {
            Some(d) => d.as_secs().serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let secs: Option<u64> = Option::deserialize(d)?;
        Ok(secs.map(Duration::from_secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_valid_provider_model() {
        let n = normalize_asr_provider_model("volcengine/bigasr").unwrap();
        assert_eq!(n.provider, "volcengine");
        assert_eq!(n.model, "bigasr");
    }

    #[test]
    fn normalize_rejects_bare_model() {
        let err = normalize_asr_provider_model("bigasr").unwrap_err();
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
        assert!(err.message.contains("bare model strings"));
    }

    #[test]
    fn normalize_rejects_empty() {
        let err = normalize_asr_provider_model("").unwrap_err();
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }

    #[test]
    fn normalize_rejects_empty_provider() {
        let err = normalize_asr_provider_model("/bigasr").unwrap_err();
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }

    #[test]
    fn normalize_rejects_empty_model() {
        let err = normalize_asr_provider_model("volcengine/").unwrap_err();
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }

    #[test]
    fn normalize_preserves_nested_model_path() {
        let n = normalize_asr_provider_model("deepgram/nova-3/general").unwrap();
        assert_eq!(n.provider, "deepgram");
        assert_eq!(n.model, "nova-3/general");
    }

    #[test]
    fn normalize_deepgram_model() {
        let n = normalize_asr_provider_model("deepgram/nova-3").unwrap();
        assert_eq!(n.provider, "deepgram");
        assert_eq!(n.model, "nova-3");
    }

    #[test]
    fn normalize_elevenlabs_model() {
        let n = normalize_asr_provider_model("elevenlabs/scribe_v2_realtime").unwrap();
        assert_eq!(n.provider, "elevenlabs");
        assert_eq!(n.model, "scribe_v2_realtime");
    }

    #[test]
    fn normalize_soniox_model() {
        let n = normalize_asr_provider_model("soniox/stt-rt-v5").unwrap();
        assert_eq!(n.provider, "soniox");
        assert_eq!(n.model, "stt-rt-v5");
    }

    #[test]
    fn normalize_preserves_aliyun_model_path() {
        let n = normalize_asr_provider_model("aliyun/fun-asr-realtime").unwrap();
        assert_eq!(n.provider, "aliyun");
        assert_eq!(n.model, "fun-asr-realtime");
    }

    #[test]
    fn normalize_trims_whitespace() {
        let n = normalize_asr_provider_model("  volcengine/bigasr  ").unwrap();
        assert_eq!(n.provider, "volcengine");
        assert_eq!(n.model, "bigasr");
    }

    #[test]
    fn factory_creates_volcengine_bigasr_with_duration_billing() {
        let provider = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "volcengine/bigasr".into(),
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: None,
            region: None,
            timeout: None,
            provider_options: Value::Null,
        })
        .expect("factory should succeed");
        assert_eq!(provider.provider_name(), "volcengine");
        assert_eq!(provider.model_name(), "bigasr");
    }

    #[test]
    fn factory_creates_volcengine_seedasr_with_concurrent_billing() {
        let provider = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "volcengine/seedasr".into(),
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: None,
            region: None,
            timeout: None,
            provider_options: serde_json::json!({"billing": "concurrent"}),
        })
        .expect("factory should succeed");
        assert_eq!(provider.provider_name(), "volcengine");
        assert_eq!(provider.model_name(), "seedasr");
    }

    #[test]
    fn factory_rejects_unknown_volcengine_model() {
        let err = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "volcengine/unknown-model".into(),
            api_key: Some("key".into()),
            ..Default::default()
        })
        .err()
        .expect("factory should return an error");
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }

    #[test]
    fn factory_rejects_unknown_billing_mode() {
        let err = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "volcengine/bigasr".into(),
            api_key: Some("key".into()),
            provider_options: serde_json::json!({"billing": "invalid"}),
            ..Default::default()
        })
        .err()
        .expect("factory should return an error");
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }

    #[test]
    fn factory_creates_aliyun_fun_asr() {
        let provider = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "aliyun/fun-asr-realtime".into(),
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: None,
            region: None,
            timeout: None,
            provider_options: Value::Null,
        })
        .expect("factory should succeed");
        assert_eq!(provider.provider_name(), "aliyun");
        assert_eq!(provider.model_name(), "fun-asr-realtime");
    }

    #[test]
    fn factory_rejects_unknown_aliyun_model() {
        let err = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "aliyun/qwen-asr".into(),
            api_key: Some("key".into()),
            ..Default::default()
        })
        .err()
        .expect("factory should return an error");
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }

    #[cfg(feature = "deepgram")]
    #[test]
    fn factory_creates_deepgram_nova_3() {
        let provider = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "deepgram/nova-3".into(),
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: None,
            region: None,
            timeout: None,
            provider_options: Value::Null,
        })
        .expect("factory should succeed");
        assert_eq!(provider.provider_name(), "deepgram");
        assert_eq!(provider.model_name(), "nova-3");
    }

    #[cfg(feature = "elevenlabs")]
    #[test]
    fn factory_creates_elevenlabs_scribe_v2_realtime() {
        let provider = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "elevenlabs/scribe_v2_realtime".into(),
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: None,
            region: None,
            timeout: None,
            provider_options: Value::Null,
        })
        .expect("factory should succeed");
        assert_eq!(provider.provider_name(), "elevenlabs");
        assert_eq!(provider.model_name(), "scribe_v2_realtime");
    }

    #[cfg(feature = "elevenlabs")]
    #[test]
    fn factory_rejects_elevenlabs_batch_model_for_realtime_adapter() {
        let err = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "elevenlabs/scribe_v2".into(),
            api_key: Some("key".into()),
            ..Default::default()
        })
        .err()
        .expect("factory should return an error");
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
        assert!(err.message.contains("catalog-only"));
    }

    #[cfg(feature = "soniox")]
    #[test]
    fn factory_creates_soniox_stt_rt_v5() {
        let provider = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "soniox/stt-rt-v5".into(),
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: None,
            region: None,
            timeout: None,
            provider_options: Value::Null,
        })
        .expect("factory should succeed");
        assert_eq!(provider.provider_name(), "soniox");
        assert_eq!(provider.model_name(), "stt-rt-v5");
    }

    #[cfg(feature = "soniox")]
    #[test]
    fn factory_rejects_unknown_soniox_model() {
        let err = create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "soniox/batch".into(),
            api_key: Some("key".into()),
            ..Default::default()
        })
        .err()
        .expect("factory should return an error");
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }
}
