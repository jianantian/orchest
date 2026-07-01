//! Issue 005 acceptance: the chat push→pull convergence.
//!
//! A push-based [`ChatModel`] (writes to `complete`'s `tx`) is bridged by
//! [`orchest_provider_http::events`] into a **pulled** `EventStream`. Proves the
//! pull path delivers the provider's events in order and terminates, while the
//! provider keeps its push transport unchanged (the working compat bridge).

use std::sync::Arc;

use async_trait::async_trait;
use orchest_protocol::{
    ChatModel, Message, ModelCapabilities, ModelError, ModelResponse, RequestOptions, StopReason,
    StreamEvent, TokenUsage, ToolDef,
};
use orchest_provider_http::events;
use tokio::sync::mpsc;

/// A chat model that only knows how to push into `complete`'s `tx`.
struct PushChat;

#[async_trait]
impl ChatModel for PushChat {
    fn provider_name(&self) -> &str {
        "fake"
    }
    fn model_name(&self) -> &str {
        "push-1"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        if let Some(tx) = tx {
            tx.send(StreamEvent::Text {
                delta: "hello".into(),
            })
            .await
            .unwrap();
            tx.send(StreamEvent::Text {
                delta: " world".into(),
            })
            .await
            .unwrap();
            tx.send(StreamEvent::Done {
                usage: TokenUsage::default(),
            })
            .await
            .unwrap();
        }
        Ok(ModelResponse {
            content: vec![],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

#[tokio::test]
async fn events_bridges_push_complete_onto_pulled_stream() {
    let mut stream = events(
        Arc::new(PushChat),
        vec![],
        vec![],
        RequestOptions::default(),
    );

    let mut deltas = String::new();
    let mut saw_done = false;
    while let Some(ev) = stream.next().await {
        match ev {
            StreamEvent::Text { delta } => deltas.push_str(&delta),
            StreamEvent::Done { .. } => saw_done = true,
            other => panic!("unexpected event on pulled stream: {other:?}"),
        }
    }
    assert_eq!(deltas, "hello world");
    assert!(saw_done, "pulled stream must carry the terminal Done");
}
