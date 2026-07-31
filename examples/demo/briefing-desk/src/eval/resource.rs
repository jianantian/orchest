//! Resource / token accounting for eval attempts.
//!
//! `gate_total_tokens` is exactly:
//! `input + output + audio_input + image_input + video_input`.
//! Reasoning, cache, and details are reported but never re-added.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::trajectory::TrajectoryEvent;

/// Summed token fields across all observed model calls for one attempt.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub audio_input_tokens: u64,
    pub image_input_tokens: u64,
    pub video_input_tokens: u64,
    /// Diagnostic only; never folded into gate total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

impl TokenTotals {
    /// Gate total: five fields only. Reasoning/cache/details stay out.
    pub fn gate_total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.audio_input_tokens)
            .saturating_add(self.image_input_tokens)
            .saturating_add(self.video_input_tokens)
    }

    pub fn add_usage_value(&mut self, usage: &Value) {
        self.input_tokens = self.input_tokens.saturating_add(u64_field(usage, "input_tokens"));
        self.output_tokens = self
            .output_tokens
            .saturating_add(u64_field(usage, "output_tokens"));
        self.reasoning_tokens = self
            .reasoning_tokens
            .saturating_add(u64_field(usage, "reasoning_tokens"));
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(u64_field(usage, "cache_read_tokens"));
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(u64_field(usage, "cache_write_tokens"));
        self.audio_input_tokens = self
            .audio_input_tokens
            .saturating_add(u64_field(usage, "audio_input_tokens"));
        self.image_input_tokens = self
            .image_input_tokens
            .saturating_add(u64_field(usage, "image_input_tokens"));
        self.video_input_tokens = self
            .video_input_tokens
            .saturating_add(u64_field(usage, "video_input_tokens"));
        if let Some(c) = usage.get("cost_usd").and_then(Value::as_f64) {
            self.cost_usd = Some(self.cost_usd.unwrap_or(0.0) + c);
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "reasoning_tokens": self.reasoning_tokens,
            "cache_read_tokens": self.cache_read_tokens,
            "cache_write_tokens": self.cache_write_tokens,
            "audio_input_tokens": self.audio_input_tokens,
            "image_input_tokens": self.image_input_tokens,
            "video_input_tokens": self.video_input_tokens,
            "gate_total_tokens": self.gate_total_tokens(),
            "cost_usd": self.cost_usd,
        })
    }
}

/// Coverage of known internal model calls that must report usage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCoverage {
    Complete,
    Incomplete,
}

impl ResourceCoverage {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceCoverage::Complete => "complete",
            ResourceCoverage::Incomplete => "incomplete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceReport {
    pub totals: TokenTotals,
    pub coverage: ResourceCoverage,
    pub model_calls: u64,
    pub vision_calls: u64,
    pub missing_usage_calls: u64,
    pub notes: Vec<String>,
}

impl ResourceReport {
    pub fn to_json(&self) -> Value {
        json!({
            "coverage": self.coverage.as_str(),
            "model_calls": self.model_calls,
            "vision_calls": self.vision_calls,
            "missing_usage_calls": self.missing_usage_calls,
            "notes": self.notes,
            "gate_total_tokens": self.totals.gate_total_tokens(),
        })
    }
}

/// Collect usage from sanitized trajectory events.
///
/// Counts:
/// - `model_call_completed` (parent + nested child wrappers already flattened
///   into trajectory lines with their own kind)
/// - `tool_call_completed` for `describe_image` whose details carry TokenUsage
///
/// Missing usage on a known model/vision call marks coverage incomplete.
pub fn collect_resources(events: &[TrajectoryEvent]) -> ResourceReport {
    let mut totals = TokenTotals::default();
    let mut model_calls = 0u64;
    let mut vision_calls = 0u64;
    let mut missing = 0u64;
    let mut notes = Vec::new();

    for ev in events {
        match ev.kind.as_str() {
            "model_call_completed" => {
                model_calls += 1;
                if let Some(usage) = ev.data.get("tokens").or_else(|| ev.data.get("usage")) {
                    if usage_has_any_tokens(usage) || usage.is_object() {
                        totals.add_usage_value(usage);
                    } else {
                        missing += 1;
                        notes.push(format!(
                            "model_call_completed seq={} missing usage",
                            ev.sequence
                        ));
                    }
                } else {
                    missing += 1;
                    notes.push(format!(
                        "model_call_completed seq={} missing tokens field",
                        ev.sequence
                    ));
                }
            }
            "tool_call_completed" => {
                let tool = ev
                    .data
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if tool == "describe_image" {
                    vision_calls += 1;
                    // Vision usage is event-only in tool details / output.
                    let usage = ev
                        .data
                        .get("details")
                        .cloned()
                        .or_else(|| {
                            ev.data
                                .get("output")
                                .and_then(|o| o.get("details").cloned())
                        })
                        .or_else(|| {
                            // Structured tool output may put usage at top-level details.
                            None
                        });
                    if let Some(usage) = usage {
                        if usage_looks_like_tokens(&usage) {
                            totals.add_usage_value(&usage);
                        } else if let Some(nested) =
                            usage.get("usage").or_else(|| usage.get("tokens"))
                        {
                            totals.add_usage_value(nested);
                        } else {
                            missing += 1;
                            notes.push(format!(
                                "describe_image seq={} details lack token usage",
                                ev.sequence
                            ));
                        }
                    } else {
                        missing += 1;
                        notes.push(format!(
                            "describe_image seq={} missing usage details",
                            ev.sequence
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    let coverage = if missing > 0 {
        ResourceCoverage::Incomplete
    } else {
        ResourceCoverage::Complete
    };

    ResourceReport {
        totals,
        coverage,
        model_calls,
        vision_calls,
        missing_usage_calls: missing,
        notes,
    }
}

fn u64_field(v: &Value, key: &str) -> u64 {
    v.get(key)
        .and_then(|x| {
            x.as_u64()
                .or_else(|| x.as_i64().map(|i| i.max(0) as u64))
                .or_else(|| x.as_f64().map(|f| f.max(0.0) as u64))
        })
        .unwrap_or(0)
}

fn usage_has_any_tokens(usage: &Value) -> bool {
    [
        "input_tokens",
        "output_tokens",
        "audio_input_tokens",
        "image_input_tokens",
        "video_input_tokens",
        "reasoning_tokens",
    ]
    .iter()
    .any(|k| u64_field(usage, k) > 0)
        || usage.is_object()
}

fn usage_looks_like_tokens(usage: &Value) -> bool {
    usage.get("input_tokens").is_some() || usage.get("output_tokens").is_some()
}

/// Equal-weight mean of gate totals across all attempts (validation policy).
pub fn mean_gate_tokens(gate_totals: &[u64]) -> Option<f64> {
    if gate_totals.is_empty() {
        None
    } else {
        let sum: u128 = gate_totals.iter().map(|t| *t as u128).sum();
        Some(sum as f64 / gate_totals.len() as f64)
    }
}

/// Median of wall latencies. Failed/inconclusive attempts must be excluded by
/// the caller. Even sample count → average of the two middle values.
pub fn median_latency_ms(latencies: &[u64]) -> Option<f64> {
    if latencies.is_empty() {
        return None;
    }
    let mut sorted = latencies.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    if n % 2 == 1 {
        Some(sorted[n / 2] as f64)
    } else {
        let a = sorted[n / 2 - 1] as f64;
        let b = sorted[n / 2] as f64;
        Some((a + b) / 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::trajectory::{RunRelation, TRAJECTORY_SCHEMA_VERSION};

    fn ev(kind: &str, seq: u64, data: Value) -> TrajectoryEvent {
        TrajectoryEvent {
            schema_version: TRAJECTORY_SCHEMA_VERSION.into(),
            sequence: seq,
            elapsed_ms: 0,
            run_relation: RunRelation::default(),
            kind: kind.into(),
            data,
        }
    }

    #[test]
    fn gate_total_excludes_reasoning_and_cache() {
        let mut t = TokenTotals {
            input_tokens: 100,
            output_tokens: 50,
            reasoning_tokens: 40, // diagnostic subset — not added
            cache_read_tokens: 10,
            cache_write_tokens: 5,
            audio_input_tokens: 3,
            image_input_tokens: 7,
            video_input_tokens: 2,
            cost_usd: None,
        };
        assert_eq!(t.gate_total_tokens(), 100 + 50 + 3 + 7 + 2);
        // Adding usage with reasoning again must not double-count gate total.
        t.add_usage_value(&json!({
            "input_tokens": 0,
            "output_tokens": 10,
            "reasoning_tokens": 8,
        }));
        assert_eq!(t.gate_total_tokens(), 100 + 60 + 3 + 7 + 2);
        assert_eq!(t.reasoning_tokens, 48);
    }

    #[test]
    fn collect_marks_missing_vision_usage_incomplete() {
        let events = vec![
            ev(
                "model_call_completed",
                0,
                json!({"tokens": {"input_tokens": 10, "output_tokens": 5}}),
            ),
            ev(
                "tool_call_completed",
                1,
                json!({"tool": "describe_image", "output": {"description": "chart"}}),
            ),
        ];
        let report = collect_resources(&events);
        assert_eq!(report.coverage, ResourceCoverage::Incomplete);
        assert_eq!(report.vision_calls, 1);
        assert_eq!(report.missing_usage_calls, 1);
        assert_eq!(report.totals.gate_total_tokens(), 15);
    }

    #[test]
    fn collect_vision_details_usage() {
        let events = vec![
            ev(
                "model_call_completed",
                0,
                json!({"tokens": {
                    "input_tokens": 10,
                    "output_tokens": 5,
                    "image_input_tokens": 0
                }}),
            ),
            ev(
                "tool_call_completed",
                1,
                json!({
                    "tool": "describe_image",
                    "details": {
                        "input_tokens": 20,
                        "output_tokens": 8,
                        "image_input_tokens": 100
                    }
                }),
            ),
        ];
        let report = collect_resources(&events);
        assert_eq!(report.coverage, ResourceCoverage::Complete);
        assert_eq!(report.totals.input_tokens, 30);
        assert_eq!(report.totals.output_tokens, 13);
        assert_eq!(report.totals.image_input_tokens, 100);
        assert_eq!(report.totals.gate_total_tokens(), 30 + 13 + 100);
    }

    #[test]
    fn median_even_average() {
        assert_eq!(median_latency_ms(&[10, 30, 20, 40]), Some(25.0));
        assert_eq!(median_latency_ms(&[10, 20, 30]), Some(20.0));
        assert_eq!(median_latency_ms(&[]), None);
    }

    #[test]
    fn mean_gate_equal_weight() {
        assert_eq!(mean_gate_tokens(&[100, 200, 300]), Some(200.0));
    }
}
