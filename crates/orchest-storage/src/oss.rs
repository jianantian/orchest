//! Aliyun OSS dialect — V1 header signature.
//!
//! `Authorization: OSS <AccessKeyId>:<base64(HMAC-SHA1(SK, StringToSign))>`
//! with `StringToSign = VERB + "\n" + Content-MD5 + "\n" + Content-Type + "\n"
//! + Date + "\n" + CanonicalizedResource` (aliyun V1 header signature).
//!
//! We send no `x-oss-*` headers, so CanonicalizedOSSHeaders is empty; the
//! canonical resource is `/{bucket}/{encoded-key}`. `Date` is the RFC 1123 GMT
//! header, derived from the injected clock.

use async_trait::async_trait;
use base64::Engine;
use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;

use super::{
    encode_key, encode_query_value, execute, fetch, http_date, ObjectStore, ObjectStoreConfig,
    ObjectStoreError,
};

type HmacSha1 = Hmac<Sha1>;
pub(crate) struct OssStore {
    access_key_id: String,
    access_key_secret: String,
    bucket: String,
    /// The vendor host `{bucket}.{endpoint}` — used for signing the canonical
    /// resource and for presigned URLs (which must target the real endpoint).
    host: String,
    public_base: String,
    /// Actual HTTP base for requests: the vendor host by default, overridable
    /// to a loopback server in tests. Signing always uses the canonical
    /// resource (bucket + key), never the transport host.
    transport_base: String,
    now: fn() -> i64,
}

impl OssStore {
    pub(crate) fn new(config: ObjectStoreConfig, transport_base: String, now: fn() -> i64) -> Self {
        Self {
            access_key_id: config.access_key_id,
            access_key_secret: config.access_key_secret,
            bucket: config.bucket.clone(),
            host: format!("{}.{}", config.bucket, config.endpoint),
            public_base: config.public_base.trim_end_matches('/').to_string(),
            transport_base,
            now,
        }
    }

    /// `base64(HMAC-SHA1(SK, message))` — the V1 signature primitive.
    fn hmac_base64(&self, message: &str) -> String {
        let mut mac = HmacSha1::new_from_slice(self.access_key_secret.as_bytes())
            .expect("HMAC accepts keys of any length");
        mac.update(message.as_bytes());
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    /// The V1 signature for `verb` against `path` (already percent-encoded,
    /// with leading slash). `middle` is the `Content-MD5` + `Content-Type`
    /// canonical lines, already joined with `\n` — two empty lines (`"\n"`)
    /// for DELETE, per the aliyun spec.
    fn sign(&self, verb: &str, path: &str, middle: &str, date: &str) -> String {
        let string_to_sign = format!(
            "{verb}\n{middle}\n{date}\n/{bucket}{path}",
            bucket = self.bucket
        );
        self.hmac_base64(&string_to_sign)
    }
}

#[async_trait]
impl ObjectStore for OssStore {
    async fn put_object(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<(), ObjectStoreError> {
        let path = format!("/{}", encode_key(key));
        let date = http_date((self.now)());
        let content_md5 =
            base64::engine::general_purpose::STANDARD.encode(Md5::digest(bytes.as_slice()));
        let middle = format!("{content_md5}\n{content_type}");
        let signature = self.sign("PUT", &path, &middle, &date);
        let request = crate::client()
            .put(format!("{}{}", self.transport_base, path))
            .header("Content-Type", content_type)
            .header("Content-MD5", content_md5)
            .header("Date", date)
            .header(
                "Authorization",
                format!("OSS {}:{}", self.access_key_id, signature),
            )
            .body(bytes);
        execute("PUT", &path, request, false).await
    }

    async fn get_object(&self, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let path = format!("/{}", encode_key(key));
        let date = http_date((self.now)());
        // GET string-to-sign: empty Content-MD5 + Content-Type lines.
        let signature = self.sign("GET", &path, "\n", &date);
        let request = crate::client()
            .get(format!("{}{}", self.transport_base, path))
            .header("Date", date)
            .header(
                "Authorization",
                format!("OSS {}:{}", self.access_key_id, signature),
            );
        fetch("GET", &path, request).await
    }

    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError> {
        let path = format!("/{}", encode_key(key));
        let date = http_date((self.now)());
        let signature = self.sign("DELETE", &path, "\n", &date);
        let request = crate::client()
            .delete(format!("{}{}", self.transport_base, path))
            .header("Date", date)
            .header(
                "Authorization",
                format!("OSS {}:{}", self.access_key_id, signature),
            );
        execute("DELETE", &path, request, true).await
    }

    fn presigned_url(&self, key: &str, ttl: std::time::Duration) -> String {
        let expires = (self.now)() + ttl.as_secs() as i64;
        let path = format!("/{}", encode_key(key));
        let string_to_sign = format!("GET\n\n\n{expires}\n/{bucket}{path}", bucket = self.bucket);
        let signature = self.hmac_base64(&string_to_sign);
        format!(
            "https://{host}{path}?OSSAccessKeyId={ak}&Expires={expires}&Signature={signature}",
            host = self.host,
            ak = encode_query_value(&self.access_key_id),
            signature = encode_query_value(&signature),
        )
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_base, encode_key(key))
    }

    fn public_base(&self) -> &str {
        &self.public_base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_support, ObjectStoreDialect};

    /// Cross-checked against an independent Python hmac/hashlib implementation.
    const FIXED_NOW: fn() -> i64 = || 1_700_000_000;

    fn store(base_url: &str) -> OssStore {
        OssStore::new(
            ObjectStoreConfig {
                dialect: ObjectStoreDialect::AliyunOss,
                access_key_id: "test-access-key".to_string(),
                access_key_secret: "test-secret-key".to_string(),
                bucket: "my-bucket".to_string(),
                endpoint: "oss-cn-hangzhou.aliyuncs.com".to_string(),
                public_base: "https://cdn.example.com".to_string(),
            },
            base_url.to_string(),
            FIXED_NOW,
        )
    }

    #[tokio::test]
    async fn put_signs_v1_known_vector_and_sends_shape() {
        let server = test_support::serve().await;
        let store = store(&server.base_url);

        store
            .put_object(
                "audio/2026/gift a.mp3",
                b"hello world".to_vec(),
                "audio/mpeg",
            )
            .await
            .expect("put");

        let req = &server.requests()[0];
        assert_eq!(req.request_line, "PUT /audio/2026/gift%20a.mp3 HTTP/1.1");
        assert_eq!(req.body, b"hello world");
        assert_eq!(req.header("content-type"), Some("audio/mpeg"));
        assert_eq!(req.header("date"), Some("Tue, 14 Nov 2023 22:13:20 GMT"));
        assert_eq!(req.header("content-md5"), Some("XrY7u+Ae7tCTyyK7j1rNww=="));
        assert_eq!(
            req.header("authorization"),
            Some("OSS test-access-key:L9zvuo6kENcfIsL5+PUiVCRFa7M=")
        );
    }

    #[tokio::test]
    async fn delete_signs_v1_known_vector() {
        let server = test_support::serve().await;
        let store = store(&server.base_url);

        store
            .delete_object("audio/2026/gift a.mp3")
            .await
            .expect("delete");

        let req = &server.requests()[0];
        assert_eq!(req.request_line, "DELETE /audio/2026/gift%20a.mp3 HTTP/1.1");
        assert_eq!(
            req.header("authorization"),
            Some("OSS test-access-key:FuXycwTSWMkLpnKPSkSAhuxzzPs=")
        );
    }

    #[tokio::test]
    async fn delete_is_idempotent_on_404() {
        let server = test_support::serve().await;
        server.set_response(404, b"<Error><Code>NoSuchKey</Code></Error>");
        let store = store(&server.base_url);

        store
            .delete_object("missing.mp3")
            .await
            .expect("404 is not an error");
    }

    #[tokio::test]
    async fn rejected_error_carries_status_and_body_summary() {
        let server = test_support::serve().await;
        server.set_response(403, b"<Error><Code>SignatureDoesNotMatch</Code></Error>");
        let store = store(&server.base_url);

        let err = store
            .put_object("audio/a.mp3", b"x".to_vec(), "audio/mpeg")
            .await
            .expect_err("403 must fail");
        match err {
            ObjectStoreError::Rejected {
                method,
                path,
                status,
                body,
            } => {
                assert_eq!(method, "PUT");
                assert_eq!(path, "/audio/a.mp3");
                assert_eq!(status, 403);
                assert!(
                    body.contains("SignatureDoesNotMatch"),
                    "body summary: {body}"
                );
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn signature_is_deterministic_under_fixed_clock() {
        let a = store("https://my-bucket.oss-cn-hangzhou.aliyuncs.com").sign(
            "PUT",
            "/k",
            "md5\napplication/octet-stream",
            "Tue, 14 Nov 2023 22:13:20 GMT",
        );
        let b = store("https://my-bucket.oss-cn-hangzhou.aliyuncs.com").sign(
            "PUT",
            "/k",
            "md5\napplication/octet-stream",
            "Tue, 14 Nov 2023 22:13:20 GMT",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn presigned_url_matches_known_vector() {
        let store = store("https://my-bucket.oss-cn-hangzhou.aliyuncs.com");
        let url = store.presigned_url("audio/2026/gift a.mp3", std::time::Duration::from_secs(900));
        assert_eq!(
            url,
            "https://my-bucket.oss-cn-hangzhou.aliyuncs.com/audio/2026/gift%20a.mp3\
             ?OSSAccessKeyId=test-access-key&Expires=1700000900\
             &Signature=pv1Z77h7kOrtUBDD%2Bv%2BtsHjPvaA%3D"
        );
    }

    #[tokio::test]
    async fn get_signs_v1_known_vector_and_reads_origin_body() {
        let server = test_support::serve().await;
        server.set_response(200, b"<html>fresh</html>");
        let store = store(&server.base_url);

        let body = store
            .get_object("audio/2026/gift a.mp3")
            .await
            .expect("get");

        assert_eq!(body, b"<html>fresh</html>");
        let req = &server.requests()[0];
        assert_eq!(req.request_line, "GET /audio/2026/gift%20a.mp3 HTTP/1.1");
        assert_eq!(
            req.header("authorization"),
            Some("OSS test-access-key:BgwcyDKqXYmuv0jd0BSbdCo5A8E=")
        );
    }

    #[tokio::test]
    async fn get_403_is_rejected_with_body_summary() {
        let server = test_support::serve().await;
        server.set_response(403, b"<Error><Code>AccessDenied</Code></Error>");
        let store = store(&server.base_url);

        let err = store
            .get_object("audio/a.mp3")
            .await
            .expect_err("403 must fail");
        match err {
            ObjectStoreError::Rejected {
                method,
                path,
                status,
                body,
            } => {
                assert_eq!(method, "GET");
                assert_eq!(path, "/audio/a.mp3");
                assert_eq!(status, 403);
                assert!(body.contains("AccessDenied"), "body summary: {body}");
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }
}
