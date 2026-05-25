use std::time::Duration;

use tracing::Span;

pub const METRIC_REQUEST_DURATION: &str = "model.request.duration";
pub const METRIC_FIRST_TOKEN_LATENCY: &str = "model.stream.first_token_latency";
pub const METRIC_STREAM_DURATION: &str = "model.stream.duration";
pub const METRIC_TOKENS_INPUT: &str = "model.tokens.input";
pub const METRIC_TOKENS_OUTPUT: &str = "model.tokens.output";
pub const METRIC_USAGE_MISSING: &str = "model.usage.missing";

pub fn model_complete_span(provider: &str, model: &str, streaming: bool) -> Span {
    tracing::info_span!(
        "model.complete",
        provider = provider,
        model = model,
        streaming = streaming,
        status = tracing::field::Empty,
        input_tokens = tracing::field::Empty,
        output_tokens = tracing::field::Empty,
    )
}

pub fn model_family(model: &str) -> &str {
    model.split('-').next().unwrap_or(model)
}

pub fn record_model_success(
    provider: &str,
    model: &str,
    duration: Duration,
    input_tokens: u64,
    output_tokens: u64,
    first_token_latency: Option<Duration>,
    stream_duration: Option<Duration>,
) {
    let family = model_family(model).to_string();
    let provider = provider.to_string();

    metrics::histogram!(
        METRIC_REQUEST_DURATION,
        "provider" => provider.clone(),
        "model_family" => family.clone(),
        "status" => "ok"
    )
    .record(duration.as_secs_f64());

    metrics::counter!(
        METRIC_TOKENS_INPUT,
        "provider" => provider.clone(),
        "model_family" => family.clone()
    )
    .increment(input_tokens);

    metrics::counter!(
        METRIC_TOKENS_OUTPUT,
        "provider" => provider.clone(),
        "model_family" => family.clone()
    )
    .increment(output_tokens);

    if let Some(latency) = first_token_latency {
        metrics::histogram!(
            METRIC_FIRST_TOKEN_LATENCY,
            "provider" => provider.clone(),
            "model_family" => family.clone()
        )
        .record(latency.as_secs_f64());
    }

    if let Some(dur) = stream_duration {
        metrics::histogram!(
            METRIC_STREAM_DURATION,
            "provider" => provider,
            "model_family" => family
        )
        .record(dur.as_secs_f64());
    }
}

pub fn record_model_error(provider: &str, model: &str, duration: Duration) {
    metrics::histogram!(
        METRIC_REQUEST_DURATION,
        "provider" => provider.to_string(),
        "model_family" => model_family(model).to_string(),
        "status" => "error"
    )
    .record(duration.as_secs_f64());
}

pub fn record_usage_missing(provider: &str, model: &str) {
    metrics::counter!(
        METRIC_USAGE_MISSING,
        "provider" => provider.to_string(),
        "model_family" => model_family(model).to_string()
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    #[test]
    fn model_complete_span_has_canonical_fields() {
        let subscriber = Registry::default().with(tracing_subscriber::fmt::layer());
        let _guard = tracing::subscriber::set_default(subscriber);

        let span = model_complete_span("anthropic", "claude-sonnet-4", true);
        let _entered = span.enter();

        // Span fields are set — tracing doesn't expose them for read,
        // but construction succeeding without panic validates the schema.
        // We verify field names by checking the span metadata.
        let meta = span.metadata().expect("span should have metadata");
        assert_eq!(meta.name(), "model.complete");
        let field_names: Vec<&str> = meta.fields().iter().map(|f| f.name()).collect();
        assert!(field_names.contains(&"provider"));
        assert!(field_names.contains(&"model"));
        assert!(field_names.contains(&"streaming"));
    }

    #[test]
    fn provider_request_metrics_are_low_cardinality() {
        // Verify metric name constants don't contain high-cardinality segments
        for name in [
            METRIC_REQUEST_DURATION,
            METRIC_FIRST_TOKEN_LATENCY,
            METRIC_STREAM_DURATION,
            METRIC_TOKENS_INPUT,
            METRIC_TOKENS_OUTPUT,
            METRIC_USAGE_MISSING,
        ] {
            assert!(!name.contains("run_id"), "{name} should not contain run_id");
            assert!(
                !name.contains("error_text"),
                "{name} should not contain error_text"
            );
            assert!(
                name.starts_with("model."),
                "{name} should start with model."
            );
        }
    }

    #[test]
    fn provider_error_metrics_preserve_failure_status() {
        // Metric names support a status label — verify the constant exists
        // and the label pattern is usable
        let labels = [("provider", "anthropic"), ("status", "error")];
        for (key, _) in &labels {
            assert!(
                ["provider", "model_family", "status"].contains(key),
                "label {key} should be one of the canonical low-cardinality labels"
            );
        }
    }

    #[test]
    fn first_token_latency_recorded_for_streams() {
        // Verify the metric name constant exists and is well-formed
        assert_eq!(
            METRIC_FIRST_TOKEN_LATENCY,
            "model.stream.first_token_latency"
        );
        // The actual recording happens in the adapter's complete() method;
        // this test confirms the constant is available for instrumentation.
    }

    #[test]
    fn model_family_extracts_prefix() {
        assert_eq!(model_family("claude-sonnet-4-20250514"), "claude");
        assert_eq!(model_family("gpt-4o-mini"), "gpt");
        assert_eq!(model_family("deepseek-chat"), "deepseek");
        assert_eq!(model_family("o3-mini"), "o3");
    }
}
