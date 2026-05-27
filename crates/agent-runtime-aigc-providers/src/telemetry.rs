pub const METRIC_PROVIDER_DURATION: &str = "aigc.provider.duration";
pub const METRIC_ASSET_PERSIST_DURATION: &str = "aigc.asset.persist.duration";
pub const METRIC_GENERATED_IMAGE_COUNT: &str = "aigc.generated.image_count";
pub const METRIC_PERSISTED_BYTES: &str = "aigc.asset.persisted_bytes";
pub const METRIC_ERROR_COUNT: &str = "aigc.error.count";

pub fn image_create_span(provider: &str, model: &str) -> tracing::Span {
    tracing::info_span!("aigc.image.create", provider, model)
}

pub fn provider_request_span(provider: &str, model: &str) -> tracing::Span {
    tracing::info_span!("aigc.provider.request", provider, model)
}

pub fn provider_poll_span(provider: &str, model: &str) -> tracing::Span {
    tracing::info_span!("aigc.provider.poll", provider, model)
}

pub fn asset_persist_span(store: &str) -> tracing::Span {
    tracing::info_span!("aigc.asset.persist", store)
}

pub fn signed_url_span(store: &str) -> tracing::Span {
    tracing::info_span!("aigc.asset.signed_url", store)
}
