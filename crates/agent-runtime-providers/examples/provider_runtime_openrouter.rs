use agent_runtime_providers::{create_adapter_from_config, ProviderRuntimeConfig, RequestOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = create_adapter_from_config(ProviderRuntimeConfig {
        model: "openrouter/anthropic/claude-sonnet-4".into(),
        api_key: Some("example-key".into()),
        api_key_env: None,
        api_url: Some("http://127.0.0.1:9".into()),
        max_tokens: None,
    })?;

    assert_eq!(adapter.provider_name(), "openrouter");
    assert_eq!(adapter.model_name(), "anthropic/claude-sonnet-4");

    let _options = RequestOptions::default();
    Ok(())
}
