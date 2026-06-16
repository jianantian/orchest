//! `LocalAssetStore` — writes to the local filesystem. Used for tests and
//! local-dev deployments that don't need real object storage.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::{
    extension_for_content_type, materialize_input, sha256_hex, AssetAccessUrl, AssetIngestSource,
    AssetStore, AssetStoreError, LocalObjectLocation, PutAssetOptions, StorageLocation,
    StoredAsset,
};

pub struct LocalAssetStore {
    root: PathBuf,
    public_base_url: Option<String>,
}

impl LocalAssetStore {
    pub fn new(root: impl Into<PathBuf>, public_base_url: Option<String>) -> Self {
        Self {
            root: root.into(),
            public_base_url,
        }
    }
}

#[async_trait]
impl AssetStore for LocalAssetStore {
    async fn put_stream(
        &self,
        input: AssetIngestSource,
        options: PutAssetOptions,
    ) -> Result<StoredAsset, AssetStoreError> {
        let (bytes, content_type) = materialize_input(input, &options).await?;
        let sha256 = sha256_hex(&bytes);
        let asset_id = format!("asset_{}", Uuid::new_v4());
        let ext = extension_for_content_type(&content_type);
        let prefix = options.key_prefix.unwrap_or_else(|| "images".into());
        let object_path = Path::new(&prefix).join(format!("{asset_id}.{ext}"));
        let full_path = self.root.join(&object_path);
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| AssetStoreError::new("asset_write_failed", err.to_string()))?;
        }
        let mut file = tokio::fs::File::create(&full_path)
            .await
            .map_err(|err| AssetStoreError::new("asset_write_failed", err.to_string()))?;
        file.write_all(&bytes)
            .await
            .map_err(|err| AssetStoreError::new("asset_write_failed", err.to_string()))?;
        Ok(StoredAsset {
            asset_id,
            location: StorageLocation::Local(LocalObjectLocation {
                path: full_path,
                public_url: self.public_base_url.as_ref().map(|base| {
                    format!(
                        "{}/{}",
                        base.trim_end_matches('/'),
                        object_path.to_string_lossy().replace('\\', "/")
                    )
                }),
            }),
            content_type,
            sha256,
            byte_count: bytes.len() as u64,
            created_at: Utc::now(),
            expires_at: None,
        })
    }

    async fn signed_url(
        &self,
        asset: &StoredAsset,
        _ttl: Option<Duration>,
    ) -> Result<AssetAccessUrl, AssetStoreError> {
        match &asset.location {
            StorageLocation::Local(location) => Ok(AssetAccessUrl {
                url: location
                    .public_url
                    .clone()
                    .unwrap_or_else(|| format!("file://{}", location.path.to_string_lossy())),
                expires_at: None,
            }),
            _ => Err(AssetStoreError::new(
                "invalid_storage_location",
                "asset is not stored in local storage",
            )),
        }
    }

    async fn get_bytes(&self, asset: &StoredAsset) -> Result<Bytes, AssetStoreError> {
        match &asset.location {
            StorageLocation::Local(location) => tokio::fs::read(&location.path)
                .await
                .map(Bytes::from)
                .map_err(|err| AssetStoreError::new("asset_read_failed", err.to_string())),
            _ => Err(AssetStoreError::new(
                "invalid_storage_location",
                "asset is not stored in local storage",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_store_put_and_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalAssetStore::new(dir.path(), Some("http://localhost/assets".into()));
        let asset = store
            .put_stream(
                AssetIngestSource::Bytes {
                    bytes: Bytes::from_static(b"png"),
                    mime_type: "image/png".into(),
                },
                PutAssetOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(asset.byte_count, 3);
        let url = store.signed_url(&asset, None).await.unwrap();
        assert!(url.url.starts_with("http://localhost/assets/"));
    }

    #[tokio::test]
    async fn local_store_can_read_asset_bytes_for_base64_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalAssetStore::new(dir.path(), None);
        let asset = store
            .put_stream(
                AssetIngestSource::Bytes {
                    bytes: Bytes::from_static(b"real-image"),
                    mime_type: "image/png".into(),
                },
                PutAssetOptions::default(),
            )
            .await
            .unwrap();

        let bytes = store.get_bytes(&asset).await.unwrap();

        assert_eq!(bytes, Bytes::from_static(b"real-image"));
    }
}
