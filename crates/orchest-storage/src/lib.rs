//! Unified object storage for asset persistence (docs/todo/object-storage.md).
//!
//! Downstream products (motif) generate assets — audio, covers, photos, HTML —
//! that must be stored durably: provider-hosted URLs expire within days and
//! rot in the database. This crate is the one interface over per-vendor
//! object-storage signing dialects, mirroring how the provider wall unifies
//! wire dialects:
//!
//! | dialect | signature |
//! |---|---|
//! | [`AliyunOss`](ObjectStoreDialect::AliyunOss) | V1 header signature: `Authorization: OSS <AK>:<base64(HMAC-SHA1(StringToSign))>` |
//! | [`TencentCos`](ObjectStoreDialect::TencentCos) | V5 `q-sign-algorithm=sha1` (signs the `host` header; covers PUT/DELETE without query params) |
//! | [`VolcengineTos`](ObjectStoreDialect::VolcengineTos) | TOS4-HMAC-SHA256 — second wave, rejected by the factory today |
//!
//! Standalone crate by decision (2026-08-19): consumers are downstream
//! products, not provider impl crates, and the coupling to provider
//! infrastructure was a single shared reqwest client — so this crate depends
//! on neither `orchest-protocol` nor `orchest-provider-core`. Signing deps are
//! inherent to the crate; consumers opt in by depending on it at all.
//!
//! Non-goals (first wave): multipart upload, list/head/copy, streaming bodies.
//! Callers enforce their own size limit (motif: 20 MiB); the body is buffered
//! as `Vec<u8>`.

mod cos;
mod oss;

use std::sync::Arc;

use async_trait::async_trait;

/// Unified object-storage interface. One `Arc<dyn ObjectStore>` per product;
/// the concrete dialect is chosen by [`create_object_store`].
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Upload the whole object under `key`. `bytes` is buffered in memory —
    /// callers enforce their own size limit (no streaming, see non-goals).
    async fn put_object(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<(), ObjectStoreError>;

    /// Fetch the whole object, reading from the **origin** (the vendor
    /// endpoint) — never through the CDN / [`public_base`](ObjectStore::public_base)
    /// — so server-side proxying sees fresh content, not stale edge cache.
    async fn get_object(&self, key: &str) -> Result<Vec<u8>, ObjectStoreError>;

    /// Delete the object. Deleting a missing key is **not** an error (the
    /// cleanup path re-deletes current + historical versions of the same key).
    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError>;

    /// A presigned GET URL valid for `ttl` (link-is-capability; the object
    /// itself stays non-public). Purely local computation — no network call.
    fn presigned_url(&self, key: &str, ttl: std::time::Duration) -> String;

    /// The object's public access URL (CDN / public-read bucket domain joined
    /// with the key — unsigned, link-is-capability model).
    fn public_url(&self, key: &str) -> String;

    /// The public URL prefix, so callers can recover the key from a stored URL
    /// (the delete cleanup path).
    fn public_base(&self) -> &str;
}

/// Errors from object-storage operations.
#[derive(Debug, thiserror::Error)]
pub enum ObjectStoreError {
    /// Transport-level failure (DNS, connect, TLS, timeout, body read).
    #[error("object store transport failure: {0}")]
    Transport(#[from] reqwest::Error),

    /// The server answered with a non-success status (404 is success only for
    /// idempotent deletes). `body` is truncated — the diagnosis lifeline for
    /// signature/permission failures.
    #[error("object store rejected {method} {path}: HTTP {status}, body: {body}")]
    Rejected {
        method: &'static str,
        path: String,
        status: reqwest::StatusCode,
        body: String,
    },

    /// The dialect is known but not implemented (yet) by this build.
    #[error("unsupported object store dialect: {0}")]
    UnsupportedDialect(String),

    /// A required config field is empty.
    #[error("invalid object store configuration: {0}")]
    InvalidConfig(String),
}

/// The storage dialect. Chosen by identity at construction — the same
/// selection pattern the provider registry uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectStoreDialect {
    /// Aliyun OSS, V1 header signature.
    AliyunOss,
    /// Tencent COS, V5 `q-sign-algorithm=sha1`.
    TencentCos,
    /// Volcengine TOS, SigV4-style (`TOS4-HMAC-SHA256` + derived key).
    /// Second wave — [`create_object_store`] rejects it with a clear error.
    /// The 6-field config shape already accommodates it: TOS derives region
    /// and service from `endpoint`/`bucket` exactly like OSS/COS do.
    VolcengineTos,
}

/// Runtime configuration for one object store. Environment-variable reading
/// stays in the application layer; the SDK takes explicit config only.
#[derive(Debug, Clone)]
pub struct ObjectStoreConfig {
    pub dialect: ObjectStoreDialect,
    pub access_key_id: String,
    pub access_key_secret: String,
    pub bucket: String,
    /// Vendor endpoint. OSS: `oss-cn-hangzhou.aliyuncs.com`; COS: the region
    /// `ap-guangzhou`; TOS: `tos-cn-beijing.volces.com`. Each dialect
    /// assembles its own request host from this.
    pub endpoint: String,
    /// Public access URL prefix (CDN domain or bucket public-read domain).
    pub public_base: String,
}

/// Build the [`ObjectStore`] for `config`'s dialect. Returns an
/// [`ObjectStoreError::InvalidConfig`] on empty required fields and
/// [`ObjectStoreError::UnsupportedDialect`] for TOS (second wave).
pub fn create_object_store(
    config: ObjectStoreConfig,
) -> Result<Arc<dyn ObjectStore>, ObjectStoreError> {
    validate(&config)?;
    match config.dialect {
        ObjectStoreDialect::AliyunOss => {
            let host = format!("{}.{}", config.bucket, config.endpoint);
            Ok(Arc::new(oss::OssStore::new(
                config,
                format!("https://{host}"),
                system_now,
            )))
        }
        ObjectStoreDialect::TencentCos => {
            let host = format!("{}.cos.{}.myqcloud.com", config.bucket, config.endpoint);
            Ok(Arc::new(cos::CosStore::new(
                config,
                format!("https://{host}"),
                system_now,
            )))
        }
        ObjectStoreDialect::VolcengineTos => Err(ObjectStoreError::UnsupportedDialect(
            "volcengine TOS is not implemented yet (second wave, docs/todo/object-storage.md)"
                .to_string(),
        )),
    }
}

fn validate(config: &ObjectStoreConfig) -> Result<(), ObjectStoreError> {
    for (name, value) in [
        ("access_key_id", config.access_key_id.as_str()),
        ("access_key_secret", config.access_key_secret.as_str()),
        ("bucket", config.bucket.as_str()),
        ("endpoint", config.endpoint.as_str()),
        ("public_base", config.public_base.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ObjectStoreError::InvalidConfig(format!(
                "{name} must not be empty"
            )));
        }
    }
    Ok(())
}

/// Percent-encode an object key for the URL path and the signature. RFC 3986
/// unreserved characters plus `/` pass through; everything else becomes `%XX`
/// (uppercase hex, UTF-8 bytes for non-ASCII). The exact same encoding feeds
/// the request URL, the public URL, and the signed canonical resource, so
/// they can never diverge.
pub(crate) fn encode_key(key: &str) -> String {
    percent_encode(key, true)
}

/// Percent-encode a query-parameter value (RFC 3986 unreserved only — `/` is
/// encoded too). Used for signature values and key ids in presigned URLs.
pub(crate) fn encode_query_value(value: &str) -> String {
    percent_encode(value, false)
}

fn percent_encode(input: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(input.len() + input.len() / 4);
    for b in input.bytes() {
        let safe = matches!(
            b,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~'
        ) || (keep_slash && b == b'/');
        if safe {
            out.push(b as char);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

const DAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// RFC 1123 GMT date for an epoch-seconds timestamp (the OSS `Date` header).
/// Hand-rolled so no chrono/time dependency is pulled behind the `storage`
/// feature.
pub(crate) fn http_date(epoch_secs: i64) -> String {
    let days = epoch_secs.div_euclid(86_400);
    let secs = epoch_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    // 1970-01-01 was a Thursday (weekday index 4).
    let weekday = (days + 4).rem_euclid(7) as usize;
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        DAY_NAMES[weekday],
        day,
        MONTH_NAMES[(month - 1) as usize],
        year,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

/// Days-since-epoch → (year, month, day), Howard Hinnant's civil-from-days.
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let year = yoe as i64 + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Epoch seconds from the system clock — the default signer clock.
pub(crate) fn system_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The crate's shared reqwest client (300s timeout, 20 idle conns/host —
/// ample for ≤20 MiB asset bodies). Mirrors the provider-core defaults so
/// request behavior matches the rest of the workspace.
///
/// # Panics
/// Only on catastrophic platform misconfiguration (no TLS backend): an early
/// panic beats failing every later request — the documented invariant that
/// licenses the `expect` below (matches provider-core's `build_client`).
pub(crate) fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(20)
            .build()
            .expect("failed to build shared reqwest::Client")
    });
    &CLIENT
}

/// Send the signed request and enforce the status contract: 2xx → the
/// response; 404 → the response only for idempotent deletes; anything else →
/// [`ObjectStoreError::Rejected`] carrying the status and a truncated body
/// summary.
async fn send(
    method: &'static str,
    path: &str,
    request: reqwest::RequestBuilder,
    idempotent_delete: bool,
) -> Result<reqwest::Response, ObjectStoreError> {
    let response = request.send().await?;
    let status = response.status();
    if status.is_success() || (idempotent_delete && status == reqwest::StatusCode::NOT_FOUND) {
        return Ok(response);
    }
    let body = summarize(response.bytes().await?);
    Err(ObjectStoreError::Rejected {
        method,
        path: path.to_string(),
        status,
        body,
    })
}

/// Send and discard the body (put/delete paths).
pub(crate) async fn execute(
    method: &'static str,
    path: &str,
    request: reqwest::RequestBuilder,
    idempotent_delete: bool,
) -> Result<(), ObjectStoreError> {
    send(method, path, request, idempotent_delete)
        .await
        .map(|_| ())
}

/// Send and read the body (the get path).
pub(crate) async fn fetch(
    method: &'static str,
    path: &str,
    request: reqwest::RequestBuilder,
) -> Result<Vec<u8>, ObjectStoreError> {
    let response = send(method, path, request, false).await?;
    Ok(response.bytes().await?.to_vec())
}

/// Truncate a response body to ~200 chars for error reporting (an XML error
/// document would drown the log). Reads at most 2 KiB of the body.
fn summarize(body: bytes::Bytes) -> String {
    let mut text: String = String::from_utf8_lossy(&body[..body.len().min(2048)])
        .chars()
        .take(200)
        .collect();
    if body.len() > 2048 {
        text.push('…');
    }
    text
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests {
    use super::*;

    fn config(dialect: ObjectStoreDialect) -> ObjectStoreConfig {
        ObjectStoreConfig {
            dialect,
            access_key_id: "test-access-key".to_string(),
            access_key_secret: "test-secret-key".to_string(),
            bucket: "my-bucket".to_string(),
            endpoint: "oss-cn-hangzhou.aliyuncs.com".to_string(),
            public_base: "https://cdn.example.com".to_string(),
        }
    }

    #[test]
    fn encode_key_passes_unreserved_and_encodes_the_rest() {
        assert_eq!(
            encode_key("audio/2026/gift a.mp3"),
            "audio/2026/gift%20a.mp3"
        );
        assert_eq!(encode_key("简单.mp3"), "%E7%AE%80%E5%8D%95.mp3");
        assert_eq!(encode_key("a?b#c&d=e"), "a%3Fb%23c%26d%3De");
        assert_eq!(encode_key("~._-/plain"), "~._-/plain");
    }

    #[test]
    fn http_date_formats_known_epochs() {
        assert_eq!(http_date(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(http_date(1_700_000_000), "Tue, 14 Nov 2023 22:13:20 GMT");
        assert_eq!(http_date(1_709_164_800), "Thu, 29 Feb 2024 00:00:00 GMT");
        assert_eq!(http_date(-1), "Wed, 31 Dec 1969 23:59:59 GMT");
        assert_eq!(http_date(4_102_444_800), "Fri, 01 Jan 2100 00:00:00 GMT");
    }

    #[test]
    fn create_object_store_selects_oss_and_serves_public_urls() {
        let store = create_object_store(config(ObjectStoreDialect::AliyunOss)).expect("oss");
        assert_eq!(store.public_base(), "https://cdn.example.com");
        assert_eq!(
            store.public_url("audio/2026/gift a.mp3"),
            "https://cdn.example.com/audio/2026/gift%20a.mp3"
        );
        // The presigned URL is part of the trait surface (link-is-capability);
        // it targets the vendor endpoint, not the public CDN base.
        let presigned = store.presigned_url("k.mp3", std::time::Duration::from_secs(60));
        assert!(
            presigned.starts_with(
                "https://my-bucket.oss-cn-hangzhou.aliyuncs.com/k.mp3?OSSAccessKeyId=test-access-key&Expires="
            ),
            "{presigned}"
        );
    }

    #[test]
    fn public_url_joins_base_with_trailing_slash() {
        let mut cfg = config(ObjectStoreDialect::TencentCos);
        cfg.public_base = "https://cdn.example.com/".to_string();
        let store = create_object_store(cfg).expect("cos");
        assert_eq!(
            store.public_url("photo/u.jpg"),
            "https://cdn.example.com/photo/u.jpg"
        );
    }

    #[test]
    fn create_object_store_rejects_tos_with_clear_error() {
        let err = match create_object_store(config(ObjectStoreDialect::VolcengineTos)) {
            Err(err) => err,
            Ok(_) => panic!("tos must be rejected"),
        };
        match err {
            ObjectStoreError::UnsupportedDialect(msg) => {
                assert!(msg.contains("TOS"), "dialect named in the error: {msg}");
            }
            other => panic!("expected UnsupportedDialect, got {other:?}"),
        }
    }

    #[test]
    fn create_object_store_validates_required_fields() {
        for empty in [
            "access_key_id",
            "access_key_secret",
            "bucket",
            "endpoint",
            "public_base",
        ] {
            let mut cfg = config(ObjectStoreDialect::AliyunOss);
            match empty {
                "access_key_id" => cfg.access_key_id = " ".to_string(),
                "access_key_secret" => cfg.access_key_secret = String::new(),
                "bucket" => cfg.bucket = String::new(),
                "endpoint" => cfg.endpoint = String::new(),
                "public_base" => cfg.public_base = String::new(),
                _ => unreachable!(),
            }
            let err = match create_object_store(cfg) {
                Err(err) => err,
                Ok(_) => panic!("empty field must be rejected"),
            };
            match err {
                ObjectStoreError::InvalidConfig(msg) => {
                    assert!(msg.contains(empty), "{empty} named in the error: {msg}");
                }
                other => panic!("expected InvalidConfig, got {other:?}"),
            }
        }
    }

    #[test]
    fn summarize_truncates_long_bodies_to_200_chars() {
        let big = bytes::Bytes::from(vec![b'x'; 10_000]);
        let text = summarize(big);
        assert_eq!(text.chars().count(), 201); // 200 chars + ellipsis
        assert!(text.ends_with('…'));
        let small = summarize(bytes::Bytes::from_static(b"short"));
        assert_eq!(small, "short");
    }
}
