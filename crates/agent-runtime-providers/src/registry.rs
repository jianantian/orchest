//! ProviderFactory trait and ProviderRegistry for adapter creation.

use std::collections::HashMap;

use agent_runtime_model::{ModelAdapter, ModelError};

/// Trait for provider-specific adapter creation. Each provider implements this
/// to register itself with the [`ProviderRegistry`].
pub trait ProviderFactory: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError>;
    fn default_api_key_env(&self) -> &'static str;
}

/// Registry of provider factories. Centralises adapter creation so new
/// providers only need to implement [`ProviderFactory`] and register.
pub struct ProviderRegistry {
    factories: HashMap<&'static str, Box<dyn ProviderFactory>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            factories: HashMap::new(),
        };
        reg.register(Box::new(super::providers::anthropic::AnthropicFactory));
        reg.register(Box::new(super::providers::openai::OpenAiFactory));
        reg.register(Box::new(super::providers::deepseek::DeepSeekFactory));
        reg.register(Box::new(super::providers::openrouter::OpenRouterFactory));
        reg.register(Box::new(super::providers::volcengine::VolcengineFactory));
        reg
    }

    pub fn register(&mut self, factory: Box<dyn ProviderFactory>) {
        self.factories.insert(factory.provider_name(), factory);
    }

    pub fn get(&self, provider: &str) -> Option<&dyn ProviderFactory> {
        self.factories.get(provider).map(|f| f.as_ref())
    }

    pub fn supported_providers(&self) -> Vec<&str> {
        let mut names: Vec<_> = self.factories.keys().copied().collect();
        names.sort();
        names
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
