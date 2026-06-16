//! `OssAssetStore` — uploads to Aliyun OSS (or any OSS-compatible endpoint)
//! using request-signing, no SDK dependency.

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use chrono::Utc;
use hmac::{Hmac, Mac};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use sha1::Sha1;
use uuid::Uuid;

use super::{
    extension_for_content_type, materialize_input, sha256_hex, AssetAccessUrl, AssetIngestSource,
    AssetStore, AssetStoreError, OssObjectLocation, PutAssetOptions, StorageLocation, StoredAsset,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
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

pub struct OssAssetStore {
    config: OssStorageConfig,
}

impl OssAssetStore {
    pub fn new(config: OssStorageConfig) -> Self {
        Self { config }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

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
