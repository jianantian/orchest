use async_trait::async_trait;
use orchest_protocol::{
    Capability, CapabilityDescriptor, Decision, DecisionRequest, DecisionResponse, ProtocolError,
};
use orchest_provider::{DecisionConfig, Entry, Registry};
use serde_json::json;

struct FakeDecision;

#[async_trait]
impl Decision for FakeDecision {
    fn provider_name(&self) -> &str {
        "local"
    }
    fn model_name(&self) -> &str {
        "rules"
    }
    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("local", "rules", Capability::Decision)
    }
    async fn decide(&self, request: DecisionRequest) -> Result<DecisionResponse, ProtocolError> {
        request.validate()?;
        assert_eq!(request.state, json!(true));
        Ok(serde_json::from_value(
            json!({"model":"rules","answers":{"q":{"type":"boolean","probability":1.0}}}),
        )
        .unwrap())
    }
}

#[tokio::test]
async fn custom_decision_works_without_http_credentials_or_usage() {
    let mut registry = Registry::new();
    registry.register_decision(Entry::new(FakeDecision.descriptor(), |_| {
        Ok(Box::new(FakeDecision) as Box<dyn Decision>)
    }));
    assert_eq!(registry.decision().list().len(), 1);
    let engine = registry
        .create_decision(&DecisionConfig::new("local/rules"))
        .unwrap();
    let request = serde_json::from_value(
        json!({"state":true,"questions":{"q":{"type":"boolean","instructions":"Enabled?"}}}),
    )
    .unwrap();
    let result = engine.decide(request).await.unwrap();
    assert_eq!(result.model, "rules");
    assert!(result.usage.is_none());
    assert!(registry.chat().list().is_empty());
}

#[test]
fn model_selection_is_explicit_and_configuration_debug_hides_secrets() {
    let mut config = DecisionConfig::new("");
    config.api_key = Some("private-key".into());
    assert!(!format!("{config:?}").contains("private-key"));
    assert!(Registry::new().create_decision(&config).is_err());
}

#[cfg(feature = "http")]
mod http {
    use super::*;
    use orchest_protocol::ErrorCode;
    use orchest_provider::{decide, find_model_for, ModelFilter};
    use serde_json::Value;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    fn request() -> DecisionRequest {
        serde_json::from_value(json!({"state":{"message":"Help! My payouts have been failing for 3 days."},"questions":{
            "urgent":{"type":"boolean","instructions":{"question":"Urgent?"},"criteria":{"true":"Time sensitive","false":"Not urgent"}},
            "team":{"type":"choice","instructions":"Which team?","criteria":{"billing":null,"tech":{"what":"Bugs"}}},
            "anger":{"type":"score","instructions":["How angry?"],"criteria":["Calm",{"level":"Frustrated"},"Angry"]}
        }})).unwrap()
    }

    fn response() -> Value {
        json!({"id":"decision-1","model":"typesafe/jev-1.13","provider":"TypeSafe","answers":{
            "urgent":{"type":"noul","noul":0.95},
            "team":{"type":"choice","choice":"billing","probabilities":{"billing":0.9,"tech":0.1},"confidence":0.8},
            "anger":{"type":"score","score":1.05}
        },"usage":{"input_tokens":310,"output_tokens":14,"cost":0.00001}})
    }

    fn server(
        body: String,
        status: &str,
        headers: &str,
        delay: Duration,
    ) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/api/alpha/decisions",
            listener.local_addr().unwrap()
        );
        let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len());
        let task = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let count = socket.read(&mut buffer).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                    let len = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    if request.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            thread::sleep(delay);
            let _ = socket.write_all(response.as_bytes());
            String::from_utf8(request).unwrap()
        });
        (url, task)
    }

    fn config(url: String) -> DecisionConfig {
        let mut config = DecisionConfig::new("openrouter/~typesafe/jev-latest");
        config.api_key = Some("test-key".into());
        config.api_url = Some(url);
        config
    }

    #[tokio::test]
    async fn maps_generic_batch_to_openrouter_and_preserves_answers() {
        let (url, server) = server(response().to_string(), "200 OK", "", Duration::ZERO);
        let result = decide(&config(url), request()).await.unwrap();
        let raw = server.join().unwrap();
        assert!(raw.starts_with("POST /api/alpha/decisions HTTP/1.1"));
        assert!(raw
            .to_lowercase()
            .contains("authorization: bearer test-key\r\n"));
        let wire: Value = serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(wire["model"], "~typesafe/jev-latest");
        assert_eq!(wire["questions"]["urgent"]["type"], "noul");
        assert_eq!(
            wire["questions"]["urgent"]["instructions"],
            json!({"question":"Urgent?"})
        );
        assert_eq!(wire["questions"].as_object().unwrap().len(), 3);
        let result = serde_json::to_value(result).unwrap();
        assert_eq!(
            result["answers"]["urgent"],
            json!({"type":"boolean","probability":0.95})
        );
        assert_eq!(result["answers"]["anger"]["score"], 1.05);
        assert!(result["answers"]["anger"].get("confidence").is_none());
        assert_eq!(result["usage"]["cost_usd"], 0.00001);
    }

    #[test]
    fn discovers_decision_models_and_pins_selected_factory() {
        let reg = Registry::with_builtin();
        let default = reg.decision().provider("openrouter").select().unwrap();
        assert_eq!(default.descriptor.model, "~typesafe/jev-latest");
        let fixed = reg
            .decision()
            .id("openrouter/typesafe/jev-1.13")
            .select()
            .unwrap();
        let cfg = orchest_provider::ProviderConfig::new("wrong", "wrong").with_api_key("test");
        let engine = fixed.instantiate(&cfg).unwrap();
        assert_eq!(engine.provider_name(), "openrouter");
        assert_eq!(engine.model_name(), "typesafe/jev-1.13");
        assert!(reg
            .chat()
            .id("openrouter/typesafe/jev-1.13")
            .list()
            .is_empty());
        assert!(find_model_for("openrouter/~typesafe/jev-latest", Capability::Decision).is_some());
        let rows: Vec<_> = orchest_provider::list_models(ModelFilter {
            capability: Some(Capability::Decision),
            ..Default::default()
        })
        .collect();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn errors_keep_http_status_retry_after_and_provider_identity() {
        for (status, code) in [
            ("401 Unauthorized", ErrorCode::InvalidApiKey),
            ("429 Too Many Requests", ErrorCode::ProviderHttpError),
            ("503 Unavailable", ErrorCode::ProviderHttpError),
        ] {
            let (url, server) = server(
                json!({"error":{"code":429,"message":"try later"}}).to_string(),
                status,
                "Retry-After: 7\r\n",
                Duration::ZERO,
            );
            let error = decide(&config(url), request()).await.unwrap_err();
            assert_eq!(error.code, code);
            assert_eq!(error.status.unwrap().to_string(), &status[..3]);
            assert_eq!(error.retry_after_secs, Some(7));
            assert_eq!(error.provider.as_deref(), Some("openrouter"));
            assert!(error.upstream.is_some());
            server.join().unwrap();
        }
    }

    #[tokio::test]
    async fn invalid_success_responses_are_not_successful_decisions() {
        let mut missing = response();
        missing["answers"].as_object_mut().unwrap().remove("urgent");
        let mut wrong_type = response();
        wrong_type["answers"]["urgent"] = json!({"type":"boolean","probability":0.95});
        let mut missing_usage = response();
        missing_usage.as_object_mut().unwrap().remove("usage");
        for body in [
            "not json".into(),
            missing.to_string(),
            wrong_type.to_string(),
            missing_usage.to_string(),
        ] {
            let (url, server) = server(body, "200 OK", "", Duration::ZERO);
            let error = decide(&config(url), request()).await.unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidResponse);
            assert_eq!(error.status, Some(200));
            server.join().unwrap();
        }
    }

    #[tokio::test]
    async fn rejects_bad_input_without_connecting_and_reports_timeout() {
        let mut req = request();
        req.state = Value::Null;
        let error = decide(&config("http://127.0.0.1:1/never".into()), req)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        let (url, server) = server(
            response().to_string(),
            "200 OK",
            "",
            Duration::from_millis(150),
        );
        let mut cfg = config(url);
        cfg.timeout_ms = Some(40);
        assert_eq!(
            decide(&cfg, request()).await.unwrap_err().code,
            ErrorCode::Timeout
        );
        server.join().unwrap();
    }

    #[test]
    fn missing_custom_key_env_never_falls_back() {
        let mut cfg = DecisionConfig::new("openrouter/~typesafe/jev-latest");
        cfg.api_key_env = Some("ORCHEST_DECISION_MISSING_KEY_7B4D19A".into());
        let error = match orchest_provider::create_decision(&cfg) {
            Ok(_) => panic!("expected missing key"),
            Err(e) => e,
        };
        assert_eq!(error.code, ErrorCode::MissingApiKey);
        cfg.api_key = Some("explicit-wins".into());
        assert!(orchest_provider::create_decision(&cfg).is_ok());
    }
}
