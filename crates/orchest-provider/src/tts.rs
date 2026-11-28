//! Resolve a TTS selection against the provider registry.

use orchest_protocol::{
    AudioFormat, ErrorCode, EventStream, ProtocolError, SynthesizeRequest, SynthesizeResult, Tts,
    TtsDuplexHandle, TtsDuplexRequest,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{ProviderConfig, Registry};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsOptions {
    provider: String,
    model: Option<String>,
    voice: String,
    format: AudioFormat,
    api_key: Option<String>,
    api_key_env: Option<String>,
    api_url: Option<String>,
    #[serde(default)]
    provider_options: Value,
    #[serde(default)]
    options: Value,
}

impl TtsOptions {
    /// Decode the common wire options and leave provider-specific validation
    /// to the selected adapter.
    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context
    pub fn parse(value: Value) -> Result<Self, ProtocolError> {
        serde_json::from_value(value)
            .map_err(|error| ProtocolError::new(ErrorCode::InvalidRequest, error.to_string()))
    }

    #[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context
    fn adapter(&self) -> Result<Box<dyn Tts>, ProtocolError> {
        let registry = Registry::with_builtin();
        let mut query = registry.tts().provider(&self.provider);
        if let Some(model) = &self.model {
            // A bare model id is scoped to the requested provider.
            if model.contains('/') {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidRequest,
                    "model must be a bare id",
                ));
            }
            query = query.id(model);
        }
        let entry = query
            .select()
            .map_err(|error| error.with_provider(&self.provider))?;
        let default_env = match self.provider.as_str() {
            "aliyun" => "DASHSCOPE_API_KEY",
            "minimax" => "MINIMAX_API_KEY",
            "volcengine" => "VOLCENGINE_TTS_API_KEY",
            _ => "",
        };
        let key_env = self.api_key_env.as_deref().unwrap_or(default_env);
        let key = self
            .api_key
            .clone()
            .or_else(|| std::env::var(key_env).ok())
            .filter(|key| !key.is_empty());
        let config = ProviderConfig {
            provider: self.provider.clone(),
            model: entry.descriptor.model.to_string(),
            api_key: key,
            api_url: self.api_url.clone(),
            options: self.provider_options.clone(),
            ..Default::default()
        };
        entry.instantiate(&config).map_err(|error| {
            error
                .with_provider(config.provider)
                .with_model(config.model)
        })
    }

    fn request(self, text: String) -> SynthesizeRequest {
        SynthesizeRequest {
            text,
            voice: Some(self.voice),
            format: self.format,
            options: self.options,
        }
    }

    pub async fn synthesize(self, text: String) -> Result<SynthesizeResult, ProtocolError> {
        self.adapter()?.synthesize(self.request(text)).await
    }

    pub async fn stream_synthesize(self, text: String) -> Result<EventStream, ProtocolError> {
        self.adapter()?.stream_synthesize(self.request(text)).await
    }

    pub async fn start_duplex(self) -> Result<TtsDuplexHandle, ProtocolError> {
        self.adapter()?
            .start_duplex_stream(TtsDuplexRequest {
                voice: self.voice,
                format: self.format,
                options: self.options,
            })
            .await
    }
}
