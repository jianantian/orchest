use agent_runtime_providers::{
    create_adapter_from_config, ProviderRuntimeConfig, RequestOptions, ThinkingLevel,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _adapter = create_adapter_from_config(ProviderRuntimeConfig {
        model: "deepseek/deepseek-reasoner".into(),
        api_key: Some("example-key".into()),
        api_key_env: None,
        api_url: Some("http://127.0.0.1:9".into()),
        max_tokens: Some(4096),
    })?;

    let _options = RequestOptions {
        thinking: ThinkingLevel::High,
        max_tokens: Some(1024),
        ..Default::default()
    };

    Ok(())
}
