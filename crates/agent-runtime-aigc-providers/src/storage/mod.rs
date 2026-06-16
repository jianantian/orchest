//! Asset storage abstraction: the `AssetStore`/`AssetRegistry` traits, their
//! shared wire types, and `resolve_asset_url` (the one place that turns a
//! stored asset back into a public-facing URL without leaking storage
//! internals like bucket/object-key).
//!
//! Implementations live in their own submodules:
//! - [`noop`] — accepts writes, can't read them back (placeholder).
//! - [`local`] — local filesystem (tests, local dev).
//! - [`oss`] — Aliyun OSS / OSS-compatible endpoints.
//! - [`registry`] — `AssetRegistry` implementations (in-memory, JSON file).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{AigcError, AssetIngestSource, ImageUrlOutput};

pub mod local;
pub mod noop;
pub mod oss;
pub mod registry;

pub use local::LocalAssetStore;
pub use noop::NoopAssetStore;
pub use oss::{OssAssetStore, OssStorageConfig};
pub use registry::{FileAssetRegistry, InMemoryAssetRegistry};

#[async_trait]
pub trait AssetStore: Send + Sync {
    async fn put_stream(
        &self,
        input: AssetIngestSource,
        options: PutAssetOptions,
    ) -> Result<StoredAsset, AssetStoreError>;

    async fn signed_url(
        &self,
        asset: &StoredAsset,
        ttl: Option<Duration>,
    ) -> Result<AssetAccessUrl, AssetStoreError>;

    async fn get_bytes(&self, asset: &StoredAsset) -> Result<Bytes, AssetStoreError>;
}

#[async_trait]
pub trait AssetRegistry: Send + Sync {
    async fn save(&self, scope: &AssetScope, asset: StoredAsset) -> Result<(), AssetStoreError>;
    async fn get(&self, scope: &AssetScope, asset_id: &str)
        -> Result<StoredAsset, AssetStoreError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AssetScope {
    pub tenant: String,
    pub workspace: String,
    pub app: String,
    pub namespace: String,
}

impl AssetScope {
    pub fn test() -> Self {
        Self {
            tenant: "test-tenant".into(),
            workspace: "test-workspace".into(),
            app: "test-app".into(),
            namespace: "test".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredAsset {
    pub asset_id: String,
    pub location: StorageLocation,
    pub content_type: String,
    pub sha256: String,
    pub byte_count: u64,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StorageLocation {
    Oss(OssObjectLocation),
    Local(LocalObjectLocation),
    Noop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OssObjectLocation {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub object_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalObjectLocation {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetAccessUrl {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PutAssetOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_base64_bytes: Option<u64>,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct AssetStoreError {
    pub code: String,
    pub message: String,
}

impl AssetStoreError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<AssetStoreError> for AigcError {
    fn from(value: AssetStoreError) -> Self {
        AigcError::new(value.code, value.message)
    }
}

pub async fn resolve_asset_url(
    registry: &dyn AssetRegistry,
    store: &dyn AssetStore,
    scope: &AssetScope,
    asset_id: &str,
    ttl: Option<Duration>,
) -> Result<ImageUrlOutput, AssetStoreError> {
    let stored = registry.get(scope, asset_id).await?;
    let access = store.signed_url(&stored, ttl).await?;
    Ok(ImageUrlOutput {
        url: access.url,
        expires_at: access.expires_at,
    })
}

pub type SharedAssetStore = Arc<dyn AssetStore>;
pub type SharedAssetRegistry = Arc<dyn AssetRegistry>;

/// Shared by every `AssetStore` impl: turns an `AssetIngestSource` into raw
/// bytes + a resolved content type, downloading from a URL if needed.
pub(crate) async fn materialize_input(
    input: AssetIngestSource,
    options: &PutAssetOptions,
) -> Result<(Bytes, String), AssetStoreError> {
    match input {
        AssetIngestSource::Bytes { bytes, mime_type } => Ok((bytes, mime_type)),
        AssetIngestSource::Base64 { data, mime_type } => {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|err| AssetStoreError::new("invalid_base64", err.to_string()))?;
            if let Some(limit) = options.max_base64_bytes {
                if decoded.len() as u64 > limit {
                    return Err(AssetStoreError::new(
                        "base64_too_large",
                        "base64 payload exceeds configured maximum",
                    ));
                }
            }
            Ok((Bytes::from(decoded), mime_type))
        }
        AssetIngestSource::DataUrl(data_url) => {
            let (mime_type, data) = parse_data_url(&data_url)?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|err| AssetStoreError::new("invalid_base64", err.to_string()))?;
            if let Some(limit) = options.max_base64_bytes {
                if decoded.len() as u64 > limit {
                    return Err(AssetStoreError::new(
                        "base64_too_large",
                        "base64 payload exceeds configured maximum",
                    ));
                }
            }
            Ok((Bytes::from(decoded), mime_type))
        }
        AssetIngestSource::Url(url) => {
            if let Some(path) = url.strip_prefix("file://") {
                let bytes = tokio::fs::read(path)
                    .await
                    .map_err(|err| AssetStoreError::new("asset_read_failed", err.to_string()))?;
                let content_type = options
                    .content_type_hint
                    .clone()
                    .unwrap_or_else(|| "application/octet-stream".into());
                Ok((Bytes::from(bytes), content_type))
            } else {
                let response = crate::http::shared_client()
                    .get(&url)
                    .send()
                    .await
                    .map_err(|err| {
                        AssetStoreError::new("asset_download_failed", err.to_string())
                    })?;
                let content_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string)
                    .or_else(|| options.content_type_hint.clone())
                    .unwrap_or_else(|| "application/octet-stream".into());
                let bytes = response.bytes().await.map_err(|err| {
                    AssetStoreError::new("asset_download_failed", err.to_string())
                })?;
                Ok((bytes, content_type))
            }
        }
    }
}

fn parse_data_url(data_url: &str) -> Result<(String, String), AssetStoreError> {
    let Some(rest) = data_url.strip_prefix("data:") else {
        return Err(AssetStoreError::new(
            "invalid_data_url",
            "data URL must start with data:",
        ));
    };
    let Some((meta, data)) = rest.split_once(',') else {
        return Err(AssetStoreError::new(
            "invalid_data_url",
            "data URL is missing comma",
        ));
    };
    let mime_type = meta
        .split(';')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream")
        .to_string();
    Ok((mime_type, data.to_string()))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn extension_for_content_type(content_type: &str) -> &'static str {
    match content_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_asset_url_returns_public_output_only() {
        let registry = InMemoryAssetRegistry::default();
        let dir = tempfile::tempdir().unwrap();
        let store = LocalAssetStore::new(dir.path(), Some("http://localhost/assets".into()));
        let scope = AssetScope::test();
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
        let asset_id = asset.asset_id.clone();
        registry.save(&scope, asset).await.unwrap();
        let output = resolve_asset_url(&registry, &store, &scope, &asset_id, None)
            .await
            .unwrap();
        assert!(output.url.starts_with("http://localhost/assets/"));
        let serialized = serde_json::to_string(&output).unwrap();
        assert!(!serialized.contains("bucket"));
        assert!(!serialized.contains("object_key"));
    }
}
