//! L0: shared telemetry primitives.
//!
//! The per-crate `observability.rs`/`telemetry.rs` carry modality-specific
//! metrics (ASR transcript rollbacks, TTS first-audio latency), but they share
//! the same skeleton: a trace id, a start instant, and latency milestones. This
//! module provides that skeleton so the modality builders compose it instead of
//! re-deriving timing/trace boilerplate (the modality structs converge onto this
//! as their crates migrate in Issues 005–007).

use std::time::Instant;

/// A monotonic latency timer with named milestones. The common core under the
/// asr/tts telemetry builders.
#[derive(Debug)]
pub struct LatencyTimer {
    started_at: Instant,
    first_event_at: Option<Instant>,
}

impl Default for LatencyTimer {
    fn default() -> Self {
        Self::start()
    }
}

impl LatencyTimer {
    /// Start timing now.
    pub fn start() -> Self {
        Self {
            started_at: Instant::now(),
            first_event_at: None,
        }
    }

    /// Record the first response event, if not already recorded. Returns the
    /// latency-to-first-event in milliseconds.
    pub fn mark_first_event(&mut self) -> u64 {
        let now = Instant::now();
        let at = *self.first_event_at.get_or_insert(now);
        at.duration_since(self.started_at).as_millis() as u64
    }

    /// Latency to the first event, if one was marked.
    pub fn first_event_ms(&self) -> Option<u64> {
        self.first_event_at
            .map(|at| at.duration_since(self.started_at).as_millis() as u64)
    }

    /// Total elapsed milliseconds since start.
    pub fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
}

/// Generate a fresh trace id (UUID-v4-shaped, no external uuid dep: 32 hex
/// chars). Entropy = wall clock + stack-address salt + a process-global
/// monotonic sequence. The sequence is what makes back-to-back calls on a
/// coarse clock (e.g. macOS's µs granularity) collide-free: even when the
/// nanos and the salt are identical, the sequence always differs. Stable
/// enough for correlation, not cryptographic.
pub fn new_trace_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // mix in the address of a stack local for a little intra-process entropy
    let local = 0u8;
    let salt = (&local as *const u8) as usize as u128;
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) as u128;
    format!("{:032x}", (nanos ^ salt).wrapping_add(seq))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_event_marked_once() {
        let mut t = LatencyTimer::start();
        let a = t.mark_first_event();
        let b = t.mark_first_event();
        assert_eq!(a, b, "first-event latency is sticky");
        assert!(t.first_event_ms().is_some());
    }

    #[test]
    fn trace_ids_are_distinct_hex() {
        let a = new_trace_id();
        let b = new_trace_id();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    /// The old nanos^salt generator could collide on back-to-back calls
    /// (coarse clock + identical stack address). The monotonic sequence must
    /// keep even a tight same-thread loop collision-free.
    #[test]
    fn trace_ids_are_unique_under_tight_loop() {
        let ids: std::collections::HashSet<String> = (0..10_000).map(|_| new_trace_id()).collect();
        assert_eq!(ids.len(), 10_000);
    }
}
