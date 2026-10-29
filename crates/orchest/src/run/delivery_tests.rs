//! Lossless event delivery tests (v2.0 issue 008).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use super::checkpoint_tests::{ScriptedModel, Turn};
use super::*;
use crate::events::{MessageDelta, RuntimeEvent};
use crate::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use crate::tool::registry::ToolRegistry;
use crate::tool::ToolDef;

/// Streams `chunks` text deltas carrying their index.
struct CountingModel {
    chunks: usize,
}

#[async_trait::async_trait]
impl ModelAdapter for CountingModel {
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_name(&self) -> &str {
        "mock"
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
            for index in 0..self.chunks {
                let _ = tx
                    .send(StreamEvent::Text {
                        delta: index.to_string(),
                    })
                    .await;
            }
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("done".into())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

fn config(capacity: usize) -> AgentConfig {
    let mut config = AgentConfig::builder("delivery-agent", "mock")
        .system_prompt("system")
        .max_steps(5)
        .event_channel_capacity(capacity)
        .build()
        .unwrap();
    config.supervision_strategy = SupervisionStrategy::Stop;
    config
}

fn text_deltas(events: &[RuntimeEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            RuntimeEvent::MessageUpdated {
                delta: MessageDelta::Text { text },
                ..
            } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn slow_consumer_receives_every_event_in_order() {
    const CHUNKS: usize = 10_000;
    let (handle, mut rx) = AgentRun::start(
        config(8),
        "hi".into(),
        Arc::new(CountingModel { chunks: CHUNKS }),
        ToolRegistry::new(),
    );
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        if events.len() % 500 == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        events.push(event);
    }
    let stats = handle.event_delivery_stats();
    handle.wait().await;

    let expected: Vec<String> = (0..CHUNKS).map(|i| i.to_string()).collect();
    assert_eq!(text_deltas(&events), expected);
    assert!(!events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::EventsDropped { .. })));
    assert!(matches!(
        events.last(),
        Some(RuntimeEvent::RunCompleted { .. })
    ));
    assert_eq!(stats.delivered, events.len() as u64);
    assert_eq!(stats.capacity, 8);
    assert!(stats.backpressure_waits > 0);
    assert_eq!(stats.high_watermark, 8);
}

#[tokio::test]
async fn a_full_channel_holds_the_run_back_instead_of_dropping() {
    const CHUNKS: usize = 2_000;
    let (handle, mut rx) = AgentRun::start(
        config(4),
        "hi".into(),
        Arc::new(CountingModel { chunks: CHUNKS }),
        ToolRegistry::new(),
    );

    // Nobody drains: the run stalls on backpressure.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let stalled = handle.event_delivery_stats();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(handle.event_delivery_stats().delivered, stalled.delivered);
    assert!(stalled.delivered < CHUNKS as u64);
    assert_eq!(stalled.high_watermark, 4);

    // Draining releases it, and nothing was lost meanwhile.
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    handle.wait().await;
    assert_eq!(text_deltas(&events).len(), CHUNKS);
    assert!(matches!(
        events.last(),
        Some(RuntimeEvent::RunCompleted { .. })
    ));
}

/// Records appends and the length at each flush.
#[derive(Default)]
struct MemorySink {
    events: Mutex<Vec<(u64, String)>>,
    flushed: Mutex<Vec<usize>>,
}

#[async_trait::async_trait]
impl DurableEventSink for MemorySink {
    async fn append(&self, seq: u64, event: &RuntimeEvent) -> Result<(), String> {
        let json = serde_json::to_string(event).map_err(|e| e.to_string())?;
        self.events.lock().unwrap().push((seq, json));
        Ok(())
    }
    async fn flush(&self) -> Result<(), String> {
        let len = self.events.lock().unwrap().len();
        self.flushed.lock().unwrap().push(len);
        Ok(())
    }
}

#[tokio::test]
async fn durable_sink_holds_the_whole_stream_through_a_crash() {
    let sink = Arc::new(MemorySink::default());
    let mut config = config(16);
    config.runtime.event_sink = Some(sink.clone());
    let model = ScriptedModel::new(vec![
        Turn::CallTool {
            id: "r1",
            name: "missing",
        },
        Turn::Crash,
    ]);
    let (handle, mut rx) = AgentRun::start(config, "go".into(), model, ToolRegistry::new());
    let mut received = Vec::new();
    while let Some(event) = rx.recv().await {
        received.push(serde_json::to_string(&event).unwrap());
    }
    handle.wait().await;

    let stored = sink.events.lock().unwrap().clone();
    let seqs: Vec<u64> = stored.iter().map(|(seq, _)| *seq).collect();
    assert_eq!(seqs, (1..=stored.len() as u64).collect::<Vec<_>>());
    let stored: Vec<String> = stored.into_iter().map(|(_, json)| json).collect();
    assert_eq!(stored, received);
    assert!(
        stored.last().unwrap().contains("RunAborted"),
        "{:?}",
        stored.last()
    );
    // Flushed through the terminal event.
    assert_eq!(sink.flushed.lock().unwrap().last(), Some(&stored.len()));
}

#[tokio::test]
async fn sink_failures_are_counted_and_delivery_continues() {
    struct FailingSink;

    #[async_trait::async_trait]
    impl DurableEventSink for FailingSink {
        async fn append(&self, _seq: u64, _event: &RuntimeEvent) -> Result<(), String> {
            Err("disk full".into())
        }
        async fn flush(&self) -> Result<(), String> {
            Ok(())
        }
    }

    let mut config = config(16);
    config.runtime.event_sink = Some(Arc::new(FailingSink));
    let (handle, mut rx) = AgentRun::start(
        config,
        "hi".into(),
        Arc::new(CountingModel { chunks: 3 }),
        ToolRegistry::new(),
    );
    let mut count = 0;
    while rx.recv().await.is_some() {
        count += 1;
    }
    let stats = handle.event_delivery_stats();
    handle.wait().await;
    assert_eq!(stats.sink_errors, count);
    assert_eq!(stats.delivered, count);
}
