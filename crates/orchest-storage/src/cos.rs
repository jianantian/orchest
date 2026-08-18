//! Tencent COS dialect — V5 `q-sign-algorithm=sha1` signature.
//!
//! Signs only the `host` header, which covers PUT/DELETE of objects without
//! query parameters (docs/todo/object-storage.md). The request host is
//! `{bucket}.cos.{region}.myqcloud.com`, assembled from the config `endpoint`
//! (the COS region, e.g. `ap-guangzhou`).
//!
//! Derivation (Tencent COS V5, tencentcloud.com/document/product/436/7778):
//! `SignKey = hex(HMAC-SHA1(SK, KeyTime))`,
//! `HttpString = method\npath\nparams\nheaders\n`,
//! `StringToSign = sha1\nKeyTime\nhex(SHA1(HttpString))\n`,
//! `Signature = hex(HMAC-SHA1(SignKey, StringToSign))`.

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha1::{Digest, Sha1};

use super::{
    encode_key, encode_query_value, execute, fetch, ObjectStore, ObjectStoreConfig,
    ObjectStoreError,
};

type HmacSha1 = Hmac<Sha1>;

/// Signature validity window in seconds — the request must arrive within
/// q-sign-time. 10 minutes matches COS SDK defaults.
const SIGN_VALIDITY_SECS: i64 = 600;
pub(crate) struct CosStore {
    access_key_id: String,
    access_key_secret: String,
    /// `{bucket}.cos.{region}.myqcloud.com` — the value signed as `host`.
    host: String,
    public_base: String,
    /// Actual HTTP base for requests: the vendor host by default, overridable
    /// to a loopback server in tests. The signature always covers the vendor
    /// `host` header value, never the transport host.
    transport_base: String,
    now: fn() -> i64,
}

impl CosStore {
    pub(crate) fn new(config: ObjectStoreConfig, transport_base: String, now: fn() -> i64) -> Self {
        Self {
            access_key_id: config.access_key_id,
            access_key_secret: config.access_key_secret,
            host: format!("{}.cos.{}.myqcloud.com", config.bucket, config.endpoint),
            public_base: config.public_base.trim_end_matches('/').to_string(),
            transport_base,
            now,
        }
    }

    /// `start;end` epoch-seconds window for a signature with `validity_secs`.
    fn key_time(&self, validity_secs: i64) -> String {
        let start = (self.now)();
        format!("{start};{}", start + validity_secs)
    }

    /// The hex signature for `method` (lowercase) against `path`
    /// (percent-encoded, leading slash) over the `key_time` window. No query
    /// params are signed — matches the no-query scope.
    fn sign(&self, method: &str, path: &str, key_time: &str) -> String {
        let sign_key = hmac_hex(self.access_key_secret.as_bytes(), key_time.as_bytes());
        let http_string = format!("{method}\n{path}\n\nhost={}\n", self.host);
        let sha1_http = hex::encode(Sha1::digest(http_string.as_bytes()));
        let string_to_sign = format!("sha1\n{key_time}\n{sha1_http}\n");
        hmac_hex(sign_key.as_bytes(), string_to_sign.as_bytes())
    }

    /// The full COS V5 `Authorization` header value for `method` (lowercase)
    /// against `path` (percent-encoded, leading slash).
    fn authorization(&self, method: &str, path: &str) -> String {
        let key_time = self.key_time(SIGN_VALIDITY_SECS);
        let signature = self.sign(method, path, &key_time);
        format!(
            "q-sign-algorithm=sha1&q-ak={ak}&q-sign-time={key_time}&q-key-time={key_time}\
             &q-header-list=host&q-url-param-list=&q-signature={signature}",
            ak = self.access_key_id
        )
    }
}

/// `hex(HMAC-SHA1(key, message))` — lowercase, per COS.
fn hmac_hex(key: &[u8], message: &[u8]) -> String {
    let mut mac = HmacSha1::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(message);
    hex::encode(mac.finalize().into_bytes())
}

#[async_trait]
impl ObjectStore for CosStore {
    async fn put_object(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<(), ObjectStoreError> {
        let path = format!("/{}", encode_key(key));
        let request = crate::client()
            .put(format!("{}{}", self.transport_base, path))
            .header("Content-Type", content_type)
            .header("Authorization", self.authorization("put", &path))
            .body(bytes);
        execute("PUT", &path, request, false).await
    }

    async fn get_object(&self, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let path = format!("/{}", encode_key(key));
        let request = crate::client()
            .get(format!("{}{}", self.transport_base, path))
            .header("Authorization", self.authorization("get", &path));
        fetch("GET", &path, request).await
    }

    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError> {
        let path = format!("/{}", encode_key(key));
        let request = crate::client()
            .delete(format!("{}{}", self.transport_base, path))
            .header("Authorization", self.authorization("delete", &path));
        execute("DELETE", &path, request, true).await
    }

    fn presigned_url(&self, key: &str, ttl: std::time::Duration) -> String {
        let path = format!("/{}", encode_key(key));
        let key_time = self.key_time(ttl.as_secs() as i64);
        let signature = self.sign("get", &path, &key_time);
        let encoded_key_time = encode_query_value(&key_time);
        format!(
            "https://{host}{path}?q-sign-algorithm=sha1&q-ak={ak}&q-sign-time={kt}\
             &q-key-time={kt}&q-header-list=host&q-url-param-list=&q-signature={signature}",
            host = self.host,
            ak = encode_query_value(&self.access_key_id),
            kt = encoded_key_time,
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

    fn store(base_url: &str) -> CosStore {
        CosStore::new(
            ObjectStoreConfig {
                dialect: ObjectStoreDialect::TencentCos,
                access_key_id: "test-access-key".to_string(),
                access_key_secret: "test-secret-key".to_string(),
                bucket: "my-bucket".to_string(),
                endpoint: "ap-guangzhou".to_string(),
                public_base: "https://cdn.example.com".to_string(),
            },
            base_url.to_string(),
            FIXED_NOW,
        )
    }

    #[tokio::test]
    async fn put_signs_v5_known_vector_and_sends_shape() {
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
        assert_eq!(
            req.header("authorization"),
            Some(
                "q-sign-algorithm=sha1&q-ak=test-access-key\
                 &q-sign-time=1700000000;1700000600&q-key-time=1700000000;1700000600\
                 &q-header-list=host&q-url-param-list=\
                 &q-signature=2d0a3f77f97927049ed0e895b693513aea8401ca"
            )
        );
    }

    #[tokio::test]
    async fn delete_signs_v5_known_vector() {
        let server = test_support::serve().await;
        let store = store(&server.base_url);

        store
            .delete_object("audio/2026/gift a.mp3")
            .await
            .expect("delete");

        let req = &server.requests()[0];
        assert_eq!(req.request_line, "DELETE /audio/2026/gift%20a.mp3 HTTP/1.1");
        let auth = req.header("authorization").expect("authorization header");
        assert!(
            auth.ends_with("q-signature=5f6f9ed12eff6e7627f27e6fea2e55c3aa017253"),
            "{auth}"
        );
    }

    #[tokio::test]
    async fn delete_is_idempotent_on_404() {
        let server = test_support::serve().await;
        server.set_response(404, b"<?xml?><Error><Code>NoSuchKey</Code></Error>");
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
            .delete_object("audio/a.mp3")
            .await
            .expect_err("403 must fail");
        match err {
            ObjectStoreError::Rejected {
                method,
                path,
                status,
                body,
            } => {
                assert_eq!(method, "DELETE");
                assert_eq!(path, "/audio/a.mp3");
                assert_eq!(status.as_u16(), 403);
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
        let a = store("https://my-bucket.cos.ap-guangzhou.myqcloud.com").authorization("put", "/k");
        let b = store("https://my-bucket.cos.ap-guangzhou.myqcloud.com").authorization("put", "/k");
        assert_eq!(a, b);
    }

    #[test]
    fn presigned_url_matches_known_vector() {
        let store = store("https://my-bucket.cos.ap-guangzhou.myqcloud.com");
        let url = store.presigned_url("audio/2026/gift a.mp3", std::time::Duration::from_secs(900));
        assert_eq!(
            url,
            "https://my-bucket.cos.ap-guangzhou.myqcloud.com/audio/2026/gift%20a.mp3\
             ?q-sign-algorithm=sha1&q-ak=test-access-key\
             &q-sign-time=1700000000%3B1700000900&q-key-time=1700000000%3B1700000900\
             &q-header-list=host&q-url-param-list=\
             &q-signature=d83185f061502f2cb7d1238cb47f68fec8462e61"
        );
    }

    #[tokio::test]
    async fn get_signs_v5_known_vector_and_reads_origin_body() {
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
            Some(
                "q-sign-algorithm=sha1&q-ak=test-access-key\
                 &q-sign-time=1700000000;1700000600&q-key-time=1700000000;1700000600\
                 &q-header-list=host&q-url-param-list=\
                 &q-signature=ab50ea68cfc22f5008fa2bea39f3d92fa9a97e8c"
            )
        );
    }

    #[tokio::test]
    async fn get_404_is_rejected_not_idempotent() {
        let server = test_support::serve().await;
        server.set_response(404, b"<?xml?><Error><Code>NoSuchKey</Code></Error>");
        let store = store(&server.base_url);

        let err = store
            .get_object("missing.mp3")
            .await
            .expect_err("reads are not idempotent");
        match err {
            ObjectStoreError::Rejected { status, body, .. } => {
                assert_eq!(status.as_u16(), 404);
                assert!(body.contains("NoSuchKey"), "body summary: {body}");
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }
}
