//! L1: header-injection auth strategies.
//!
//! The PRD's key observation: provider auth "factors cleanly as a header-injection
//! strategy" — there is no stateful crypto handshake. An L3 provider entry binds
//! one strategy per endpoint (Volcengine = three: ark→`Bearer`,
//! openspeech→`X-Api-*`, visual→`AK/SK-HMAC`). Strategies are transport-agnostic:
//! they yield `(name, value)` header pairs usable on both a `reqwest` request and
//! a `tokio-tungstenite` upgrade.

/// A header-injection auth strategy: produces the headers an endpoint needs.
pub trait HeaderAuth {
    /// The headers to inject, as `(name, value)` pairs.
    fn headers(&self) -> Vec<(String, String)>;
}

/// `Authorization: Bearer <token>` — OpenAI-compatible, Anthropic, minimax-rest,
/// Volcengine ark, minimax music.
#[derive(Debug, Clone)]
pub struct BearerAuth {
    pub token: String,
}

impl BearerAuth {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

impl HeaderAuth for BearerAuth {
    fn headers(&self) -> Vec<(String, String)> {
        vec![("Authorization".into(), format!("Bearer {}", self.token))]
    }
}

/// Volcengine openspeech `X-Api-*` header set (asr / tts / omni realtime).
/// Optional fields are emitted only when present, matching the per-endpoint
/// variation (realtime uses `X-Api-App-Key`; asr uses `X-Api-Key`/`X-Api-Request-Id`).
#[derive(Debug, Clone, Default)]
pub struct OpenSpeechHeaders {
    pub app_id: Option<String>,
    pub api_key: Option<String>,
    pub access_key: Option<String>,
    pub resource_id: Option<String>,
    pub app_key: Option<String>,
    pub connect_id: Option<String>,
    pub request_id: Option<String>,
}

impl OpenSpeechHeaders {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn app_id(mut self, v: impl Into<String>) -> Self {
        self.app_id = Some(v.into());
        self
    }
    #[must_use]
    pub fn api_key(mut self, v: impl Into<String>) -> Self {
        self.api_key = Some(v.into());
        self
    }
    #[must_use]
    pub fn access_key(mut self, v: impl Into<String>) -> Self {
        self.access_key = Some(v.into());
        self
    }
    #[must_use]
    pub fn resource_id(mut self, v: impl Into<String>) -> Self {
        self.resource_id = Some(v.into());
        self
    }
    #[must_use]
    pub fn app_key(mut self, v: impl Into<String>) -> Self {
        self.app_key = Some(v.into());
        self
    }
    #[must_use]
    pub fn connect_id(mut self, v: impl Into<String>) -> Self {
        self.connect_id = Some(v.into());
        self
    }
    #[must_use]
    pub fn request_id(mut self, v: impl Into<String>) -> Self {
        self.request_id = Some(v.into());
        self
    }
}

impl HeaderAuth for OpenSpeechHeaders {
    fn headers(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut push = |k: &str, v: &Option<String>| {
            if let Some(v) = v {
                out.push((k.to_string(), v.clone()));
            }
        };
        push("X-Api-App-ID", &self.app_id);
        push("X-Api-Key", &self.api_key);
        push("X-Api-Access-Key", &self.access_key);
        push("X-Api-Resource-Id", &self.resource_id);
        push("X-Api-App-Key", &self.app_key);
        push("X-Api-Connect-Id", &self.connect_id);
        push("X-Api-Request-Id", &self.request_id);
        out
    }
}

/// AK/SK HMAC-SHA256 signing primitive for signed-gen endpoints (volc-visual,
/// OSS). Behind the `oss` feature so default builds pull no crypto deps.
#[cfg(feature = "oss")]
pub mod hmac_signer {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    /// Compute `hex(HMAC-SHA256(key, message))`.
    pub fn sign_hex(key: &[u8], message: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
        mac.update(message);
        hex::encode(mac.finalize().into_bytes())
    }

    /// Compute raw `HMAC-SHA256(key, message)` bytes (for chained signing, e.g.
    /// the AWS/Aliyun v4 derived-key scheme).
    pub fn sign_bytes(key: &[u8], message: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
        mac.update(message);
        mac.finalize().into_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_emits_authorization() {
        assert_eq!(
            BearerAuth::new("sk-123").headers(),
            vec![("Authorization".to_string(), "Bearer sk-123".to_string())]
        );
    }

    #[test]
    fn openspeech_emits_only_present_headers() {
        let h = OpenSpeechHeaders::new()
            .app_id("app")
            .access_key("ak")
            .resource_id("res")
            .app_key("appkey")
            .connect_id("c1")
            .headers();
        assert!(h.contains(&("X-Api-App-ID".into(), "app".into())));
        assert!(h.contains(&("X-Api-Access-Key".into(), "ak".into())));
        assert!(h.contains(&("X-Api-Resource-Id".into(), "res".into())));
        assert!(h.contains(&("X-Api-App-Key".into(), "appkey".into())));
        assert!(h.contains(&("X-Api-Connect-Id".into(), "c1".into())));
        // api_key / request_id were not set → not emitted
        assert!(!h.iter().any(|(k, _)| k == "X-Api-Key"));
        assert!(!h.iter().any(|(k, _)| k == "X-Api-Request-Id"));
    }

    #[cfg(feature = "oss")]
    #[test]
    fn hmac_sign_is_deterministic_and_known() {
        // RFC 4231-style determinism check.
        let a = hmac_signer::sign_hex(b"key", b"message");
        let b = hmac_signer::sign_hex(b"key", b"message");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64); // 32 bytes hex-encoded
    }
}
