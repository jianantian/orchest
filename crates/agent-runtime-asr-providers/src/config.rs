use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::error::{AsrError, AsrErrorCode};
use crate::traits::AsrProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[allow(clippy::result_large_err)] // justified: ConfigError is large by design (rich diagnostics); callers box it immediately
pub fn create_asr_provider_from_config(
    config: AsrProviderRuntimeConfig,
) -> Result<Arc<dyn AsrProvider>, AsrError> {
    let normalized = normalize_asr_provider_model(&config.model)?;

    match normalized.provider {
        #[cfg(feature = "volcengine")]
        "volcengine" => {
            let _ = &config;
            Err(AsrError::new(
                AsrErrorCode::UnsupportedOperation,
                "volcengine adapter creation from config is not yet implemented",
            ))
        }
        #[cfg(feature = "aliyun")]
        "aliyun" => {
            let _ = &config;
            Err(AsrError::new(
                AsrErrorCode::UnsupportedOperation,
                "aliyun adapter creation from config is not yet implemented",
            ))
        }
        other => Err(AsrError::new(
            AsrErrorCode::UnknownProvider,
            format!("unknown ASR provider '{other}'"),
        )),
    }
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
        let n = normalize_asr_provider_model("volcengine/bigmodel_async").unwrap();
        assert_eq!(n.provider, "volcengine");
        assert_eq!(n.model, "bigmodel_async");
    }

    #[test]
    fn normalize_rejects_bare_model() {
        let err = normalize_asr_provider_model("bigmodel_async").unwrap_err();
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
        let err = normalize_asr_provider_model("/bigmodel_async").unwrap_err();
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }

    #[test]
    fn normalize_rejects_empty_model() {
        let err = normalize_asr_provider_model("volcengine/").unwrap_err();
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }

    #[test]
    fn normalize_preserves_nested_model_path() {
        let n = normalize_asr_provider_model("aliyun/fun-asr-realtime").unwrap();
        assert_eq!(n.provider, "aliyun");
        assert_eq!(n.model, "fun-asr-realtime");
    }

    #[test]
    fn normalize_trims_whitespace() {
        let n = normalize_asr_provider_model("  volcengine/bigmodel_async  ").unwrap();
        assert_eq!(n.provider, "volcengine");
        assert_eq!(n.model, "bigmodel_async");
    }

    #[test]
    fn factory_returns_unsupported_for_volcengine_config() {
        let err = match create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "volcengine/bigmodel_async".into(),
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: None,
            region: None,
            timeout: None,
            provider_options: Value::Null,
        }) {
            Ok(_) => panic!("factory should return an error instead of panicking"),
            Err(err) => err,
        };

        assert_eq!(err.code, AsrErrorCode::UnsupportedOperation);
    }

    #[test]
    fn factory_returns_unsupported_for_aliyun_config() {
        let err = match create_asr_provider_from_config(AsrProviderRuntimeConfig {
            model: "aliyun/fun-asr-realtime".into(),
            api_key: Some("key".into()),
            api_key_env: None,
            api_url: None,
            region: None,
            timeout: None,
            provider_options: Value::Null,
        }) {
            Ok(_) => panic!("factory should return an error instead of panicking"),
            Err(err) => err,
        };

        assert_eq!(err.code, AsrErrorCode::UnsupportedOperation);
    }
}
