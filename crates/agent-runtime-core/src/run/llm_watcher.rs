//! LlmWatcher: LLM-powered event monitor that evaluates batched events and decides actions.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;

use crate::events::RuntimeEvent;
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelResponse, RequestOptions, Role, ToolDef,
};

use super::watcher::{Watcher, WatcherAction};

pub struct LlmWatcher {
    model: Arc<dyn ModelAdapter>,
    system_prompt: String,
    eval_interval: usize,
    event_buffer: Mutex<Vec<String>>,
    event_count: AtomicUsize,
}

pub struct LlmWatcherBuilder {
    model: Option<Arc<dyn ModelAdapter>>,
    system_prompt: String,
    eval_interval: usize,
}

impl LlmWatcherBuilder {
    pub fn new() -> Self {
        Self {
            model: None,
            system_prompt: default_system_prompt().to_string(),
            eval_interval: 10,
        }
    }

    pub fn model(mut self, m: Arc<dyn ModelAdapter>) -> Self {
        self.model = Some(m);
        self
    }

    pub fn system_prompt(mut self, s: impl Into<String>) -> Self {
        self.system_prompt = s.into();
        self
    }

    pub fn eval_interval(mut self, n: usize) -> Self {
        self.eval_interval = n.max(1);
        self
    }

    pub fn build(self) -> LlmWatcher {
        LlmWatcher {
            model: self.model.expect("LlmWatcherBuilder requires a model"),
            system_prompt: self.system_prompt,
            eval_interval: self.eval_interval,
            event_buffer: Mutex::new(Vec::new()),
            event_count: AtomicUsize::new(0),
        }
    }
}

impl Default for LlmWatcherBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn default_system_prompt() -> &'static str {
    "You are a supervisor monitoring an AI agent's execution. \
     Review the events and decide whether to continue, inject a user message, \
     steer with a system instruction, or abort the run. \
     Use the decide_action tool to report your decision."
}

fn decide_action_tool() -> ToolDef {
    ToolDef {
        name: "decide_action".to_string(),
        description: "Decide what action to take based on observed events.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["continue", "inject", "steer", "abort"],
                    "description": "The action to take."
                },
                "message": {
                    "type": "string",
                    "description": "For inject/steer/abort: the message, instruction, or reason."
                }
            },
            "required": ["action"]
        }),
    }
}

pub fn format_event(event: &RuntimeEvent) -> String {
    match event {
        RuntimeEvent::ToolCallStarted {
            tool,
            metadata,
            input,
        } => {
            let input_summary = serde_json::to_string(input)
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>();
            format!(
                "Tool called: {} (side_effect: {}), input: {}",
                tool, metadata.side_effect, input_summary
            )
        }
        RuntimeEvent::ToolCallCompleted { tool, duration, .. } => format!(
            "Tool completed: {}, duration: {}ms",
            tool,
            duration.as_millis()
        ),
        RuntimeEvent::ToolCallFailed { tool, error } => {
            format!(
                "Tool failed: {}, kind: {:?}, message: {}",
                tool, error.kind, error.message
            )
        }
        RuntimeEvent::ModelCallStarted { step } => format!("Model call started: step {}", step),
        RuntimeEvent::ModelCallCompleted { tokens, .. } => {
            format!(
                "Model call completed: {} input + {} output tokens",
                tokens.input_tokens, tokens.output_tokens
            )
        }
        RuntimeEvent::RunStarted { run_id } => format!("Run started: {}", run_id),
        RuntimeEvent::RunCompleted { .. } => "Run completed".to_string(),
        RuntimeEvent::RunFailed { error } => format!("Run failed: {}", error),
        RuntimeEvent::RunAborted { reason } => {
            format!("Run aborted: {}", reason.as_deref().unwrap_or("unknown"))
        }
        RuntimeEvent::RunRestarted { attempt } => format!("Run restarted: attempt {}", attempt),
        RuntimeEvent::BudgetWarning { .. } => "Budget warning".to_string(),
        _ => format!("{:?}", event).chars().take(200).collect(),
    }
}

fn parse_action(response: &ModelResponse) -> WatcherAction {
    for block in &response.content {
        if let ContentBlock::ToolUse { input, name, .. } = block {
            if name == "decide_action" {
                let action = input
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("continue");
                let message = input
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                return match action {
                    "inject" => WatcherAction::Inject(message),
                    "steer" => WatcherAction::Steer(message),
                    "abort" => WatcherAction::Abort(message),
                    _ => WatcherAction::Continue,
                };
            }
        }
    }
    WatcherAction::Continue
}

impl LlmWatcher {
    pub fn builder() -> LlmWatcherBuilder {
        LlmWatcherBuilder::new()
    }

    async fn evaluate(&self) -> WatcherAction {
        let events: Vec<String> = {
            let mut buf = self.event_buffer.lock().await;
            buf.drain(..).collect()
        };

        if events.is_empty() {
            return WatcherAction::Continue;
        }

        let event_summary = events.join("\n");
        let messages = vec![
            Message {
                role: Role::System,
                content: vec![ContentBlock::Text(self.system_prompt.clone())],
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text(format!(
                    "Recent agent events:\n{}",
                    event_summary
                ))],
            },
        ];

        let tools = vec![decide_action_tool()];
        match self
            .model
            .complete(&messages, &tools, &RequestOptions::default(), None)
            .await
        {
            Ok(response) => parse_action(&response),
            Err(_) => WatcherAction::Continue,
        }
    }
}

#[async_trait]
impl Watcher for LlmWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        let formatted = format_event(event);
        self.event_buffer.lock().await.push(formatted);
        let count = self.event_count.fetch_add(1, Ordering::Relaxed) + 1;

        if count.is_multiple_of(self.eval_interval) {
            self.evaluate().await
        } else {
            WatcherAction::Continue
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelError, StopReason, TokenUsage};
    use crate::tool::{ErrorKind, ToolError, ToolMetadata, ToolSource};
    use std::time::Duration;

    struct MockModel {
        response: ModelResponse,
    }

    impl MockModel {
        fn with_action(action: &str, message: &str) -> Self {
            Self {
                response: ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "call_1".to_string(),
                        name: "decide_action".to_string(),
                        input: json!({ "action": action, "message": message }),
                    }],
                    usage: TokenUsage::default(),
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                },
            }
        }

        fn failing() -> Self {
            Self {
                response: ModelResponse {
                    content: vec![],
                    usage: TokenUsage::default(),
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                },
            }
        }
    }

    #[async_trait]
    impl ModelAdapter for MockModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> crate::model::ModelCapabilities {
            crate::model::ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio::sync::mpsc::Sender<crate::model::ModelStreamChunk>>,
        ) -> Result<ModelResponse, ModelError> {
            Ok(self.response.clone())
        }
    }

    struct FailingModel;

    #[async_trait]
    impl ModelAdapter for FailingModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn capabilities(&self) -> crate::model::ModelCapabilities {
            crate::model::ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio::sync::mpsc::Sender<crate::model::ModelStreamChunk>>,
        ) -> Result<ModelResponse, ModelError> {
            Err(ModelError {
                message: "connection failed".to_string(),
                code: None,
                provider: None,
                status: None,
                retry_after_secs: None,
                upstream: None,
            })
        }
    }

    fn run_started() -> RuntimeEvent {
        RuntimeEvent::RunStarted {
            run_id: crate::run::RunId::new(),
        }
    }

    fn tool_started() -> RuntimeEvent {
        RuntimeEvent::ToolCallStarted {
            tool: "test_tool".to_string(),
            metadata: ToolMetadata {
                source: ToolSource::Builtin,
                side_effect: false,
                timeout: None,
                max_output_tokens: None,
                approval: crate::tool::Approval::Never,
                cost_hint: None,
            },
            input: json!({"query": "test"}),
        }
    }

    fn tool_failed() -> RuntimeEvent {
        RuntimeEvent::ToolCallFailed {
            tool: "bad_tool".to_string(),
            error: ToolError::fatal("not found").with_code("NOT_FOUND"),
        }
    }

    #[tokio::test]
    async fn accumulates_events_before_eval_interval() {
        let model = Arc::new(MockModel::with_action("abort", "stop now"));
        let watcher = LlmWatcher::builder().model(model).eval_interval(3).build();

        let r1 = watcher.on_event(&run_started()).await;
        assert!(matches!(r1, WatcherAction::Continue));
        let r2 = watcher.on_event(&tool_started()).await;
        assert!(matches!(r2, WatcherAction::Continue));
        let r3 = watcher.on_event(&tool_failed()).await;
        assert!(matches!(r3, WatcherAction::Abort(_)));
    }

    #[tokio::test]
    async fn maps_steer_action() {
        let model = Arc::new(MockModel::with_action("steer", "focus on task"));
        let watcher = LlmWatcher::builder().model(model).eval_interval(1).build();
        let result = watcher.on_event(&run_started()).await;
        assert!(matches!(result, WatcherAction::Steer(msg) if msg == "focus on task"));
    }

    #[tokio::test]
    async fn maps_inject_action() {
        let model = Arc::new(MockModel::with_action("inject", "try another approach"));
        let watcher = LlmWatcher::builder().model(model).eval_interval(1).build();
        let result = watcher.on_event(&run_started()).await;
        assert!(matches!(result, WatcherAction::Inject(msg) if msg == "try another approach"));
    }

    #[tokio::test]
    async fn maps_continue_action() {
        let model = Arc::new(MockModel::with_action("continue", ""));
        let watcher = LlmWatcher::builder().model(model).eval_interval(1).build();
        let result = watcher.on_event(&run_started()).await;
        assert!(matches!(result, WatcherAction::Continue));
    }

    #[tokio::test]
    async fn model_failure_degrades_to_continue() {
        let model: Arc<dyn ModelAdapter> = Arc::new(FailingModel);
        let watcher = LlmWatcher::builder().model(model).eval_interval(1).build();
        let result = watcher.on_event(&run_started()).await;
        assert!(matches!(result, WatcherAction::Continue));
    }

    #[tokio::test]
    async fn no_tool_use_degrades_to_continue() {
        let model = Arc::new(MockModel::failing());
        let watcher = LlmWatcher::builder().model(model).eval_interval(1).build();
        let result = watcher.on_event(&run_started()).await;
        assert!(matches!(result, WatcherAction::Continue));
    }

    #[test]
    fn format_event_tool_started() {
        let s = format_event(&tool_started());
        assert!(s.contains("test_tool"));
        assert!(s.contains("side_effect: false"));
    }

    #[test]
    fn format_event_tool_failed() {
        let s = format_event(&tool_failed());
        assert!(s.contains("bad_tool"));
        assert!(s.contains("not found"));
    }

    #[test]
    fn format_event_tool_completed() {
        let s = format_event(&RuntimeEvent::ToolCallCompleted {
            tool: "my_tool".to_string(),
            output: json!("ok"),
            duration: Duration::from_millis(42),
        });
        assert!(s.contains("my_tool"));
        assert!(s.contains("42ms"));
    }
}
