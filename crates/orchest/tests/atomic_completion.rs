use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use orchest::atomic::{complete, CompletionRequest};
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, ResponseFormat, Role, StopReason, StreamEvent, TokenUsage, ToolDef,
};
use orchest::run::{BackoffStrategy, RetryPolicy};
use tokio::sync::mpsc;

#[derive(Debug)]
struct SeenCall {
    messages: Vec<Message>,
    tool_count: usize,
    streamed: bool,
}

struct ScriptedModel {
    responses: Mutex<VecDeque<Result<ModelResponse, ModelError>>>,
    calls: Mutex<Vec<SeenCall>>,
}

impl ScriptedModel {
    fn new(responses: impl IntoIterator<Item = Result<ModelResponse, ModelError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> std::sync::MutexGuard<'_, Vec<SeenCall>> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl ModelAdapter for ScriptedModel {
    fn provider_name(&self) -> &str {
        "fake"
    }

    fn model_name(&self) -> &str {
        "scripted"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(SeenCall {
                messages: messages.to_vec(),
                tool_count: tools.len(),
                streamed: tx.is_some(),
            });
        self.responses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .unwrap_or_else(|| Err(ModelError::internal("script exhausted", "script_exhausted")))
    }
}

fn response(blocks: Vec<ContentBlock>, stop_reason: StopReason) -> ModelResponse {
    ModelResponse {
        content: blocks,
        usage: TokenUsage::default(),
        stop_reason,
        option_adjustments: Vec::new(),
    }
}

#[tokio::test]
async fn atomic_completion_builds_messages_without_tools_or_streaming() {
    let model = ScriptedModel::new([Ok(response(
        vec![
            ContentBlock::Text("hello ".into()),
            ContentBlock::Thinking {
                text: Some("secret".into()),
                signature: None,
                provider_details: None,
            },
            ContentBlock::Text("world".into()),
        ],
        StopReason::EndTurn,
    ))]);

    let text = complete(
        &model,
        CompletionRequest {
            system: Some("system".into()),
            user: "user".into(),
            options: RequestOptions::default(),
            retry_policy: None,
        },
    )
    .await
    .expect("atomic completion succeeds");

    assert_eq!(text, "hello world");
    let calls = model.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_count, 0);
    assert!(!calls[0].streamed);
    assert_eq!(calls[0].messages.len(), 2);
    assert_eq!(calls[0].messages[0].role, Role::System);
    assert_eq!(calls[0].messages[1].role, Role::User);
}

#[tokio::test]
async fn empty_system_is_omitted_and_stop_sequence_succeeds() {
    let model = ScriptedModel::new([Ok(response(
        vec![ContentBlock::Text("done".into())],
        StopReason::StopSequence,
    ))]);

    let text = complete(
        &model,
        CompletionRequest {
            system: Some(String::new()),
            user: "user".into(),
            options: RequestOptions::default(),
            retry_policy: None,
        },
    )
    .await
    .expect("stop sequence is successful");

    assert_eq!(text, "done");
    assert_eq!(model.calls()[0].messages.len(), 1);
}

#[tokio::test]
async fn incomplete_stop_reasons_are_errors() {
    let reasons = [
        StopReason::ToolUse,
        StopReason::MaxTokens,
        StopReason::ContentFilter,
        StopReason::Refusal,
        StopReason::ContextWindowExceeded,
        StopReason::Pause,
        StopReason::Interrupted,
        StopReason::Other("vendor".into()),
    ];

    for reason in reasons {
        let model = ScriptedModel::new([Ok(response(
            vec![ContentBlock::Text("partial".into())],
            reason.clone(),
        ))]);
        let error = complete(
            &model,
            CompletionRequest {
                system: None,
                user: "user".into(),
                options: RequestOptions::default(),
                retry_policy: None,
            },
        )
        .await
        .expect_err("incomplete response must fail");

        assert_eq!(error.code.as_deref(), Some("atomic_incomplete"));
    }
}

#[tokio::test]
async fn json_mode_requires_a_json_object_but_returns_original_text() {
    let options = RequestOptions {
        response_format: ResponseFormat::JsonObject,
        ..RequestOptions::default()
    };
    let valid = "{\n  \"answer\": 42\n}";
    let model = ScriptedModel::new([Ok(response(
        vec![ContentBlock::Text(valid.into())],
        StopReason::EndTurn,
    ))]);

    let text = complete(
        &model,
        CompletionRequest {
            system: None,
            user: "user".into(),
            options: options.clone(),
            retry_policy: None,
        },
    )
    .await
    .expect("JSON object succeeds");
    assert_eq!(text, valid);

    for invalid in ["not json", "[1, 2]", "null"] {
        let model = ScriptedModel::new([Ok(response(
            vec![ContentBlock::Text(invalid.into())],
            StopReason::EndTurn,
        ))]);
        let error = complete(
            &model,
            CompletionRequest {
                system: None,
                user: "user".into(),
                options: options.clone(),
                retry_policy: None,
            },
        )
        .await
        .expect_err("non-object JSON must fail");
        assert_eq!(error.code.as_deref(), Some("invalid_json_object"));
    }
}

#[tokio::test]
async fn retry_policy_retries_transient_errors_only() {
    let transient = ModelError {
        message: "busy".into(),
        code: None,
        provider: Some("fake".into()),
        status: Some(503),
        retry_after_secs: None,
        upstream: None,
    };
    let model = ScriptedModel::new([
        Err(transient),
        Ok(response(
            vec![ContentBlock::Text("recovered".into())],
            StopReason::EndTurn,
        )),
    ]);

    let text = complete(
        &model,
        CompletionRequest {
            system: None,
            user: "user".into(),
            options: RequestOptions::default(),
            retry_policy: Some(RetryPolicy {
                max_retries: 1,
                backoff: BackoffStrategy::Fixed(Duration::ZERO),
            }),
        },
    )
    .await
    .expect("transient error is retried");

    assert_eq!(text, "recovered");
    assert_eq!(model.calls().len(), 2);
}
