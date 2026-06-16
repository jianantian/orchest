//! Gateways wrap a provider adapter with the cross-cutting concern every
//! provider needs but shouldn't implement itself: persisting the provider's
//! (often short-lived) asset URL to our own storage before handing back a
//! controlled URL. See [`image::ImageGateway`] and [`video::VideoGateway`].

pub mod image;
pub mod video;

pub use image::{ImageGateway, ImageGatewayConfig};
pub use video::{VideoGateway, VideoGatewayConfig};

use crate::{
    AigcError, AssetIngestSource, AssetRegistry, AssetScope, AssetStore, ProviderGenerationStatus,
    PutAssetOptions, StoredAsset,
};

/// Bundles the object-storage handles every gateway needs to persist a
/// provider asset: where to write it, where to register it, under which
/// scope, and which provider's name to tag metrics with. Exists so
/// `persist_provider_asset` — the single shared implementation of "accept a
/// provider asset and put it in our own storage" — takes one argument
/// instead of four.
struct AssetStorageContext<'a> {
    asset_store: &'a dyn AssetStore,
    asset_registry: &'a dyn AssetRegistry,
    scope: &'a AssetScope,
    provider_name: &'a str,
}

/// Persists a provider-supplied asset (image or video) to our own asset
/// store and registers it, recording the same telemetry regardless of media
/// type. Both `ImageGateway` and `VideoGateway` call into this so there is
/// exactly one implementation of object-storage semantics (key prefixes,
/// registry scoping, persisted-bytes/duration metrics) for the whole crate.
async fn persist_provider_asset(
    ctx: AssetStorageContext<'_>,
    source: AssetIngestSource,
    options: PutAssetOptions,
) -> Result<StoredAsset, AigcError> {
    let persist_started_at = std::time::Instant::now();
    let stored = ctx.asset_store.put_stream(source, options).await?;
    metrics::histogram!(
        crate::telemetry::METRIC_ASSET_PERSIST_DURATION,
        "provider" => ctx.provider_name.to_string()
    )
    .record(persist_started_at.elapsed().as_secs_f64());
    metrics::counter!(
        crate::telemetry::METRIC_PERSISTED_BYTES,
        "provider" => ctx.provider_name.to_string()
    )
    .increment(stored.byte_count);
    ctx.asset_registry.save(ctx.scope, stored.clone()).await?;
    Ok(stored)
}

/// Maps a provider's async-job status onto the gateway's public-facing
/// status. Shared by both gateways since `ProviderGenerationStatus` has no
/// media-specific variants.
pub(super) fn provider_status_to_public(
    status: ProviderGenerationStatus,
) -> crate::GenerationStatus {
    match status {
        ProviderGenerationStatus::Queued => crate::GenerationStatus::Queued,
        ProviderGenerationStatus::Running => crate::GenerationStatus::Running,
        ProviderGenerationStatus::Completed => crate::GenerationStatus::Completed,
        ProviderGenerationStatus::Failed => crate::GenerationStatus::Failed,
        ProviderGenerationStatus::TimedOut => crate::GenerationStatus::TimedOut,
    }
}

pub fn mock_provider_asset(data: &'static [u8]) -> crate::ProviderAsset {
    crate::ProviderAsset {
        source: AssetIngestSource::Bytes {
            bytes: bytes::Bytes::from_static(data),
            mime_type: "image/png".into(),
        },
        mime_type: Some("image/png".into()),
        width: Some(1),
        height: Some(1),
        expires_at: None,
        metadata: serde_json::json!({ "mock": true, "id": uuid::Uuid::new_v4().to_string() }),
    }
}
