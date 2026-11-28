//! Shared run lifecycle for SDK bindings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::{watch, Mutex as AsyncMutex};
use tokio_util::sync::CancellationToken;

use serde_json::Value;

use crate::capabilities::Capabilities;
use crate::events::RuntimeEvent;
use crate::run::{Cancellation, EventReceiver, RequestError, RequestId, RunHandle};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BindingRunError {
    #[error("the agent already has an active run")]
    Active,
    #[error("consume the event stream to completion before waiting for the result")]
    EventsPending,
    #[error("the event stream was closed before it was exhausted")]
    StreamClosed,
    #[error("the run ended without a terminal result")]
    MissingResult,
    #[error("{0}")]
    InvalidResponse(String),
    #[error(transparent)]
    RequestNotFound(#[from] RequestError),
}

impl BindingRunError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Active => "run_active",
            Self::EventsPending => "run_events_pending",
            Self::StreamClosed => "run_stream_closed",
            Self::MissingResult => "run_missing_result",
            Self::InvalidResponse(_) => "invalid_response",
            Self::RequestNotFound(_) => "request_not_found",
        }
    }
}

#[derive(Default)]
enum SlotState {
    #[default]
    Idle,
    Starting,
    Running(Arc<RunHandle>),
}

/// A single Agent instance's shared startup reservation and active controls.
#[derive(Clone, Default)]
pub struct BindingRunSlot(Arc<Mutex<SlotState>>);

impl BindingRunSlot {
    fn state(&self) -> MutexGuard<'_, SlotState> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn reserve(&self) -> Result<BindingRunReservation, BindingRunError> {
        let mut state = self.state();
        if !matches!(*state, SlotState::Idle) {
            return Err(BindingRunError::Active);
        }
        *state = SlotState::Starting;
        Ok(BindingRunReservation(self.clone()))
    }

    pub fn current(&self) -> Option<Arc<RunHandle>> {
        match &*self.state() {
            SlotState::Running(handle) => Some(Arc::clone(handle)),
            _ => None,
        }
    }
}

/// Releases the Agent slot on startup failure or when the core run has joined.
pub struct BindingRunReservation(BindingRunSlot);

impl BindingRunReservation {
    /// Call on the runtime that owns the new run.
    pub fn attach(self, handle: RunHandle) -> BindingRun {
        let handle = Arc::new(handle);
        *self.0.state() = SlotState::Running(Arc::clone(&handle));
        let (done, completion) = watch::channel(());
        let active = Arc::clone(&handle);
        tokio::spawn(async move {
            active.wait_for_task().await;
            drop(self);
            drop(done);
        });
        BindingRun { handle, completion }
    }
}

impl Drop for BindingRunReservation {
    fn drop(&mut self) {
        *self.0.state() = SlotState::Idle;
    }
}

pub struct BindingRun {
    handle: Arc<RunHandle>,
    completion: watch::Receiver<()>,
}

impl BindingRun {
    pub fn cancel(&self, cancellation: Cancellation) -> bool {
        self.completion.has_changed().is_ok() && self.handle.cancel(cancellation)
    }

    pub async fn join(&self) {
        let _ = self.completion.clone().changed().await;
    }
}

pub struct BindingRunStream {
    run: BindingRun,
    reading: AsyncMutex<Option<EventReceiver>>,
    closing: CancellationToken,
    exhausted: AtomicBool,
    terminal: Mutex<Option<RuntimeEvent>>,
}

impl BindingRunStream {
    pub fn new(run: BindingRun, events: EventReceiver) -> Self {
        Self {
            run,
            reading: AsyncMutex::new(Some(events)),
            closing: CancellationToken::new(),
            exhausted: AtomicBool::new(false),
            terminal: Mutex::new(None),
        }
    }

    pub async fn next(&self) -> Option<RuntimeEvent> {
        let mut reading = self.reading.lock().await;
        let receiver = reading.as_mut()?;
        let event = tokio::select! {
            biased;
            () = self.closing.cancelled() => {
                reading.take();
                return None;
            }
            event = receiver.recv() => event,
        };
        match &event {
            Some(RuntimeEvent::RunRestarted { .. }) => *self.terminal() = None,
            Some(
                event @ (RuntimeEvent::RunCompleted { .. }
                | RuntimeEvent::RunFailed { .. }
                | RuntimeEvent::RunAborted { .. }),
            ) => *self.terminal() = Some(event.clone()),
            Some(_) => {}
            None => {
                reading.take();
                self.exhausted.store(true, Ordering::Release);
            }
        }
        event
    }

    pub async fn wait(&self) -> Result<RuntimeEvent, BindingRunError> {
        if !self.exhausted.load(Ordering::Acquire) {
            return Err(if self.closing.is_cancelled() {
                BindingRunError::StreamClosed
            } else {
                BindingRunError::EventsPending
            });
        }
        self.run.join().await;
        self.terminal()
            .clone()
            .ok_or(BindingRunError::MissingResult)
    }

    pub fn cancel(&self, cancellation: Cancellation) -> bool {
        self.run.cancel(cancellation)
    }

    pub async fn respond(&self, request_id: String, payload: Value) -> Result<(), BindingRunError> {
        let response =
            super::parse_binding_response(payload).map_err(BindingRunError::InvalidResponse)?;
        self.run
            .handle
            .respond(&RequestId::from(request_id), response)
            .await?;
        Ok(())
    }

    pub fn capabilities(&self) -> &Capabilities {
        self.run.handle.capabilities()
    }

    pub fn stop_reading(&self) {
        if !self.closing.is_cancelled() {
            if !self.exhausted.load(Ordering::Acquire) {
                self.cancel(Cancellation::user());
            }
            self.closing.cancel();
        }
        if let Ok(mut reading) = self.reading.try_lock() {
            reading.take();
        }
    }

    pub async fn close(&self) {
        self.stop_reading();
        self.reading.lock().await.take();
        self.run.join().await;
    }

    fn terminal(&self) -> MutexGuard<'_, Option<RuntimeEvent>> {
        self.terminal
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}
