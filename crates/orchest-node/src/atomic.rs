use napi_derive::napi;
use orchest::atomic::{complete as atomic_complete, CompletionRequest};
use orchest::model::{ProviderRuntimeConfig, ResponseFormat};
use orchest::run::RetryPolicy;
use orchest_provider::create_adapter_from_config;

#[napi(object)]
pub struct CompletionOptions {
    pub model: String,
    pub user: String,
    pub system: Option<String>,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub api_url: Option<String>,
    pub json_mode: Option<bool>,
    pub retry: Option<bool>,
    pub request_options: Option<super::RequestOptions>,
}

#[napi]
pub async fn complete(input: CompletionOptions) -> napi::Result<String> {
    let mut options = input
        .request_options
        .map(super::rust_request_options_from_js)
        .transpose()
        .map_err(napi::Error::from_reason)?
        .unwrap_or_default();
    if input.json_mode.unwrap_or(false) {
        options.response_format = ResponseFormat::JsonObject;
    }
    let adapter = create_adapter_from_config(ProviderRuntimeConfig {
        model: input.model,
        api_key: input.api_key,
        api_key_env: input.api_key_env,
        api_url: input.api_url,
        max_tokens: options.max_tokens,
    })
    .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    atomic_complete(
        adapter.as_ref(),
        CompletionRequest {
            system: input.system,
            user: input.user,
            options,
            retry_policy: input.retry.unwrap_or(false).then(RetryPolicy::recommended),
        },
    )
    .await
    .map_err(|error| napi::Error::from_reason(error.to_string()))
}
