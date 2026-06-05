use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::error::AsrError;
use crate::streaming::AsrStream;
use crate::traits::AsrProvider;
use crate::types::{
    Language, NetworkRegion, StreamingTranscribeRequest, TranscribeRequest, TranscribeResult,
};

use serde::{Deserialize, Serialize};

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

    pub async fn transcribe(
        &self,
        _request: TranscribeRequest,
    ) -> Result<TranscribeResult, AsrError> {
        todo!("gateway transcribe routing — implemented in issue 002")
    }

    pub async fn start_stream(
        &self,
        _request: StreamingTranscribeRequest,
    ) -> Result<AsrStream, AsrError> {
        todo!("gateway stream routing — implemented in issue 002")
    }
}
