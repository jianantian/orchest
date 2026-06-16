//! `NoopAssetStore` — accepts and hashes input but cannot produce URLs or read
//! bytes back. Useful as a placeholder when no real storage is configured.

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use uuid::Uuid;

use super::{
    materialize_input, sha256_hex, AssetAccessUrl, AssetIngestSource, AssetStore, AssetStoreError,
    PutAssetOptions, StorageLocation, StoredAsset,
};

pub struct NoopAssetStore;

#[async_trait]
impl AssetStore for NoopAssetStore {
    async fn put_stream(
        &self,
        input: AssetIngestSource,
        options: PutAssetOptions,
    ) -> Result<StoredAsset, AssetStoreError> {
        let (bytes, content_type) = materialize_input(input, &options).await?;
        let sha256 = sha256_hex(&bytes);
        Ok(StoredAsset {
            asset_id: format!("asset_{}", Uuid::new_v4()),
            location: StorageLocation::Noop,
            content_type,
            sha256,
            byte_count: bytes.len() as u64,
            created_at: Utc::now(),
            expires_at: None,
        })
    }

    async fn signed_url(
        &self,
        _asset: &StoredAsset,
        _ttl: Option<Duration>,
    ) -> Result<AssetAccessUrl, AssetStoreError> {
        Err(AssetStoreError::new(
            "url_unavailable",
            "NoopAssetStore cannot produce usable URLs",
        ))
    }

    async fn get_bytes(&self, _asset: &StoredAsset) -> Result<Bytes, AssetStoreError> {
        Err(AssetStoreError::new(
            "bytes_unavailable",
            "NoopAssetStore cannot read asset bytes",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[tokio::test]
    async fn base64_limit_is_enforced() {
        let store = NoopAssetStore;
        let err = store
            .put_stream(
                AssetIngestSource::Base64 {
                    data: base64::engine::general_purpose::STANDARD.encode("hello"),
                    mime_type: "image/png".into(),
                },
                PutAssetOptions {
                    max_base64_bytes: Some(2),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, "base64_too_large");
    }
}
