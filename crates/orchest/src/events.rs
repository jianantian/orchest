//! Runtime event types emitted during agent execution.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::budget::{BudgetConfig, BudgetUsage};
use crate::model::{ModelStreamChunk, OptionAdjustment, StopReason, TokenUsage};
use crate::run::RunId;
use crate::tool::async_job::JobStatus;
use crate::tool::{ToolCall, ToolError, ToolMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ApprovalContext {
    #[default]
    InitialToolCall,
    CommitToolCall {
        draft_tool: String,
    },
    RetryAfterFailure {
        attempt: u32,
        previous_error: ToolError,
    },
}

/// Machine-dispatchable classification of a run failure, carried by
/// [`RuntimeEvent::RunFailed`]. The `error` text remains the human- and
/// model-facing diagnostic; `kind` is what consumers dispatch on. Only
/// categories with a real consumer get their own variant — everything else
/// is [`RunFailureKind::Other`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunFailureKind {
    /// The budget guard fired; `error` starts with `"budget_exceeded"`.
    BudgetExceeded,
    /// The run hit `runtime.max_steps`; `error` is `"max_steps_reached"`.
    MaxStepsReached,
    /// Any other failure. Also the serde default for events serialized
    /// before `kind` existed — those carried only the opaque error string.
    #[default]
    Other,
}

/// An event emitted on the run's event stream: run lifecycle, model calls,
/// tool calls, approvals, budget, sub-agents, and steering. Consumers receive
/// these from the `EventReceiver` returned by `AgentRun::start`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEvent {
    RunStarted {
        run_id: RunId,
    },

    ModelCallStarted {
        step: u32,
    },
    ModelStreamChunk {
        delta: ModelStreamChunk,
    },
    ModelCallCompleted {
        tokens: TokenUsage,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        option_adjustments: Vec<OptionAdjustment>,
    },
    /// A model call failed with a retryable error and will be retried after
    /// `next_delay`. Subscribers that accumulated partial `ModelStreamChunk`s
    /// from the interrupted attempt must drop that buffer: the retry restarts
    /// the stream from the beginning.
    ModelRetry {
        attempt: u32,
        error: String,
        next_delay: Duration,
    },

    ToolCallStarted {
        tool: String,
        metadata: ToolMetadata,
        input: Value,
    },
    ToolCallUpdate {
        tool: String,
        tool_call_id: String,
        partial: Value,
    },
    ToolCallCompleted {
        tool: String,
        output: Value,
        duration: Duration,
    },
    ToolCallFailed {
        tool: String,
        error: ToolError,
    },
    ToolCallRetry {
        tool: String,
        attempt: u32,
        previous_error: ToolError,
        next_delay: Duration,
    },
    ToolCallBatchStarted {
        batch_id: String,
        tool_count: usize,
    },
    ToolCallBatchItemStarted {
        batch_id: String,
        tool: String,
        requested_order: usize,
    },
    ToolCallBatchItemCompleted {
        batch_id: String,
        tool: String,
        requested_order: usize,
        completion_order: usize,
    },

    AsyncToolStarted {
        tool: String,
        job_id: String,
    },
    AsyncToolProgress {
        tool: String,
        job_id: String,
        status: JobStatus,
    },
    AsyncToolCompleted {
        tool: String,
        job_id: String,
        output: Value,
        elapsed: Duration,
    },

    SkillContentRead {
        skill_name: String,
        file: String,
        tokens: u32,
    },

    ApprovalRequested {
        tool_call: ToolCall,
        #[serde(default)]
        context: ApprovalContext,
    },
    ApprovalGranted {
        tool_call: ToolCall,
        #[serde(default)]
        context: ApprovalContext,
    },
    ApprovalDenied {
        tool_call: ToolCall,
        #[serde(default)]
        context: ApprovalContext,
    },

    BudgetWarning {
        used: BudgetUsage,
        limit: BudgetConfig,
    },
    RuntimeWarning {
        message: String,
    },
    SkillMissingCapabilities {
        skill_name: String,
    },
    SkillLoadWarning {
        path: String,
        reason: String,
    },
    ContextCompacted {
        removed_messages: usize,
        summary_tokens: u32,
    },

    SubAgentStarted {
        parent_run_id: RunId,
        child_run_id: RunId,
        config_summary: Value,
    },
    SubAgentCompleted {
        child_run_id: RunId,
        output: Value,
        budget_used: BudgetUsage,
    },
    SubAgentFailed {
        child_run_id: RunId,
        error: String,
    },

    ChildRunEvent {
        child_run_id: RunId,
        run_depth: u32,
        event: Box<RuntimeEvent>,
    },
    SubAgentEvent {
        parent_run_id: RunId,
        child_run_id: RunId,
        event: Box<RuntimeEvent>,
    },

    HookPanicked {
        hook_name: String,
        message: String,
    },

    AgentUpdated {
        previous_agent: String,
        new_agent: String,
    },

    /// One or more events were not delivered to a subscriber under
    /// backpressure. Secondary subscribers receive this on their own channel
    /// once capacity frees (coalesced); the primary also observes a mirror
    /// for operator visibility. Missed payloads are not replayed — see
    /// [`deliver_to_subscribers`] recovery contract.
    EventsDropped {
        subscriber_id: u64,
        count: u64,
        /// Inclusive start of the lost delivery-sequence range. `0` when
        /// unknown (legacy events deserialized without this field).
        #[serde(default)]
        from_seq: u64,
        /// Inclusive end of the lost delivery-sequence range. `0` when
        /// unknown (legacy events deserialized without this field).
        #[serde(default)]
        to_seq: u64,
    },

    RunRestarted {
        attempt: u32,
    },

    /// The run finished normally; `output` is the final assistant text.
    ///
    /// `stop_reason` carries the model's stop reason for the completing turn:
    /// [`StopReason::EndTurn`] means the output is complete, while
    /// [`StopReason::MaxTokens`] means the model hit the token cap and
    /// `output` is **truncated**. Consumers must read this marker before
    /// treating `output` as final: on truncation, decide whether to continue
    /// generation (ask the model to keep writing), retry with a larger token
    /// budget, or surface an error. Persisting truncated output as-is (e.g.
    /// half-written HTML) is the failure mode this field exists to prevent.
    RunCompleted {
        output: Value,
        /// Why the completing model call stopped. Defaults to
        /// [`StopReason::EndTurn`] when deserializing events emitted before
        /// this field existed (they only distinguished normal completion).
        #[serde(default = "default_run_completed_stop_reason")]
        stop_reason: StopReason,
    },
    RunFailed {
        error: String,
        /// Structured failure category for machine dispatch. Defaults to
        /// [`RunFailureKind::Other`] when deserializing events emitted before
        /// this field existed (they carried only the opaque error string).
        #[serde(default)]
        kind: RunFailureKind,
    },
    RunAborted {
        reason: Option<String>,
    },
}

/// Serde default for `RunCompleted::stop_reason` on events serialized before
/// the field existed: those only distinguished normal completion, so
/// `EndTurn` (not truncated) is the faithful reading.
fn default_run_completed_stop_reason() -> StopReason {
    StopReason::EndTurn
}

/// Timeout for primary (blocking) event delivery. Matches the run actor.
pub const EVENT_DELIVERY_TIMEOUT: Duration = Duration::from_millis(500);

/// Coalesced secondary loss awaiting delivery on that subscriber's channel.
///
/// State is O(1) per subscriber: a count and inclusive sequence bounds. It
/// cannot grow with the number of lost events.
#[derive(Debug, Default)]
struct PendingLoss {
    count: u64,
    from_seq: u64,
    to_seq: u64,
}

/// One fan-out slot in a run's event subscriber list.
///
/// Index `0` is always the primary receiver (awaited send with timeout).
/// Remaining slots are secondary watcher / `subscribe_events` channels that
/// use non-blocking delivery with coalesced [`RuntimeEvent::EventsDropped`]
/// recovery signals.
///
/// # Recovery contract (SB-5 / #252)
///
/// Secondary delivery is lossy under backpressure so a slow watcher cannot
/// stall the run. When a secondary `try_send` fails:
///
/// 1. **Observe** — the loss is recorded with stable metadata
///    (`subscriber_id`, coalesced `count`, `from_seq`..=`to_seq`) and, once
///    the secondary channel has capacity, an `EventsDropped` is delivered on
///    *that same channel* before the next accepted event. A mirror signal is
///    also offered to the primary (best-effort) for operators.
/// 2. **Bounded failure** — lost event *payloads* are not retained or
///    replayed. The gap is permanent for content recovery; only the loss
///    signal is guaranteed once capacity frees.
/// 3. **Continue or resubscribe** — after observing `EventsDropped`, keep
///    consuming for subsequent FIFO events, or call
///    [`crate::run::RunHandle::subscribe_events`] /
///    [`crate::run::RunHandle::attach_watcher`] for a fresh channel that sees
///    only future events.
///
/// Primary backpressure semantics are unchanged: awaited send with
/// [`EVENT_DELIVERY_TIMEOUT`]. Recovery state per secondary is a fixed-size
/// pending counter (cannot grow without bound).
///
/// Flushing a pending `EventsDropped` consumes one channel slot before the
/// next event is offered. Subscribers that want the resumed event to land in
/// the same delivery should leave at least two free slots (or drain before the
/// runtime offers the next event).
#[derive(Clone, Debug)]
pub struct EventSink {
    tx: mpsc::Sender<RuntimeEvent>,
    subscriber_id: u64,
    next_seq: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// `None` for the primary slot (index 0).
    pending: Option<std::sync::Arc<std::sync::Mutex<PendingLoss>>>,
}

impl EventSink {
    /// Build the primary (index 0) sink sharing a run-scoped sequence counter.
    pub fn primary(
        tx: mpsc::Sender<RuntimeEvent>,
        next_seq: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            tx,
            subscriber_id: 0,
            next_seq,
            pending: None,
        }
    }

    /// Build a secondary sink. `subscriber_id` should match the fan-out index
    /// at registration time (stable for the life of this subscription).
    pub fn secondary(
        tx: mpsc::Sender<RuntimeEvent>,
        subscriber_id: u64,
        next_seq: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            tx,
            subscriber_id,
            next_seq,
            pending: Some(std::sync::Arc::new(std::sync::Mutex::new(
                PendingLoss::default(),
            ))),
        }
    }

    /// Shared run-scoped sequence allocator used by this fan-out.
    pub fn sequence_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicU64> {
        std::sync::Arc::clone(&self.next_seq)
    }

    /// Borrow the underlying channel sender (hooks / primary-only paths).
    pub fn sender(&self) -> &mpsc::Sender<RuntimeEvent> {
        &self.tx
    }

    fn alloc_seq(&self) -> u64 {
        self.next_seq
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    fn record_pending_locked(pending: &mut PendingLoss, seq: u64) {
        if pending.count == 0 {
            pending.from_seq = seq;
        }
        pending.count = pending.count.saturating_add(1);
        pending.to_seq = seq;
    }

    /// Offer one event to this secondary sink. Returns `true` when a loss was
    /// recorded (channel full).
    fn offer_secondary(&self, event: RuntimeEvent, seq: u64) -> bool {
        let Some(pending_mtx) = &self.pending else {
            return false;
        };
        let mut pending = pending_mtx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if pending.count > 0 {
            let signal = RuntimeEvent::EventsDropped {
                subscriber_id: self.subscriber_id,
                count: pending.count,
                from_seq: pending.from_seq,
                to_seq: pending.to_seq,
            };
            match self.tx.try_send(signal) {
                Ok(()) => {
                    pending.count = 0;
                    pending.from_seq = 0;
                    pending.to_seq = 0;
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    Self::record_pending_locked(&mut pending, seq);
                    return true;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => return false,
            }
        }

        match self.tx.try_send(event) {
            Ok(()) => false,
            Err(mpsc::error::TrySendError::Full(_)) => {
                Self::record_pending_locked(&mut pending, seq);
                true
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

/// Deliver a runtime event to a subscriber list using the public fan-out contract:
///
/// - index `0` is the primary event receiver — awaited send with
///   [`EVENT_DELIVERY_TIMEOUT`];
/// - remaining subscribers are attached watcher channels — non-blocking
///   `try_send` with coalesced [`RuntimeEvent::EventsDropped`] recovery on
///   the affected secondary (see [`EventSink`] recovery contract).
///
/// Each event is offered at most once per subscriber, preserving primary
/// order. Tools that forward nested child events should call
/// [`crate::tool::ToolContext::emit_event`], which uses this helper so
/// attached watchers observe the same stream as the primary receiver.
pub async fn deliver_to_subscribers(subscribers: &[EventSink], event: RuntimeEvent) {
    let seq = subscribers.first().map(EventSink::alloc_seq).unwrap_or(0);

    if let Some(primary) = subscribers.first() {
        match tokio::time::timeout(EVENT_DELIVERY_TIMEOUT, primary.tx.send(event.clone())).await {
            Ok(Ok(())) | Ok(Err(_)) => {}
            Err(_) => {
                crate::telemetry::record_event_drop("primary", 1);
                if primary
                    .tx
                    .try_send(RuntimeEvent::EventsDropped {
                        subscriber_id: 0,
                        count: 1,
                        from_seq: seq,
                        to_seq: seq,
                    })
                    .is_err()
                {
                    tracing::warn!(
                        "primary event subscriber timed out and EventsDropped notification channel is full"
                    );
                }
            }
        }
    }

    for sink in subscribers.iter().skip(1) {
        if sink.offer_secondary(event.clone(), seq) {
            crate::telemetry::record_event_drop("secondary", 1);
            if let Some(primary) = subscribers.first() {
                if primary
                    .tx
                    .try_send(RuntimeEvent::EventsDropped {
                        subscriber_id: sink.subscriber_id,
                        count: 1,
                        from_seq: seq,
                        to_seq: seq,
                    })
                    .is_err()
                {
                    tracing::warn!(
                        subscriber_id = sink.subscriber_id,
                        "secondary event subscriber dropped an event and primary notification channel is full"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn skill_load_warning_round_trips_through_serde() {
        let event = RuntimeEvent::SkillLoadWarning {
            path: "skills/bad/SKILL.md".to_string(),
            reason: "invalid frontmatter YAML: missing field `name`".to_string(),
        };

        let serialized = serde_json::to_string(&event).expect("serialize event");
        let deserialized: RuntimeEvent =
            serde_json::from_str(&serialized).expect("deserialize event");

        match deserialized {
            RuntimeEvent::SkillLoadWarning { path, reason } => {
                assert_eq!(path, "skills/bad/SKILL.md");
                assert!(reason.contains("invalid frontmatter YAML"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn sub_agent_event_round_trips_through_serde() {
        let parent_run_id = RunId::new();
        let child_run_id = RunId::new();
        let event = RuntimeEvent::SubAgentEvent {
            parent_run_id,
            child_run_id,
            event: Box::new(RuntimeEvent::RunCompleted {
                output: json!("done"),
                stop_reason: StopReason::EndTurn,
            }),
        };

        let serialized = serde_json::to_string(&event).expect("serialize event");
        let deserialized: RuntimeEvent =
            serde_json::from_str(&serialized).expect("deserialize event");

        match deserialized {
            RuntimeEvent::SubAgentEvent {
                parent_run_id: parent,
                child_run_id: child,
                event,
            } => {
                assert_eq!(parent, parent_run_id);
                assert_eq!(child, child_run_id);
                assert!(matches!(
                    event.as_ref(),
                    RuntimeEvent::RunCompleted { output, .. } if output == "done"
                ));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn run_completed_stop_reason_round_trips_through_serde() {
        let event = RuntimeEvent::RunCompleted {
            output: json!("cut off"),
            stop_reason: StopReason::MaxTokens,
        };

        let value = serde_json::to_value(&event).expect("serialize event");
        assert_eq!(
            value,
            json!({"RunCompleted": {"output": "cut off", "stop_reason": "MaxTokens"}})
        );
        let deserialized: RuntimeEvent = serde_json::from_value(value).expect("deserialize event");
        assert!(matches!(
            deserialized,
            RuntimeEvent::RunCompleted {
                stop_reason: StopReason::MaxTokens,
                ..
            }
        ));
    }

    #[test]
    fn run_completed_without_stop_reason_deserializes_as_end_turn() {
        // Events serialized before the stop_reason field existed carry only
        // `output`; they must still deserialize (serde default).
        let legacy = json!({"RunCompleted": {"output": "done"}});
        let event: RuntimeEvent = serde_json::from_value(legacy).expect("deserialize legacy event");

        match event {
            RuntimeEvent::RunCompleted {
                output,
                stop_reason,
            } => {
                assert_eq!(output, json!("done"));
                assert_eq!(stop_reason, StopReason::EndTurn);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn run_failed_kind_round_trips_through_serde() {
        let event = RuntimeEvent::RunFailed {
            error: "budget_exceeded: TokenLimit".to_string(),
            kind: RunFailureKind::BudgetExceeded,
        };

        let value = serde_json::to_value(&event).expect("serialize event");
        assert_eq!(
            value,
            json!({"RunFailed": {"error": "budget_exceeded: TokenLimit", "kind": "BudgetExceeded"}})
        );
        let deserialized: RuntimeEvent = serde_json::from_value(value).expect("deserialize event");
        assert!(matches!(
            deserialized,
            RuntimeEvent::RunFailed {
                kind: RunFailureKind::BudgetExceeded,
                ..
            }
        ));
    }

    #[test]
    fn run_failed_without_kind_deserializes_as_other() {
        // Events serialized before the kind field existed carry only `error`;
        // they must still deserialize (serde default).
        let legacy = json!({"RunFailed": {"error": "provider exploded"}});
        let event: RuntimeEvent = serde_json::from_value(legacy).expect("deserialize legacy event");

        match event {
            RuntimeEvent::RunFailed { error, kind } => {
                assert_eq!(error, "provider exploded");
                assert_eq!(kind, RunFailureKind::Other);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn events_dropped_without_seq_deserializes_as_zero() {
        let legacy = json!({"EventsDropped": {"subscriber_id": 2, "count": 4}});
        let event: RuntimeEvent =
            serde_json::from_value(legacy).expect("deserialize legacy EventsDropped");
        match event {
            RuntimeEvent::EventsDropped {
                subscriber_id,
                count,
                from_seq,
                to_seq,
            } => {
                assert_eq!(subscriber_id, 2);
                assert_eq!(count, 4);
                assert_eq!(from_seq, 0);
                assert_eq!(to_seq, 0);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn secondary_saturation_delivers_coalesced_loss_then_resumes() {
        let next_seq = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1));
        // Large primary capacity so mirror EventsDropped signals never block delivery.
        let (primary_tx, mut primary_rx) = mpsc::channel(64);
        // Capacity 2: leaves room to flush EventsDropped and the next event together
        // after the buffered prefixes are drained (capacity 1 would re-lose the
        // resumed event immediately after the loss signal).
        let (secondary_tx, mut secondary_rx) = mpsc::channel(2);
        let sinks = vec![
            EventSink::primary(primary_tx, std::sync::Arc::clone(&next_seq)),
            EventSink::secondary(secondary_tx, 1, std::sync::Arc::clone(&next_seq)),
        ];

        for step in 1..=5u32 {
            deliver_to_subscribers(&sinks, RuntimeEvent::ModelCallStarted { step }).await;
        }

        let mut primary_started = 0u32;
        let mut primary_drop_mirrors = 0u64;
        while let Ok(event) = primary_rx.try_recv() {
            match event {
                RuntimeEvent::ModelCallStarted { .. } => primary_started += 1,
                RuntimeEvent::EventsDropped {
                    subscriber_id: 1, ..
                } => primary_drop_mirrors += 1,
                other => panic!("unexpected primary event: {other:?}"),
            }
        }
        assert_eq!(primary_started, 5);
        assert!(primary_drop_mirrors >= 1);

        let first = secondary_rx.try_recv().expect("buffered event 1");
        let second = secondary_rx.try_recv().expect("buffered event 2");
        assert!(matches!(first, RuntimeEvent::ModelCallStarted { step: 1 }));
        assert!(matches!(second, RuntimeEvent::ModelCallStarted { step: 2 }));

        // Channel empty; pending loss still recorded. Next offer flushes the
        // coalesced signal then delivers the new event.
        deliver_to_subscribers(&sinks, RuntimeEvent::ModelCallStarted { step: 6 }).await;

        let loss = secondary_rx
            .try_recv()
            .expect("coalesced EventsDropped should flush before the resumed event");
        match loss {
            RuntimeEvent::EventsDropped {
                subscriber_id,
                count,
                from_seq,
                to_seq,
            } => {
                assert_eq!(subscriber_id, 1);
                assert!(
                    count >= 3,
                    "expected coalesced loss count >= 3, got {count}"
                );
                assert!(from_seq >= 1);
                assert!(to_seq >= from_seq);
            }
            other => panic!("expected EventsDropped, got {other:?}"),
        }

        let resumed = secondary_rx
            .try_recv()
            .expect("resumed event after loss signal");
        assert!(matches!(
            resumed,
            RuntimeEvent::ModelCallStarted { step: 6 }
        ));
    }

    #[tokio::test]
    async fn secondary_no_drop_preserves_fifo() {
        let next_seq = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1));
        let (primary_tx, mut primary_rx) = mpsc::channel(32);
        let (secondary_tx, mut secondary_rx) = mpsc::channel(32);
        let sinks = vec![
            EventSink::primary(primary_tx, std::sync::Arc::clone(&next_seq)),
            EventSink::secondary(secondary_tx, 1, next_seq),
        ];

        for step in 1..=8u32 {
            deliver_to_subscribers(&sinks, RuntimeEvent::ModelCallStarted { step }).await;
        }

        let mut primary_steps = Vec::new();
        while let Ok(RuntimeEvent::ModelCallStarted { step }) = primary_rx.try_recv() {
            primary_steps.push(step);
        }
        let mut secondary_steps = Vec::new();
        while let Ok(RuntimeEvent::ModelCallStarted { step }) = secondary_rx.try_recv() {
            secondary_steps.push(step);
        }
        assert_eq!(primary_steps, (1..=8).collect::<Vec<_>>());
        assert_eq!(secondary_steps, primary_steps);
        assert!(primary_rx.try_recv().is_err());
        assert!(secondary_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn pending_loss_state_stays_bounded_under_sustained_saturation() {
        let next_seq = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1));
        // Drain primary in the background so mirror signals never fill it and
        // trip the primary delivery timeout (which would make this test slow).
        let (primary_tx, mut primary_rx) = mpsc::channel(32);
        tokio::spawn(async move { while primary_rx.recv().await.is_some() {} });
        let (secondary_tx, secondary_rx) = mpsc::channel(1);
        let sinks = vec![
            EventSink::primary(primary_tx, std::sync::Arc::clone(&next_seq)),
            EventSink::secondary(secondary_tx, 1, next_seq),
        ];

        // Fill secondary, then lose many events without draining.
        deliver_to_subscribers(&sinks, RuntimeEvent::ModelCallStarted { step: 1 }).await;
        for step in 2..=200u32 {
            deliver_to_subscribers(&sinks, RuntimeEvent::ModelCallStarted { step }).await;
        }

        let pending = sinks[1]
            .pending
            .as_ref()
            .expect("secondary has pending slot")
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        // O(1) state: only count + bounds, not one record per lost event.
        assert_eq!(
            std::mem::size_of_val(&*pending),
            std::mem::size_of::<PendingLoss>()
        );
        assert!(pending.count >= 199);
        assert!(pending.from_seq > 0);
        assert!(pending.to_seq >= pending.from_seq);
        // Hold receiver so the channel stays alive through the asserts.
        drop(secondary_rx);
    }
}
