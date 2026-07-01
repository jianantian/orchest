//! Runtime telemetry: metrics counters and span helpers.

use std::time::Duration;

use tracing::Span;

use crate::budget::{BudgetConfig, BudgetUsage};
use crate::model::TokenUsage;

pub const METRIC_MODEL_REQUESTS_TOTAL: &str = "orchest_model_requests_total";
pub const METRIC_MODEL_REQUEST_DURATION_SECONDS: &str = "orchest_model_request_duration_seconds";
pub const METRIC_MODEL_TOKENS_TOTAL: &str = "orchest_model_tokens_total";
pub const METRIC_TOOL_CALL_DURATION: &str = "orchest_tool_duration_seconds";
pub const METRIC_TOOL_CALL_COUNT: &str = "orchest_tool_calls_total";
pub const METRIC_BUDGET_TOKENS_TOTAL: &str = "orchest_budget_tokens_total";
pub const METRIC_BUDGET_UTILIZATION_RATIO: &str = "orchest_budget_utilization_ratio";
pub const METRIC_BUDGET_EXCEEDED_TOTAL: &str = "orchest_budget_exceeded_total";
pub const METRIC_APPROVAL_REQUESTS_TOTAL: &str = "orchest_approval_requests_total";
pub const METRIC_APPROVAL_LATENCY_SECONDS: &str = "orchest_approval_latency_seconds";
pub const METRIC_CONTEXT_COMPACTIONS_TOTAL: &str = "orchest_context_compactions_total";
pub const METRIC_CONTEXT_COMPACTION_TOKEN_SAVINGS: &str =
    "orchest_context_compaction_token_savings";
pub const METRIC_EVENT_DROPS_TOTAL: &str = "orchest_event_drops_total";

pub fn tool_execute_span(tool: &str, source: &str) -> Span {
    tracing::info_span!(
        "tool.execute",
        tool_name = tool,
        tool_source = source,
        status = tracing::field::Empty,
    )
}

pub fn record_tool_success(_tool: &str, source: &str, duration: Duration) {
    metrics::histogram!(
        METRIC_TOOL_CALL_DURATION,
        "tool_source" => source.to_string(),
        "status" => "ok"
    )
    .record(duration.as_secs_f64());

    metrics::counter!(
        METRIC_TOOL_CALL_COUNT,
        "tool_source" => source.to_string(),
        "status" => "ok"
    )
    .increment(1);
}

pub fn record_tool_error(_tool: &str, source: &str, duration: Duration) {
    metrics::histogram!(
        METRIC_TOOL_CALL_DURATION,
        "tool_source" => source.to_string(),
        "status" => "error"
    )
    .record(duration.as_secs_f64());

    metrics::counter!(
        METRIC_TOOL_CALL_COUNT,
        "tool_source" => source.to_string(),
        "status" => "error"
    )
    .increment(1);
}

pub fn record_tool_timeout(_tool: &str, source: &str, duration: Duration) {
    metrics::histogram!(
        METRIC_TOOL_CALL_DURATION,
        "tool_source" => source.to_string(),
        "status" => "timeout"
    )
    .record(duration.as_secs_f64());

    metrics::counter!(
        METRIC_TOOL_CALL_COUNT,
        "tool_source" => source.to_string(),
        "status" => "timeout"
    )
    .increment(1);
}

pub fn model_complete_span(provider: &str, model: &str, streaming: bool) -> Span {
    tracing::info_span!(
        "model.complete",
        provider = provider,
        model = model,
        model_family = model_family(provider, model),
        streaming = streaming,
        status = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    )
}

pub fn record_model_success(provider: &str, model: &str, duration: Duration, usage: &TokenUsage) {
    record_model_request(provider, model, "ok", duration);
    record_model_tokens(provider, model, usage);
}

pub fn record_model_error(provider: &str, model: &str, duration: Duration) {
    record_model_request(provider, model, "error", duration);
}

pub fn record_budget_usage(usage: &BudgetUsage, config: &BudgetConfig) {
    metrics::counter!(METRIC_BUDGET_TOKENS_TOTAL, "kind" => "total").increment(usage.tokens_used);
    record_utilization(
        "tokens",
        usage.tokens_used as f64,
        config.max_tokens.map(|v| v as f64),
    );
    record_utilization(
        "tool_calls",
        usage.tool_calls_used as f64,
        config.max_tool_calls.map(|v| v as f64),
    );
    record_utilization("cost", usage.cost_usd, config.max_cost_usd);
}

pub fn record_budget_exceeded(kind: &str) {
    metrics::counter!(METRIC_BUDGET_EXCEEDED_TOTAL, "kind" => kind.to_string()).increment(1);
}

pub fn record_approval(status: &str, duration: Duration) {
    metrics::counter!(METRIC_APPROVAL_REQUESTS_TOTAL, "status" => status.to_string()).increment(1);
    metrics::histogram!(METRIC_APPROVAL_LATENCY_SECONDS, "status" => status.to_string())
        .record(duration.as_secs_f64());
}

pub fn record_context_compaction(removed_messages: usize, token_savings: u64) {
    metrics::counter!(METRIC_CONTEXT_COMPACTIONS_TOTAL).increment(1);
    metrics::histogram!(METRIC_CONTEXT_COMPACTION_TOKEN_SAVINGS).record(token_savings as f64);
    metrics::histogram!("orchest_context_compaction_removed_messages")
        .record(removed_messages as f64);
}

pub fn record_event_drop(subscriber: &str, count: u64) {
    metrics::counter!(METRIC_EVENT_DROPS_TOTAL, "subscriber" => subscriber.to_string())
        .increment(count);
}

fn record_model_request(provider: &str, model: &str, status: &str, duration: Duration) {
    let family = model_family(provider, model);
    metrics::counter!(
        METRIC_MODEL_REQUESTS_TOTAL,
        "provider" => provider.to_string(),
        "model_family" => family.clone(),
        "status" => status.to_string()
    )
    .increment(1);
    metrics::histogram!(
        METRIC_MODEL_REQUEST_DURATION_SECONDS,
        "provider" => provider.to_string(),
        "model_family" => family,
        "status" => status.to_string()
    )
    .record(duration.as_secs_f64());
}

fn record_model_tokens(provider: &str, model: &str, usage: &TokenUsage) {
    record_model_token_kind(provider, model, "input", usage.input_tokens);
    record_model_token_kind(provider, model, "output", usage.output_tokens);
    record_model_token_kind(provider, model, "reasoning", usage.reasoning_tokens);
    record_model_token_kind(provider, model, "cache_read", usage.cache_read_tokens);
    record_model_token_kind(provider, model, "cache_write", usage.cache_write_tokens);
}

fn record_model_token_kind(provider: &str, model: &str, kind: &str, count: u64) {
    if count == 0 {
        return;
    }
    metrics::counter!(
        METRIC_MODEL_TOKENS_TOTAL,
        "provider" => provider.to_string(),
        "model_family" => model_family(provider, model),
        "kind" => kind.to_string()
    )
    .increment(count);
}

fn record_utilization(kind: &str, used: f64, limit: Option<f64>) {
    let Some(limit) = limit else {
        return;
    };
    if limit <= 0.0 {
        return;
    }
    metrics::gauge!(METRIC_BUDGET_UTILIZATION_RATIO, "kind" => kind.to_string())
        .set((used / limit).clamp(0.0, f64::INFINITY));
}

fn model_family(provider: &str, model: &str) -> String {
    let candidate = if provider == "openrouter" {
        model.split('/').nth(1).unwrap_or(model)
    } else {
        model.rsplit('/').next().unwrap_or(model)
    };
    let lower = candidate.to_ascii_lowercase();
    if lower.starts_with("claude") {
        "claude".into()
    } else if lower.starts_with("gpt") || lower.starts_with("o1") || lower.starts_with("o3") {
        "gpt".into()
    } else if lower.starts_with("deepseek") {
        "deepseek".into()
    } else if lower.starts_with("doubao") {
        "doubao".into()
    } else if provider.is_empty() {
        "unknown".into()
    } else {
        provider.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use metrics_util::debugging::DebuggingRecorder;

    fn recorded_metric_names(recorder: &DebuggingRecorder) -> Vec<String> {
        recorder
            .snapshotter()
            .snapshot()
            .into_vec()
            .into_iter()
            .map(|(key, _, _, _)| key.key().name().to_string())
            .collect()
    }

    #[test]
    fn model_metrics_are_recorded() {
        let recorder = DebuggingRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            record_model_success(
                "openai",
                "gpt-4o",
                Duration::from_millis(25),
                &TokenUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    ..Default::default()
                },
            );
        });

        let names = recorded_metric_names(&recorder);
        assert!(names.contains(&METRIC_MODEL_REQUESTS_TOTAL.to_string()));
        assert!(names.contains(&METRIC_MODEL_REQUEST_DURATION_SECONDS.to_string()));
        assert!(names.contains(&METRIC_MODEL_TOKENS_TOTAL.to_string()));
    }

    #[test]
    fn approval_metrics_are_recorded() {
        let recorder = DebuggingRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            record_approval("granted", Duration::from_millis(10));
        });

        let names = recorded_metric_names(&recorder);
        assert!(names.contains(&METRIC_APPROVAL_REQUESTS_TOTAL.to_string()));
        assert!(names.contains(&METRIC_APPROVAL_LATENCY_SECONDS.to_string()));
    }

    #[test]
    fn compaction_metrics_are_recorded() {
        let recorder = DebuggingRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            record_context_compaction(4, 120);
        });

        let names = recorded_metric_names(&recorder);
        assert!(names.contains(&METRIC_CONTEXT_COMPACTIONS_TOTAL.to_string()));
        assert!(names.contains(&METRIC_CONTEXT_COMPACTION_TOKEN_SAVINGS.to_string()));
    }
}
