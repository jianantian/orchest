//! Lossless delivery of a run's event stream to its host.
//!
//! Every event the run, its supervisor and its hooks emit for the primary
//! receiver passes through one relay task, in order. The relay never drops an
//! event: when the host's channel (capacity
//! [`RuntimeConfig::event_channel_capacity`](crate::run::RuntimeConfig::event_channel_capacity))
//! is full it waits, and the wait propagates back to the run as backpressure.
//! [`EventDeliveryStats`] (from
//! [`RunHandle::event_delivery_stats`](crate::run::RunHandle::event_delivery_stats))
//! and the `orchest_event_backpressure_*` metrics show when the host is not
//! keeping up. A host that must never slow the run down subscribes a
//! secondary receiver instead, which is lossy and reports losses as
//! `RuntimeEvent::EventsDropped`.
//!
//! A [`DurableEventSink`] receives the same stream before the host does. The
//! relay appends every event, flushes after ledger and terminal events and
//! when the stream ends, and only then forwards them, so a host that has seen
//! an event's terminal marker can rely on the sink holding everything up to
//! it. The sink records the observation stream; the run's execution state is
//! the session log, which has its own flush.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::events::RuntimeEvent;

/// Default capacity of the host's event channel. A streamed answer of about a
/// thousand deltas fits without making the model wait on the host.
pub const DEFAULT_EVENT_CHANNEL_CAPACITY: usize = 1024;

/// Capacity of the channel between the run and its relay. Small on purpose:
/// the host-facing channel is the buffer.
pub(crate) const RELAY_CHANNEL_CAPACITY: usize = 64;

/// Host storage for the event stream: appends in order, flushes on request.
#[async_trait]
pub trait DurableEventSink: Send + Sync {
    /// Appends `event`, the `seq`-th event of the run (from 1, without gaps).
    async fn append(&self, seq: u64, event: &RuntimeEvent) -> Result<(), String>;
    /// Makes every appended event durable.
    async fn flush(&self) -> Result<(), String>;
}

/// How delivery to the host has gone so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EventDeliveryStats {
    /// Capacity of the host's channel.
    pub capacity: usize,
    /// Events delivered to the host.
    pub delivered: u64,
    /// Deliveries that found the channel full and waited.
    pub backpressure_waits: u64,
    /// The longest of those waits.
    pub max_wait: Duration,
    /// All those waits together.
    pub total_wait: Duration,
    /// The most events the channel held at once.
    pub high_watermark: usize,
    /// Failed appends or flushes of the durable sink.
    pub sink_errors: u64,
}

pub(crate) type SharedDeliveryStats = Arc<Mutex<EventDeliveryStats>>;

/// Spawns the relay from `rx` to a new host channel of `capacity` and
/// returns the host's receiver.
pub(crate) fn spawn_relay(
    rx: mpsc::Receiver<RuntimeEvent>,
    capacity: usize,
    sink: Option<Arc<dyn DurableEventSink>>,
) -> (mpsc::Receiver<RuntimeEvent>, SharedDeliveryStats) {
    let capacity = capacity.max(1);
    let (tx, host_rx) = mpsc::channel(capacity);
    let stats = Arc::new(Mutex::new(EventDeliveryStats {
        capacity,
        ..EventDeliveryStats::default()
    }));
    tokio::spawn(relay(rx, tx, sink, Arc::clone(&stats)));
    (host_rx, stats)
}

async fn relay(
    mut rx: mpsc::Receiver<RuntimeEvent>,
    tx: mpsc::Sender<RuntimeEvent>,
    sink: Option<Arc<dyn DurableEventSink>>,
    stats: SharedDeliveryStats,
) {
    let mut seq = 0;
    let mut host_open = true;
    while let Some(event) = rx.recv().await {
        seq += 1;
        if let Some(sink) = &sink {
            let appended = sink.append(seq, &event).await;
            let flushed = if needs_flush(&event) {
                sink.flush().await
            } else {
                Ok(())
            };
            if let Err(error) = appended.and(flushed) {
                record_sink_error(&stats, &error);
            }
        }
        if host_open {
            host_open = deliver(&tx, event, &stats).await;
        }
    }
    if let Some(sink) = &sink {
        if let Err(error) = sink.flush().await {
            record_sink_error(&stats, &error);
        }
    }
}

/// Ledger and terminal events correspond to session log checkpoints; the
/// sink is flushed at the same boundaries.
fn needs_flush(event: &RuntimeEvent) -> bool {
    matches!(
        event,
        RuntimeEvent::ToolDispatched { .. }
            | RuntimeEvent::RunCompleted { .. }
            | RuntimeEvent::RunFailed { .. }
            | RuntimeEvent::RunAborted { .. }
    )
}

/// Delivers one event, waiting for capacity. Returns `false` once the host
/// dropped its receiver.
async fn deliver(
    tx: &mpsc::Sender<RuntimeEvent>,
    event: RuntimeEvent,
    stats: &SharedDeliveryStats,
) -> bool {
    let event = match tx.try_send(event) {
        Ok(()) => {
            record_delivery(tx, stats, None);
            return true;
        }
        Err(mpsc::error::TrySendError::Closed(_)) => return false,
        Err(mpsc::error::TrySendError::Full(event)) => event,
    };
    let started = Instant::now();
    if tx.send(event).await.is_err() {
        return false;
    }
    let waited = started.elapsed();
    crate::telemetry::record_event_backpressure(waited);
    record_delivery(tx, stats, Some(waited));
    true
}

fn record_delivery(
    tx: &mpsc::Sender<RuntimeEvent>,
    stats: &SharedDeliveryStats,
    waited: Option<Duration>,
) {
    let occupancy = tx.max_capacity() - tx.capacity();
    crate::telemetry::record_event_channel_occupancy(occupancy);
    let mut stats = stats
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    stats.delivered += 1;
    stats.high_watermark = stats.high_watermark.max(occupancy);
    if let Some(waited) = waited {
        stats.backpressure_waits += 1;
        stats.max_wait = stats.max_wait.max(waited);
        stats.total_wait += waited;
    }
}

fn record_sink_error(stats: &SharedDeliveryStats, error: &str) {
    tracing::warn!(error, "durable event sink failed");
    stats
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .sink_errors += 1;
}
