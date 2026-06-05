use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::Instrument;

use crate::compatibility::validate_streaming_request;
use crate::config::normalize_asr_provider_model;
use crate::error::{AsrError, AsrErrorCode};
use crate::observability;
use crate::streaming::AsrStream;
use crate::traits::AsrProvider;
use crate::types::{
    Language, NetworkRegion, StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrRoute {
    pub languages: Vec<Language>,
    #[serde(default)]
    pub regions: Vec<NetworkRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_micros_per_minute: Option<u64>,
    pub priority: u8,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrGatewayConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_config_path: Option<PathBuf>,
}

pub struct AsrRouter {
    providers: HashMap<String, Arc<dyn AsrProvider>>,
    routes: Vec<AsrRoute>,
}

impl AsrRouter {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            routes: Vec::new(),
        }
    }

    pub fn register_provider(&mut self, model: String, provider: Arc<dyn AsrProvider>) {
        self.providers.insert(model, provider);
    }

    pub fn set_routes(&mut self, routes: Vec<AsrRoute>) {
        self.routes = routes;
    }

    pub fn providers(&self) -> &HashMap<String, Arc<dyn AsrProvider>> {
        &self.providers
    }

    pub fn routes(&self) -> &[AsrRoute] {
        &self.routes
    }

    pub fn select_for_streaming(
        &self,
        request: &StreamingTranscribeRequest,
    ) -> Result<Arc<dyn AsrProvider>, AsrError> {
        if let Some(ref model) = request.model {
            let normalized = normalize_asr_provider_model(model)?;
            let key = format!("{}/{}", normalized.provider, normalized.model);
            return self.providers.get(&key).cloned().ok_or_else(|| {
                AsrError::new(
                    AsrErrorCode::NoMatchingProvider,
                    format!("no registered provider for model '{}'", key),
                )
            });
        }

        if self.routes.is_empty() {
            return Err(AsrError::new(
                AsrErrorCode::NoMatchingProvider,
                "no model specified and no route config available",
            ));
        }

        let mut candidates: Vec<&AsrRoute> = self
            .routes
            .iter()
            .filter(|route| {
                if let Some(ref lang) = request.options.language {
                    if !route.languages.iter().any(|l| l.0 == lang.0) {
                        return false;
                    }
                }
                self.providers.contains_key(&route.model)
            })
            .collect();

        candidates.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.model.cmp(&b.model)));

        let route = candidates.first().ok_or_else(|| {
            AsrError::new(
                AsrErrorCode::NoMatchingProvider,
                "no route matches the request constraints",
            )
        })?;

        self.providers.get(&route.model).cloned().ok_or_else(|| {
            AsrError::new(
                AsrErrorCode::NoMatchingProvider,
                format!(
                    "route selected '{}' but provider not registered",
                    route.model
                ),
            )
        })
    }

    pub fn select_for_transcribe(
        &self,
        request: &TranscribeRequest,
    ) -> Result<Arc<dyn AsrProvider>, AsrError> {
        if let Some(ref model) = request.model {
            let normalized = normalize_asr_provider_model(model)?;
            let key = format!("{}/{}", normalized.provider, normalized.model);
            return self.providers.get(&key).cloned().ok_or_else(|| {
                AsrError::new(
                    AsrErrorCode::NoMatchingProvider,
                    format!("no registered provider for model '{}'", key),
                )
            });
        }

        if self.routes.is_empty() {
            return Err(AsrError::new(
                AsrErrorCode::NoMatchingProvider,
                "no model specified and no route config available",
            ));
        }

        let mut candidates: Vec<&AsrRoute> = self
            .routes
            .iter()
            .filter(|route| {
                if let Some(ref lang) = request.options.language {
                    if !route.languages.iter().any(|l| l.0 == lang.0) {
                        return false;
                    }
                }
                self.providers.contains_key(&route.model)
            })
            .collect();

        candidates.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.model.cmp(&b.model)));

        let route = candidates.first().ok_or_else(|| {
            AsrError::new(
                AsrErrorCode::NoMatchingProvider,
                "no route matches the request constraints",
            )
        })?;

        self.providers.get(&route.model).cloned().ok_or_else(|| {
            AsrError::new(
                AsrErrorCode::NoMatchingProvider,
                format!(
                    "route selected '{}' but provider not registered",
                    route.model
                ),
            )
        })
    }
}

impl Default for AsrRouter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AsrGateway {
    router: AsrRouter,
    config: AsrGatewayConfig,
}

impl AsrGateway {
    pub fn new(router: AsrRouter, config: AsrGatewayConfig) -> Self {
        Self { router, config }
    }

    pub fn router(&self) -> &AsrRouter {
        &self.router
    }

    pub fn config(&self) -> &AsrGatewayConfig {
        &self.config
    }

    fn validate_provider_options_require_model(
        provider_options: &serde_json::Value,
        model: &Option<String>,
    ) -> Result<(), AsrError> {
        if !provider_options.is_null()
            && provider_options != &serde_json::Value::Object(Default::default())
            && model.is_none()
        {
            return Err(AsrError::new(
                AsrErrorCode::InvalidRequest,
                "provider_options requires an explicit model selector",
            ));
        }
        Ok(())
    }

    fn ensure_trace_id(options: &mut crate::types::TranscribeOptions) -> String {
        if options.trace_id.is_none() {
            options.trace_id = Some(uuid::Uuid::new_v4().to_string());
        }
        options.trace_id.clone().unwrap()
    }

    pub async fn transcribe(
        &self,
        mut request: TranscribeRequest,
    ) -> Result<TranscribeResult, AsrError> {
        Self::validate_provider_options_require_model(&request.provider_options, &request.model)?;
        let trace_id = Self::ensure_trace_id(&mut request.options);
        let provider = {
            let _span = observability::router_select_span(&trace_id).entered();
            self.router.select_for_transcribe(&request)?
        };
        let model = provider.model_name().to_string();
        let span = observability::gateway_transcribe_span(&trace_id, &model);
        async {
            let result = provider.transcribe(request).await;
            if let Ok(ref r) = result {
                observability::record_request_duration(
                    &model,
                    std::time::Duration::from_millis(r.processing_latency_ms),
                );
                observability::record_audio_duration(&model, r.audio_duration_ms);
            }
            result
        }
        .instrument(span)
        .await
    }

    pub async fn start_stream(
        &self,
        mut request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        Self::validate_provider_options_require_model(&request.provider_options, &request.model)?;
        let trace_id = Self::ensure_trace_id(&mut request.options);
        let provider = {
            let _span = observability::router_select_span(&trace_id).entered();
            self.router.select_for_streaming(&request)?
        };
        let model = provider.model_name().to_string();
        let caps = provider.capabilities();
        let compat = validate_streaming_request(&request, &caps)?;
        if !compat.adjustments.is_empty() {
            observability::record_option_adjustment_count(&model, compat.adjustments.len() as u32);
        }
        let span = observability::gateway_stream_span(&trace_id, &model);
        async { provider.start_stream(request).await }
            .instrument(span)
            .await
    }
}

// ---------------------------------------------------------------------------
// Route config parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct AsrRouteConfigFile {
    #[serde(default)]
    pub routes: Vec<AsrRouteEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AsrRouteEntry {
    pub model: String,
    pub priority: u8,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub regions: Option<Vec<String>>,
    #[serde(default)]
    pub max_latency_ms: Option<u64>,
    #[serde(default)]
    pub max_cost_micros_per_minute: Option<u64>,
}

pub fn parse_route_config(toml_content: &str) -> Result<Vec<AsrRoute>, AsrError> {
    let config: AsrRouteConfigFile = toml::from_str(toml_content).map_err(|e| {
        AsrError::new(
            AsrErrorCode::InvalidRequest,
            format!("failed to parse route config: {}", e),
        )
    })?;

    let mut routes = Vec::with_capacity(config.routes.len());
    for entry in config.routes {
        normalize_asr_provider_model(&entry.model)?;
        routes.push(AsrRoute {
            languages: entry.languages.into_iter().map(Language::new).collect(),
            regions: entry
                .regions
                .unwrap_or_default()
                .into_iter()
                .map(NetworkRegion::new)
                .collect(),
            max_latency_ms: entry.max_latency_ms,
            max_cost_micros_per_minute: entry.max_cost_micros_per_minute,
            priority: entry.priority,
            model: entry.model,
        });
    }
    Ok(routes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_route_config() {
        let toml = r#"
[[routes]]
model = "volcengine/bigmodel_async"
priority = 10
languages = ["zh-CN"]
regions = ["cn"]
max_latency_ms = 800

[[routes]]
model = "aliyun/fun-asr-realtime"
priority = 20
languages = ["zh-CN", "en", "ja"]
regions = ["cn"]
"#;
        let routes = parse_route_config(toml).unwrap();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].model, "volcengine/bigmodel_async");
        assert_eq!(routes[0].priority, 10);
        assert_eq!(routes[1].languages.len(), 3);
    }

    #[test]
    fn parse_route_config_rejects_bare_model() {
        let toml = r#"
[[routes]]
model = "bigmodel_async"
priority = 10
languages = ["zh-CN"]
"#;
        let err = parse_route_config(toml).unwrap_err();
        assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    }

    #[test]
    fn parse_empty_routes() {
        let routes = parse_route_config("[routes]\n").unwrap_err();
        // toml expects [[routes]] for array, [routes] is a table
        assert_eq!(routes.code, AsrErrorCode::InvalidRequest);
    }

    #[test]
    fn parse_no_routes_key() {
        let routes = parse_route_config("").unwrap();
        assert!(routes.is_empty());
    }
}
