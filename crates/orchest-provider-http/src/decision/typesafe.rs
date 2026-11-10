use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orchest_protocol::{
    CapabilityDescriptor, Decision, DecisionRequest, DecisionResponse, ErrorCode, ProtocolError,
    UpstreamErrorDetail,
};
use orchest_provider_core::registry::ProviderConfig;
use serde_json::Value;

use super::wire;

/// One deployment of the TypeSafe Decisions wire: the same request / response
/// shape served by OpenRouter Decisions and Vercel AI Gateway's
/// TypeSafe-compatible API.
#[derive(Debug, Clone, Copy)]
pub(super) struct Deployment {
    pub provider: &'static str,
    /// Human-readable name used in error messages.
    pub display_name: &'static str,
    pub endpoint: &'static str,
    pub api_key_env: &'static str,
}

/// OpenRouter Decisions (alpha).
pub(super) const OPENROUTER: Deployment = Deployment {
    provider: "openrouter",
    display_name: "OpenRouter Decisions",
    endpoint: "https://openrouter.ai/api/alpha/decisions",
    api_key_env: "OPENROUTER_API_KEY",
};

/// Vercel AI Gateway's TypeSafe-compatible Decisions API.
pub(super) const VERCEL: Deployment = Deployment {
    provider: "vercel",
    display_name: "Vercel AI Gateway Decisions",
    endpoint: crate::defaults::vercel::DECISION_URL,
    api_key_env: crate::defaults::vercel::API_KEY_ENV,
};

/// The deployment serving `provider`'s decision rows.
pub(super) fn deployment(provider: &str) -> Option<Deployment> {
    [OPENROUTER, VERCEL]
        .into_iter()
        .find(|deployment| deployment.provider == provider)
}

pub(super) struct TypeSafeDecision {
    deployment: Deployment,
    descriptor: CapabilityDescriptor,
    api_key: String,
    endpoint: reqwest::Url,
    timeout: Option<Duration>,
}

impl TypeSafeDecision {
    #[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
    pub fn new(
        deployment: Deployment,
        config: &ProviderConfig,
        descriptor: CapabilityDescriptor,
    ) -> Result<Self, ProtocolError> {
        let error = |code, message: String| {
            ProtocolError::new(code, message)
                .with_provider(deployment.provider)
                .with_model(descriptor.model.to_string())
        };
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var(deployment.api_key_env).ok())
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| {
                error(
                    ErrorCode::MissingApiKey,
                    format!("{} requires an API key", deployment.display_name),
                )
            })?;
        let endpoint = reqwest::Url::parse(
            config.api_url.as_deref().unwrap_or(deployment.endpoint),
        )
        .map_err(|_| {
            error(
                ErrorCode::InvalidRequest,
                "decision api_url must be an absolute HTTP(S) endpoint".into(),
            )
        })?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(error(
                ErrorCode::InvalidRequest,
                "invalid decision endpoint".into(),
            ));
        }
        let timeout = parse_options(&config.options).map_err(|e| {
            e.with_provider(deployment.provider)
                .with_model(descriptor.model.to_string())
        })?;
        if config.max_tokens.is_some() {
            return Err(error(
                ErrorCode::UnsupportedOption,
                "max_tokens is not supported for Decisions".into(),
            ));
        }
        Ok(Self {
            deployment,
            descriptor,
            api_key,
            endpoint,
            timeout,
        })
    }

    fn contextualize(&self, error: ProtocolError) -> ProtocolError {
        error
            .with_provider(self.provider_name())
            .with_model(self.model_name())
    }

    fn transport_error(&self, error: reqwest::Error) -> ProtocolError {
        self.contextualize(ProtocolError::new(
            if error.is_timeout() {
                ErrorCode::Timeout
            } else {
                ErrorCode::ProviderHttpError
            },
            // Strip the URL, which could contain private query parameters.
            format!("decision request failed: {}", error.without_url()),
        ))
    }
}

#[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
fn parse_options(value: &Value) -> Result<Option<Duration>, ProtocolError> {
    if value.is_null() {
        return Ok(None);
    }
    let options = value.as_object().ok_or_else(|| {
        ProtocolError::new(
            ErrorCode::InvalidRequest,
            "decision options must be an object",
        )
    })?;
    if options.keys().any(|key| key != "timeout_ms") {
        return Err(ProtocolError::new(
            ErrorCode::UnsupportedOption,
            "unsupported decision option",
        ));
    }
    options
        .get("timeout_ms")
        .map(|value| {
            value
                .as_u64()
                .filter(|v| *v > 0)
                .map(Duration::from_millis)
                .ok_or_else(|| {
                    ProtocolError::new(
                        ErrorCode::InvalidRequest,
                        "timeout_ms must be a positive integer",
                    )
                })
        })
        .transpose()
}

#[async_trait]
impl Decision for TypeSafeDecision {
    fn provider_name(&self) -> &str {
        self.deployment.provider
    }
    fn model_name(&self) -> &str {
        &self.descriptor.model
    }
    fn descriptor(&self) -> CapabilityDescriptor {
        self.descriptor.clone()
    }

    async fn decide(&self, request: DecisionRequest) -> Result<DecisionResponse, ProtocolError> {
        request.validate().map_err(|e| self.contextualize(e))?;
        if !matches!(
            request.state,
            Value::String(_) | Value::Object(_) | Value::Array(_)
        ) {
            return Err(self.contextualize(ProtocolError::new(
                ErrorCode::InvalidRequest,
                format!(
                    "{} state must be a string, object, or array",
                    self.deployment.display_name
                ),
            )));
        }
        let body =
            wire::Request::new(self.model_name(), &request).map_err(|e| self.contextualize(e))?;
        let mut http = crate::http::shared_client()
            .post(self.endpoint.clone())
            .bearer_auth(&self.api_key)
            .json(&body);
        if let Some(timeout) = self.timeout {
            http = http.timeout(timeout);
        }
        let response = http.send().await.map_err(|e| self.transport_error(e))?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.trim().parse::<u64>().ok());
        let bytes = response.bytes().await.map_err(|e| {
            let mut error = self.transport_error(e).with_status(status.as_u16());
            error.retry_after_secs = retry_after;
            error
        })?;
        if !status.is_success() {
            let mut error = self.contextualize(http_error(
                self.deployment.display_name,
                status.as_u16(),
                &bytes,
            ));
            error.retry_after_secs = retry_after;
            return Err(error);
        }
        let result: DecisionResponse = serde_json::from_slice::<wire::Response>(&bytes)
            .map_err(|_| {
                self.contextualize(ProtocolError::new(
                    ErrorCode::InvalidResponse,
                    format!(
                        "{} response does not match the expected schema",
                        self.deployment.display_name
                    ),
                ))
                .with_status(status.as_u16())
            })?
            .into();
        result
            .validate_for(&request)
            .map_err(|e| self.contextualize(e).with_status(status.as_u16()))?;
        Ok(result)
    }
}

/// Map an error response. OpenRouter nests it as `{"error": {message, code}}`;
/// the TypeSafe shape Vercel serves is flat `{message, error_type}`.
fn http_error(display_name: &str, status: u16, bytes: &[u8]) -> ProtocolError {
    let body = serde_json::from_slice::<Value>(bytes).ok();
    let detail = body
        .as_ref()
        .map(|v| v.get("error").filter(|e| e.is_object()).unwrap_or(v));
    let message = detail
        .and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let code = detail
        .and_then(|v| v.get("code").or_else(|| v.get("error_type")))
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        });
    let mut error = ProtocolError::new(
        if status == 401 {
            ErrorCode::InvalidApiKey
        } else {
            ErrorCode::ProviderHttpError
        },
        message
            .clone()
            .unwrap_or_else(|| format!("{display_name} HTTP {status}")),
    )
    .with_status(status);
    error.upstream = Some(Arc::new(UpstreamErrorDetail {
        code,
        message,
        body,
    }));
    error
}
