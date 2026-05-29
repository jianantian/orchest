#![allow(clippy::result_large_err)]

pub mod gateway;
pub mod http;
pub mod image;
pub mod providers;
pub mod storage;
pub mod telemetry;
pub mod types;

use crate::providers::{
    AliyunImageAdapter, AliyunImageConfig, CrazyrouterImageAdapter, CrazyrouterImageConfig,
    OpenRouterImageAdapter, OpenRouterImageConfig, RenderfulImageAdapter, RenderfulImageConfig,
};

pub use gateway::*;
pub use image::*;
pub use storage::*;
pub use types::*;

#[allow(clippy::result_large_err)] // justified: AigcError carries diagnostic context needed for user-facing messages
pub fn create_image_provider_from_config(
    config: AigcProviderRuntimeConfig,
) -> Result<Box<dyn ImageProvider>, AigcError> {
    let provider = config.provider.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return Err(AigcError::new(
            "unknown_provider",
            "provider cannot be empty",
        ));
    }
    if config.model.trim().is_empty() {
        return Err(AigcError::new("invalid_model", "model cannot be empty"));
    }

    let api_key = resolve_api_key(
        &provider,
        config.api_key.as_deref(),
        config.api_key_env.as_deref(),
    )?;
    match provider.as_str() {
        "crazyrouter" => Ok(Box::new(CrazyrouterImageAdapter::from_config(
            CrazyrouterImageConfig {
                model: config.model,
                api_key,
                api_url: config.api_url,
                timeout: config.timeout,
            },
        )?)),
        "aliyun" | "dashscope" => Ok(Box::new(AliyunImageAdapter::from_config(
            AliyunImageConfig {
                model: config.model,
                api_key,
                region: config.region,
                api_url: config.api_url,
                timeout: config.timeout,
            },
        )?)),
        "openrouter" => Ok(Box::new(OpenRouterImageAdapter::from_config(
            OpenRouterImageConfig {
                model: config.model,
                api_key,
                api_url: config.api_url,
                timeout: config.timeout,
                app_title: config
                    .provider_options
                    .get("app_title")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                site_url: config
                    .provider_options
                    .get("site_url")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            },
        )?)),
        "renderful" => Ok(Box::new(RenderfulImageAdapter::from_config(
            RenderfulImageConfig {
                model: config.model,
                api_key,
                api_url: config.api_url,
                timeout: config.timeout,
                webhook: config
                    .provider_options
                    .get("webhook")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            },
        )?)),
        _ => Err(AigcError::new(
            "unknown_provider",
            format!("unknown provider '{provider}'"),
        )),
    }
}

#[allow(clippy::result_large_err)] // justified: same AigcError used throughout for consistency
fn resolve_api_key(
    provider: &str,
    explicit: Option<&str>,
    api_key_env: Option<&str>,
) -> Result<String, AigcError> {
    if let Some(value) = explicit {
        return non_empty_api_key(value);
    }
    if let Some(env_name) = api_key_env {
        if env_name.trim().is_empty() {
            return Err(AigcError::new(
                "invalid_api_key_env",
                "api_key_env cannot be empty",
            ));
        }
        return std::env::var(env_name)
            .map_err(|_| {
                AigcError::new(
                    "missing_api_key",
                    format!("API key env var '{env_name}' is not set"),
                )
            })
            .and_then(|value| non_empty_api_key(&value));
    }
    let env_name = match provider {
        "crazyrouter" => "CRAZYROUTER_API_KEY",
        "aliyun" | "dashscope" => "DASHSCOPE_API_KEY",
        "openrouter" => "OPENROUTER_API_KEY",
        "renderful" => "RENDERFUL_API_KEY",
        _ => {
            return Err(AigcError::new(
                "unknown_provider",
                format!("unknown provider '{provider}'"),
            ))
        }
    };
    std::env::var(env_name)
        .map_err(|_| {
            AigcError::new(
                "missing_api_key",
                format!("{env_name} not set and no api_key provided"),
            )
        })
        .and_then(|value| non_empty_api_key(&value))
}

fn non_empty_api_key(value: &str) -> Result<String, AigcError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(AigcError::new("invalid_api_key", "API key cannot be empty"))
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_rejects_unknown_provider() {
        let err = match create_image_provider_from_config(AigcProviderRuntimeConfig {
            provider: "unknown".into(),
            model: "model".into(),
            api_key: Some("key".into()),
            ..Default::default()
        }) {
            Ok(_) => panic!("unknown provider should fail"),
            Err(err) => err,
        };
        assert_eq!(err.code, "unknown_provider");
    }

    #[test]
    fn factory_rejects_empty_model() {
        let err = match create_image_provider_from_config(AigcProviderRuntimeConfig {
            provider: "openrouter".into(),
            model: " ".into(),
            api_key: Some("key".into()),
            ..Default::default()
        }) {
            Ok(_) => panic!("empty model should fail"),
            Err(err) => err,
        };
        assert_eq!(err.code, "invalid_model");
    }

    #[test]
    fn api_key_env_does_not_fallback_to_default() {
        let missing = "ORCHEST_AIGC_MISSING_KEY_FOR_TEST";
        std::env::remove_var(missing);
        std::env::set_var("OPENROUTER_API_KEY", "default-key");
        let err = match create_image_provider_from_config(AigcProviderRuntimeConfig {
            provider: "openrouter".into(),
            model: "google/gemini".into(),
            api_key_env: Some(missing.into()),
            ..Default::default()
        }) {
            Ok(_) => panic!("missing explicit api_key_env should fail"),
            Err(err) => err,
        };
        assert_eq!(err.code, "missing_api_key");
        std::env::remove_var("OPENROUTER_API_KEY");
    }

    #[test]
    fn factory_routes_to_openrouter() {
        let provider = create_image_provider_from_config(AigcProviderRuntimeConfig {
            provider: "openrouter".into(),
            model: "google/gemini".into(),
            api_key: Some("key".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(provider.provider_name(), "openrouter");
    }
}
