//! Construction and one-shot execution of the generic Decision capability.

use orchest_protocol::{Decision, DecisionRequest, DecisionResponse, ErrorCode, ProtocolError};

use crate::{ProviderConfig, Registry};

/// Explicit provider/model selection, with optional transport credentials.
/// No provider or model is selected implicitly by the portable API.
#[derive(Clone)]
pub struct DecisionConfig {
    pub model: String,
    pub api_key: Option<String>,
    /// When set, this environment variable must exist unless api_key is set.
    pub api_key_env: Option<String>,
    /// Full endpoint override. Interpretation belongs to the chosen provider.
    pub api_url: Option<String>,
    pub timeout_ms: Option<u64>,
}

impl DecisionConfig {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            api_key: None,
            api_key_env: None,
            api_url: None,
            timeout_ms: None,
        }
    }
}

impl std::fmt::Debug for DecisionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecisionConfig")
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "[redacted]"))
            .field("api_key_env", &self.api_key_env)
            .field("api_url", &self.api_url.as_ref().map(|_| "[configured]"))
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
}

impl Registry {
    /// Construct a registered implementation, including application-defined
    /// local providers, without depending on any transport or built-in vendor.
    #[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
    pub fn create_decision(
        &self,
        config: &DecisionConfig,
    ) -> Result<Box<dyn Decision>, ProtocolError> {
        let (provider, model) = config
            .model
            .split_once('/')
            .filter(|(p, m)| !p.trim().is_empty() && !m.trim().is_empty())
            .ok_or_else(|| {
                ProtocolError::new(
                    ErrorCode::InvalidRequest,
                    "decision model must be provider/model",
                )
            })?;
        if config.timeout_ms == Some(0) {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "timeout_ms must be positive",
            ));
        }
        let entry = self.decision().id(&config.model).select()?;
        let api_key = match (&config.api_key, &config.api_key_env) {
            (Some(key), _) => Some(key.clone()),
            (None, Some(name)) => Some(std::env::var(name).map_err(|_| {
                ProtocolError::new(
                    ErrorCode::MissingApiKey,
                    "configured decision API key environment variable is not set",
                )
                .with_provider(provider)
                .with_model(model)
            })?),
            (None, None) => None,
        };
        let mut options = serde_json::Map::new();
        if let Some(timeout) = config.timeout_ms {
            options.insert("timeout_ms".into(), timeout.into());
        }
        entry.instantiate(&ProviderConfig {
            provider: provider.into(),
            model: model.into(),
            api_key,
            api_url: config.api_url.clone(),
            max_tokens: None,
            options: options.into(),
        })
    }
}

/// Construct an enabled built-in Decision implementation. For reusable clients,
/// retain the handle and call [`Decision::decide`] for each batch.
#[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
pub fn create_decision(config: &DecisionConfig) -> Result<Box<dyn Decision>, ProtocolError> {
    Registry::with_builtin().create_decision(config)
}

/// Evaluate one batch using an enabled built-in provider. Does not start an
/// agent, choose thresholds, perform actions, or automatically retry.
pub async fn decide(
    config: &DecisionConfig,
    request: DecisionRequest,
) -> Result<DecisionResponse, ProtocolError> {
    request.validate()?;
    create_decision(config)?.decide(request).await
}
