use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::error::{TtsError, TtsErrorCode};
use crate::traits::TtsProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsProviderRuntimeConfig {
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
pub struct NormalizedTtsProviderModel<'a> {
    pub provider: &'a str,
    pub model: &'a str,
}

pub fn normalize_tts_provider_model(
    model: &str,
) -> Result<NormalizedTtsProviderModel<'_>, TtsError> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(TtsError::new(
            TtsErrorCode::InvalidRequest,
            "model string cannot be empty",
        ));
    }

    let Some((provider, model_name)) = trimmed.split_once('/') else {
        return Err(TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!(
                "invalid TTS model string '{trimmed}': expected 'provider/model', \
                 bare model strings are not supported"
            ),
        ));
    };

    if provider.is_empty() || model_name.is_empty() {
        return Err(TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("invalid TTS model string '{trimmed}': expected 'provider/model'"),
        ));
    }

    Ok(NormalizedTtsProviderModel {
        provider,
        model: model_name,
    })
}

pub fn validate_direct_model_selector(
    provider_name: &str,
    model_name: &str,
    request_model: &Option<String>,
) -> Result<(), TtsError> {
    let Some(model) = request_model else {
        return Ok(());
    };
    let normalized = normalize_tts_provider_model(model)?;
    if normalized.provider == provider_name && normalized.model == model_name {
        return Ok(());
    }
    Err(TtsError::new(
        TtsErrorCode::UnknownModel,
        format!(
            "request model '{}' does not match adapter '{}/{}'",
            model, provider_name, model_name
        ),
    ))
}

pub fn resolve_api_key(
    explicit_api_key: Option<&str>,
    api_key_env: Option<&str>,
    default_env: &str,
) -> Result<Option<String>, TtsError> {
    if let Some(key) = explicit_api_key {
        if key.trim().is_empty() {
            return Err(TtsError::new(
                TtsErrorCode::MissingApiKey,
                "explicit api_key is empty",
            ));
        }
        return Ok(Some(key.to_owned()));
    }

    if let Some(env_name) = api_key_env {
        let value = std::env::var(env_name).unwrap_or_default();
        if value.trim().is_empty() {
            return Err(TtsError::new(
                TtsErrorCode::MissingApiKey,
                format!("api key env var '{env_name}' is missing or empty"),
            ));
        }
        return Ok(Some(value));
    }

    Ok(std::env::var(default_env)
        .ok()
        .filter(|value| !value.trim().is_empty()))
}

pub fn create_tts_provider_from_config(
    config: TtsProviderRuntimeConfig,
) -> Result<Arc<dyn TtsProvider>, TtsError> {
    let normalized = normalize_tts_provider_model(&config.model)?;
    match normalized.provider {
        #[cfg(feature = "volcengine")]
        "volcengine" => crate::providers::volcengine::create_provider(config),
        #[cfg(feature = "aliyun")]
        "aliyun" => crate::providers::aliyun::create_provider(config),
        other => Err(TtsError::new(
            TtsErrorCode::UnknownProvider,
            format!("unknown TTS provider '{other}'"),
        )),
    }
}

mod serde_opt_duration_secs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(val: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match val {
            Some(duration) => duration.as_secs().serialize(s),
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
    fn normalize_preserves_provider_and_model() {
        let normalized = normalize_tts_provider_model("aliyun/qwen3-tts-flash-realtime").unwrap();
        assert_eq!(normalized.provider, "aliyun");
        assert_eq!(normalized.model, "qwen3-tts-flash-realtime");
    }

    #[test]
    fn normalize_rejects_bare_model() {
        let err = normalize_tts_provider_model("qwen3-tts-flash-realtime").unwrap_err();
        assert_eq!(err.code, TtsErrorCode::InvalidRequest);
    }

    #[test]
    fn resolve_api_key_prefers_explicit_key() {
        std::env::set_var("ORCHEST_TTS_TEST_EXPLICIT_ENV", "env-key");
        let key = resolve_api_key(
            Some("explicit-key"),
            Some("ORCHEST_TTS_TEST_EXPLICIT_ENV"),
            "ORCHEST_TTS_TEST_DEFAULT_ENV",
        )
        .unwrap();
        assert_eq!(key.as_deref(), Some("explicit-key"));
    }

    #[test]
    fn resolve_api_key_prefers_explicit_env_over_default_env() {
        std::env::set_var("ORCHEST_TTS_TEST_EXPLICIT_ENV_2", "env-key");
        std::env::set_var("ORCHEST_TTS_TEST_DEFAULT_ENV_2", "default-key");
        let key = resolve_api_key(
            None,
            Some("ORCHEST_TTS_TEST_EXPLICIT_ENV_2"),
            "ORCHEST_TTS_TEST_DEFAULT_ENV_2",
        )
        .unwrap();
        assert_eq!(key.as_deref(), Some("env-key"));
    }

    #[test]
    fn resolve_api_key_uses_default_env_only_without_explicit_inputs() {
        std::env::set_var("ORCHEST_TTS_TEST_DEFAULT_ENV_3", "default-key");
        let key = resolve_api_key(None, None, "ORCHEST_TTS_TEST_DEFAULT_ENV_3").unwrap();
        assert_eq!(key.as_deref(), Some("default-key"));
    }

    #[test]
    fn resolve_api_key_missing_explicit_env_does_not_fallback() {
        std::env::remove_var("ORCHEST_TTS_TEST_EXPLICIT_ENV_4");
        std::env::set_var("ORCHEST_TTS_TEST_DEFAULT_ENV_4", "default-key");
        let err = resolve_api_key(
            None,
            Some("ORCHEST_TTS_TEST_EXPLICIT_ENV_4"),
            "ORCHEST_TTS_TEST_DEFAULT_ENV_4",
        )
        .unwrap_err();
        assert_eq!(err.code, TtsErrorCode::MissingApiKey);
    }
}
