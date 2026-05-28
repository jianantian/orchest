//! Runtime telemetry: metrics counters and span helpers.

use std::time::Duration;

use tracing::Span;

pub const METRIC_TOOL_CALL_DURATION: &str = "tool.call.duration";
pub const METRIC_TOOL_CALL_COUNT: &str = "tool.call.count";

pub fn tool_execute_span(tool: &str, source: &str) -> Span {
    tracing::info_span!(
        "tool.execute",
        tool = tool,
        source = source,
        status = tracing::field::Empty,
    )
}

pub fn record_tool_success(tool: &str, source: &str, duration: Duration) {
    metrics::histogram!(
        METRIC_TOOL_CALL_DURATION,
        "tool" => tool.to_string(),
        "source" => source.to_string(),
        "status" => "ok"
    )
    .record(duration.as_secs_f64());

    metrics::counter!(
        METRIC_TOOL_CALL_COUNT,
        "tool" => tool.to_string(),
        "source" => source.to_string(),
        "status" => "ok"
    )
    .increment(1);
}

pub fn record_tool_error(tool: &str, source: &str, duration: Duration) {
    metrics::histogram!(
        METRIC_TOOL_CALL_DURATION,
        "tool" => tool.to_string(),
        "source" => source.to_string(),
        "status" => "error"
    )
    .record(duration.as_secs_f64());

    metrics::counter!(
        METRIC_TOOL_CALL_COUNT,
        "tool" => tool.to_string(),
        "source" => source.to_string(),
        "status" => "error"
    )
    .increment(1);
}

pub fn record_tool_timeout(tool: &str, source: &str, duration: Duration) {
    metrics::histogram!(
        METRIC_TOOL_CALL_DURATION,
        "tool" => tool.to_string(),
        "source" => source.to_string(),
        "status" => "timeout"
    )
    .record(duration.as_secs_f64());

    metrics::counter!(
        METRIC_TOOL_CALL_COUNT,
        "tool" => tool.to_string(),
        "source" => source.to_string(),
        "status" => "timeout"
    )
    .increment(1);
}
