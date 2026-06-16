use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::{AigcError, AssetIngestSource, ImageUrlOutput};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OssStorageConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub access_key_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_url_ttl: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_prefix: Option<String>,
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

#[derive(Default)]
pub struct InMemoryAssetRegistry {
    assets: Mutex<HashMap<(AssetScope, String), StoredAsset>>,
}

#[async_trait]
impl AssetRegistry for InMemoryAssetRegistry {
    async fn save(&self, scope: &AssetScope, asset: StoredAsset) -> Result<(), AssetStoreError> {
        self.assets
            .lock()
            .map_err(|_| AssetStoreError::new("registry_poisoned", "asset registry lock poisoned"))?
            .insert((scope.clone(), asset.asset_id.clone()), asset);
        Ok(())
    }

    async fn get(
        &self,
        scope: &AssetScope,
        asset_id: &str,
    ) -> Result<StoredAsset, AssetStoreError> {
        self.assets
            .lock()
            .map_err(|_| AssetStoreError::new("registry_poisoned", "asset registry lock poisoned"))?
            .get(&(scope.clone(), asset_id.to_string()))
            .cloned()
            .ok_or_else(|| {
                AssetStoreError::new("asset_not_found", "asset not found in requested scope")
            })
    }
}

pub struct FileAssetRegistry {
    path: PathBuf,
    memory: InMemoryAssetRegistry,
}

impl FileAssetRegistry {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self, AssetStoreError> {
        let path = path.into();
        let memory = InMemoryAssetRegistry::default();
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|err| AssetStoreError::new("registry_read_failed", err.to_string()))?;
            let entries: Vec<(AssetScope, StoredAsset)> = serde_json::from_slice(&bytes)
                .map_err(|err| AssetStoreError::new("registry_parse_failed", err.to_string()))?;
            for (scope, asset) in entries {
                memory.save(&scope, asset).await?;
            }
        }
        Ok(Self { path, memory })
    }

    async fn flush(&self) -> Result<(), AssetStoreError> {
        let entries = self
            .memory
            .assets
            .lock()
            .map_err(|_| AssetStoreError::new("registry_poisoned", "asset registry lock poisoned"))?
            .iter()
            .map(|((scope, _), asset)| (scope.clone(), asset.clone()))
            .collect::<Vec<_>>();
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| AssetStoreError::new("registry_write_failed", err.to_string()))?;
        }
        let bytes = serde_json::to_vec_pretty(&entries)
            .map_err(|err| AssetStoreError::new("registry_serialize_failed", err.to_string()))?;
        tokio::fs::write(&self.path, bytes)
            .await
            .map_err(|err| AssetStoreError::new("registry_write_failed", err.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl AssetRegistry for FileAssetRegistry {
    async fn save(&self, scope: &AssetScope, asset: StoredAsset) -> Result<(), AssetStoreError> {
        self.memory.save(scope, asset).await?;
        self.flush().await
    }

    async fn get(
        &self,
        scope: &AssetScope,
        asset_id: &str,
    ) -> Result<StoredAsset, AssetStoreError> {
        self.memory.get(scope, asset_id).await
    }
}

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

pub struct OssAssetStore {
    config: OssStorageConfig,
}

impl OssAssetStore {
    pub fn new(config: OssStorageConfig) -> Self {
        Self { config }
    }
}

impl OssStorageConfig {
    pub fn from_env() -> Result<Self, AssetStoreError> {
        Self::from_env_prefix("AIGC_OSS")
    }

    pub fn from_env_prefix(prefix: &str) -> Result<Self, AssetStoreError> {
        let read_required = |suffix: &str| -> Result<String, AssetStoreError> {
            let name = format!("{prefix}_{suffix}");
            std::env::var(&name)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(|value| value.trim().to_string())
                .ok_or_else(|| {
                    AssetStoreError::new(
                        "missing_storage_config",
                        format!("storage environment variable {name} is required"),
                    )
                })
        };
        let read_optional = |suffix: &str| -> Option<String> {
            std::env::var(format!("{prefix}_{suffix}"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let signed_url_ttl = read_optional("SIGNED_URL_TTL_SECONDS")
            .map(|value| {
                value
                    .parse::<u64>()
                    .map(Duration::from_secs)
                    .map_err(|err| AssetStoreError::new("invalid_storage_config", err.to_string()))
            })
            .transpose()?;
        Ok(Self {
            endpoint: read_required("ENDPOINT")?,
            bucket: read_required("BUCKET")?,
            region: read_required("REGION")?,
            access_key_id: read_required("ACCESS_KEY_ID")?,
            access_key_secret: read_required("ACCESS_KEY_SECRET")?,
            public_base_url: read_optional("PUBLIC_BASE_URL"),
            signed_url_ttl,
            key_prefix: read_optional("KEY_PREFIX"),
        })
    }
}

#[async_trait]
impl AssetStore for OssAssetStore {
    async fn put_stream(
        &self,
        input: AssetIngestSource,
        options: PutAssetOptions,
    ) -> Result<StoredAsset, AssetStoreError> {
        let (bytes, content_type) = materialize_input(input, &options).await?;
        let sha256 = sha256_hex(&bytes);
        let asset_id = format!("asset_{}", Uuid::new_v4());
        let ext = extension_for_content_type(&content_type);
        let prefix = options
            .key_prefix
            .or_else(|| self.config.key_prefix.clone())
            .unwrap_or_else(|| "images".into());
        let object_key = format!("{prefix}/{asset_id}.{ext}");
        self.put_object(&object_key, bytes.clone(), &content_type)
            .await?;
        Ok(StoredAsset {
            asset_id,
            location: StorageLocation::Oss(OssObjectLocation {
                endpoint: self.config.endpoint.clone(),
                bucket: self.config.bucket.clone(),
                region: self.config.region.clone(),
                object_key,
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
        ttl: Option<Duration>,
    ) -> Result<AssetAccessUrl, AssetStoreError> {
        let StorageLocation::Oss(location) = &asset.location else {
            return Err(AssetStoreError::new(
                "invalid_storage_location",
                "asset is not stored in OSS",
            ));
        };
        if let Some(public_base_url) = &self.config.public_base_url {
            return Ok(AssetAccessUrl {
                url: format!(
                    "{}/{}",
                    public_base_url.trim_end_matches('/'),
                    location.object_key
                ),
                expires_at: None,
            });
        }
        let expires_at = ttl.or(self.config.signed_url_ttl).map(|ttl| {
            Utc::now()
                + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::seconds(0))
        });
        let expires = expires_at
            .unwrap_or_else(|| Utc::now() + chrono::Duration::minutes(15))
            .timestamp();
        let resource = canonical_resource(&location.bucket, &location.object_key);
        let string_to_sign = format!("GET\n\n\n{expires}\n{resource}");
        let signature = oss_signature(&self.config.access_key_secret, &string_to_sign)?;
        let encoded_signature = utf8_percent_encode(&signature, NON_ALPHANUMERIC).to_string();
        Ok(AssetAccessUrl {
            url: format!(
                "{}?OSSAccessKeyId={}&Expires={}&Signature={}",
                self.object_url(&location.bucket, &location.object_key),
                utf8_percent_encode(&self.config.access_key_id, NON_ALPHANUMERIC),
                expires,
                encoded_signature
            ),
            expires_at,
        })
    }

    async fn get_bytes(&self, asset: &StoredAsset) -> Result<Bytes, AssetStoreError> {
        let StorageLocation::Oss(location) = &asset.location else {
            return Err(AssetStoreError::new(
                "invalid_storage_location",
                "asset is not stored in OSS",
            ));
        };
        let date = oss_http_date();
        let resource = canonical_resource(&location.bucket, &location.object_key);
        let signature = oss_signature(
            &self.config.access_key_secret,
            &format!("GET\n\n\n{date}\n{resource}"),
        )?;
        let response = crate::http::shared_client()
            .get(self.object_url(&location.bucket, &location.object_key))
            .header("Date", date)
            .header(
                "Authorization",
                format!("OSS {}:{signature}", self.config.access_key_id),
            )
            .send()
            .await
            .map_err(|err| AssetStoreError::new("oss_get_failed", err.to_string()))?;
        if !response.status().is_success() {
            return Err(AssetStoreError::new(
                "oss_get_failed",
                format!("OSS GET failed with status {}", response.status()),
            ));
        }
        response
            .bytes()
            .await
            .map_err(|err| AssetStoreError::new("oss_get_failed", err.to_string()))
    }
}

impl OssAssetStore {
    async fn put_object(
        &self,
        object_key: &str,
        bytes: Bytes,
        content_type: &str,
    ) -> Result<(), AssetStoreError> {
        let date = oss_http_date();
        let resource = canonical_resource(&self.config.bucket, object_key);
        let string_to_sign = format!("PUT\n\n{content_type}\n{date}\n{resource}");
        let signature = oss_signature(&self.config.access_key_secret, &string_to_sign)?;
        let response = crate::http::shared_client()
            .put(self.object_url(&self.config.bucket, object_key))
            .header("Date", date)
            .header("Content-Type", content_type)
            .header(
                "Authorization",
                format!("OSS {}:{signature}", self.config.access_key_id),
            )
            .body(bytes)
            .send()
            .await
            .map_err(|err| AssetStoreError::new("oss_put_failed", err.to_string()))?;
        if !response.status().is_success() {
            return Err(AssetStoreError::new(
                "oss_put_failed",
                format!("OSS PUT failed with status {}", response.status()),
            ));
        }
        Ok(())
    }

    fn object_url(&self, bucket: &str, object_key: &str) -> String {
        let endpoint = self.config.endpoint.trim_end_matches('/');
        if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            format!("{endpoint}/{bucket}/{object_key}")
        } else {
            format!("https://{bucket}.{endpoint}/{object_key}")
        }
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

async fn materialize_input(
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn canonical_resource(bucket: &str, object_key: &str) -> String {
    format!("/{bucket}/{object_key}")
}

fn oss_signature(secret: &str, string_to_sign: &str) -> Result<String, AssetStoreError> {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
        .map_err(|err| AssetStoreError::new("oss_sign_failed", err.to_string()))?;
    mac.update(string_to_sign.as_bytes());
    Ok(base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
}

fn oss_http_date() -> String {
    Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn extension_for_content_type(content_type: &str) -> &'static str {
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
    async fn in_memory_registry_scopes_assets() {
        let registry = InMemoryAssetRegistry::default();
        let asset = StoredAsset {
            asset_id: "asset-1".into(),
            location: StorageLocation::Noop,
            content_type: "image/png".into(),
            sha256: "abc".into(),
            byte_count: 1,
            created_at: Utc::now(),
            expires_at: None,
        };
        let scope = AssetScope::test();
        registry.save(&scope, asset.clone()).await.unwrap();
        assert_eq!(
            registry.get(&scope, "asset-1").await.unwrap().asset_id,
            "asset-1"
        );
        let wrong = AssetScope {
            namespace: "wrong".into(),
            ..scope
        };
        assert_eq!(
            registry.get(&wrong, "asset-1").await.unwrap_err().code,
            "asset_not_found"
        );
    }

    #[tokio::test]
    async fn file_registry_persists_across_instances() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");
        let scope = AssetScope::test();
        let asset = StoredAsset {
            asset_id: "asset-2".into(),
            location: StorageLocation::Noop,
            content_type: "image/png".into(),
            sha256: "abc".into(),
            byte_count: 1,
            created_at: Utc::now(),
            expires_at: None,
        };
        let registry = FileAssetRegistry::open(&path).await.unwrap();
        registry.save(&scope, asset).await.unwrap();
        let reopened = FileAssetRegistry::open(&path).await.unwrap();
        assert_eq!(
            reopened.get(&scope, "asset-2").await.unwrap().asset_id,
            "asset-2"
        );
    }

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

    #[tokio::test]
    async fn oss_store_uploads_object_with_authorization() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 1024];
            loop {
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if request_is_complete(&request) {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&request);
            let lower_request = request.to_ascii_lowercase();
            assert!(request.starts_with("PUT /bucket/images/asset_"));
            assert!(lower_request.contains("authorization: oss ak:"));
            assert!(request.contains("\r\n\r\npng"));
            let response = "HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n";
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let store = OssAssetStore::new(OssStorageConfig {
            endpoint: format!("http://{addr}"),
            bucket: "bucket".into(),
            region: "cn-test".into(),
            access_key_id: "ak".into(),
            access_key_secret: "sk".into(),
            public_base_url: None,
            signed_url_ttl: None,
            key_prefix: Some("images".into()),
        });
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

        server.await.unwrap();
        assert!(matches!(asset.location, StorageLocation::Oss(_)));
        assert_eq!(asset.byte_count, 3);
    }

    #[test]
    fn oss_config_loads_storage_scoped_environment_names() {
        let prefix = "AIGC_STORAGE_OSS_TEST";
        for suffix in [
            "ENDPOINT",
            "BUCKET",
            "REGION",
            "ACCESS_KEY_ID",
            "ACCESS_KEY_SECRET",
            "PUBLIC_BASE_URL",
            "SIGNED_URL_TTL_SECONDS",
            "KEY_PREFIX",
        ] {
            std::env::remove_var(format!("{prefix}_{suffix}"));
        }
        std::env::set_var(format!("{prefix}_ENDPOINT"), "oss-cn-test.aliyuncs.com");
        std::env::set_var(format!("{prefix}_BUCKET"), "orchest-aigc-test");
        std::env::set_var(format!("{prefix}_REGION"), "cn-test");
        std::env::set_var(format!("{prefix}_ACCESS_KEY_ID"), "storage-ak");
        std::env::set_var(format!("{prefix}_ACCESS_KEY_SECRET"), "storage-sk");
        std::env::set_var(
            format!("{prefix}_PUBLIC_BASE_URL"),
            "https://assets.example",
        );
        std::env::set_var(format!("{prefix}_SIGNED_URL_TTL_SECONDS"), "60");
        std::env::set_var(format!("{prefix}_KEY_PREFIX"), "generated");
        std::env::set_var("DASHSCOPE_API_KEY", "dashscope-key");

        let config = OssStorageConfig::from_env_prefix(prefix).unwrap();

        assert_eq!(config.access_key_id, "storage-ak");
        assert_eq!(config.access_key_secret, "storage-sk");
        assert_eq!(
            config.public_base_url.as_deref(),
            Some("https://assets.example")
        );
        assert_eq!(config.signed_url_ttl, Some(Duration::from_secs(60)));
        assert_eq!(config.key_prefix.as_deref(), Some("generated"));
        assert_ne!(config.access_key_secret, "dashscope-key");
    }

    fn request_is_complete(request: &[u8]) -> bool {
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        request.len() >= header_end + 4 + content_length
    }
}
