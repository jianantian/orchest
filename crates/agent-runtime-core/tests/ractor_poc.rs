//! Ractor PoC — validates V1–V4 integration criteria for issue #81.
//!
//! V1: self-message step pattern (mode B)
//! V2: Cancel preempts queued RunStep messages
//! V3: actor panic surfaces through JoinHandle
//! V4: typed AgentRef API wraps raw ActorRef
//!
//! Ractor 0.15 uses RPITIT (not async_trait). In non-cluster builds, Message
//! is blanket-implemented for all `Any + Send + Sized + 'static` — do NOT add
//! manual `impl ractor::Message` impls.

use std::time::Duration;

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use tokio::sync::mpsc;

// ── Message types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct SteerCmd(String);

#[derive(Debug, Clone)]
struct SteerResult(String);

enum WorkerMsg {
    RunStep,
    Steer(SteerCmd, RpcReplyPort<SteerResult>),
    Cancel,
}

// ── Worker state ──────────────────────────────────────────────────────────────

struct WorkerState {
    steps_done: u32,
    max_steps: u32,
    cancelled: bool,
    event_tx: mpsc::Sender<String>,
}

// ── WorkerAgent ───────────────────────────────────────────────────────────────

struct WorkerAgent;

impl Actor for WorkerAgent {
    type Msg = WorkerMsg;
    type State = WorkerState;
    type Arguments = (mpsc::Sender<String>, u32); // (event_tx, max_steps)

    async fn pre_start(
        &self,
        myself: ActorRef<WorkerMsg>,
        (tx, max_steps): (mpsc::Sender<String>, u32),
    ) -> Result<WorkerState, ActorProcessingErr> {
        myself.cast(WorkerMsg::RunStep)?;
        Ok(WorkerState {
            steps_done: 0,
            max_steps,
            cancelled: false,
            event_tx: tx,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<WorkerMsg>,
        msg: WorkerMsg,
        state: &mut WorkerState,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            WorkerMsg::RunStep => {
                if state.cancelled || state.steps_done >= state.max_steps {
                    myself.stop(None);
                    return Ok(());
                }
                // simulate LLM latency
                tokio::time::sleep(Duration::from_millis(10)).await;
                let _ = state
                    .event_tx
                    .send(format!("step:{}", state.steps_done))
                    .await;
                state.steps_done += 1;
                myself.cast(WorkerMsg::RunStep)?;
            }
            WorkerMsg::Steer(cmd, reply) => {
                let _ = state.event_tx.send(format!("steered:{}", cmd.0)).await;
                let _ = reply.send(SteerResult(format!("ack:{}", cmd.0)));
            }
            WorkerMsg::Cancel => {
                state.cancelled = true;
                let _ = state.event_tx.send("cancelled".to_string()).await;
                myself.stop(None);
            }
        }
        Ok(())
    }
}

// ── Typed AgentRef (V4) ───────────────────────────────────────────────────────

struct AgentRef {
    inner: ActorRef<WorkerMsg>,
}

impl AgentRef {
    fn cancel(&self) {
        let _ = self.inner.cast(WorkerMsg::Cancel);
    }

    async fn steer(&self, cmd: SteerCmd) -> Result<SteerResult, String> {
        // call!(actor, Variant, args...) expands to actor.call(|tx| Variant(args, tx), None)
        ractor::call!(self.inner, WorkerMsg::Steer, cmd).map_err(|e| e.to_string())
    }
}

// ── Panicking actor (V3) ──────────────────────────────────────────────────────

struct PanickingActor;

enum PanicMsg {
    Go,
}

impl Actor for PanickingActor {
    type Msg = PanicMsg;
    type State = ();
    type Arguments = ();

    async fn pre_start(&self, _: ActorRef<PanicMsg>, _: ()) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        _: ActorRef<PanicMsg>,
        _: PanicMsg,
        _: &mut (),
    ) -> Result<(), ActorProcessingErr> {
        panic!("simulated panic for V3");
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// V1: mode-B self-message step pattern.
/// WorkerAgent self-sends RunStep from pre_start; each handle() emits one event
/// and schedules the next, running exactly max_steps iterations.
#[tokio::test]
async fn v1_self_message_step_pattern() {
    let (tx, mut rx) = mpsc::channel(64);
    let (_actor_ref, handle) = Actor::spawn(None, WorkerAgent, (tx, 5)).await.unwrap();
    let _ = handle.await;

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    let step_count = events.iter().filter(|e| e.starts_with("step:")).count();
    assert_eq!(step_count, 5, "expected exactly 5 steps, got {step_count}");
}

/// V2: Cancel preempts queued RunStep messages.
/// With max_steps=50 and ~10ms per step (~500ms total), sending Cancel after
/// 50ms stops the actor before all 50 steps complete.
#[tokio::test]
async fn v2_cancel_priority() {
    let (tx, mut rx) = mpsc::channel(256);
    let (actor_ref, handle) = Actor::spawn(None, WorkerAgent, (tx, 50)).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = actor_ref.cast(WorkerMsg::Cancel);

    tokio::time::timeout(Duration::from_secs(3), handle)
        .await
        .expect("actor did not stop within 3s after Cancel")
        .expect("actor join failed");

    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    let steps = events.iter().filter(|e| e.starts_with("step:")).count();

    assert!(
        events.contains(&"cancelled".to_string()),
        "Cancel event was never emitted"
    );
    assert!(
        steps < 50,
        "Cancel did not preempt RunStep queue (steps={steps})"
    );
}

/// V3: actor panic surfaces through the JoinHandle — actor terminates, not hangs.
#[tokio::test]
async fn v3_supervision_panic_surfaces() {
    let (actor_ref, handle) = Actor::spawn(None, PanickingActor, ()).await.unwrap();
    let _ = actor_ref.cast(PanicMsg::Go);

    let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(
        result.is_ok(),
        "actor did not terminate after panic within 2s"
    );
}

/// V4: typed AgentRef wraps ActorRef with ergonomic steer/cancel methods.
#[tokio::test]
async fn v4_typed_agent_ref() {
    let (tx, mut rx) = mpsc::channel(64);
    let (actor_ref, handle) = Actor::spawn(None, WorkerAgent, (tx, 20)).await.unwrap();
    let agent_ref = AgentRef { inner: actor_ref };

    let result = agent_ref.steer(SteerCmd("redirect".to_string())).await;
    assert!(result.is_ok(), "steer failed: {:?}", result.err());
    assert_eq!(result.unwrap().0, "ack:redirect");

    agent_ref.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        events.iter().any(|e| e.starts_with("steered:")),
        "steer event not emitted"
    );
    assert!(
        events.contains(&"cancelled".to_string()),
        "cancel event not emitted"
    );
}
