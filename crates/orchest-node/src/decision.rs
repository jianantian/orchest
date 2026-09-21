use std::collections::BTreeMap;

use napi_derive::napi;
use orchest_protocol::{DecisionQuestion, DecisionRequest, ErrorCode, ProtocolError};
use orchest_provider::{decide as provider_decide, DecisionConfig};
use serde_json::Value;

#[napi(object)]
pub struct DecisionOptions {
    pub model: String,
    pub state: Value,
    pub questions: Value,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub api_url: Option<String>,
    pub timeout_ms: Option<u32>,
}

#[napi(js_name = "_decide")]
pub async fn decide(input: DecisionOptions) -> napi::Result<Value> {
    let questions = serde_json::from_value::<BTreeMap<String, DecisionQuestion>>(input.questions)
        .map_err(|error| {
        super::error::protocol_error(
            ProtocolError::new(
                ErrorCode::InvalidRequest,
                format!("invalid decision questions: {error}"),
            )
            .with_model(input.model.clone()),
        )
    })?;
    let response = provider_decide(
        &DecisionConfig {
            model: input.model,
            api_key: input.api_key,
            api_key_env: input.api_key_env,
            api_url: input.api_url,
            timeout_ms: input.timeout_ms.map(u64::from),
        },
        DecisionRequest {
            state: input.state,
            questions,
        },
    )
    .await
    .map_err(super::error::protocol_error)?;
    serde_json::to_value(response).map_err(|error| napi::Error::from_reason(error.to_string()))
}
