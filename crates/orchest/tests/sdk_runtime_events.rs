use std::sync::Arc;

use orchest::bindings::runtime_event_to_wire_value;
use orchest::model::ModelAdapter;
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::registry::ToolRegistry;
use orchest_provider::{create_adapter_from_config, ProviderRuntimeConfig};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[tokio::test]
async fn sdk_fixture_matches_rust_primary_stream() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/sdk-runtime-events.json")).unwrap();
    let response: String = fixture["complete"]
        .as_array()
        .unwrap()
        .iter()
        .map(|chunk| {
            format!(
                "event: {}\ndata: {}\n\n",
                chunk[0].as_str().unwrap(),
                chunk[1]
            )
        })
        .collect();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut client = BufReader::new(socket);
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert_ne!(client.read_line(&mut line).await.unwrap(), 0);
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
        }
        client.read_exact(&mut vec![0; length]).await.unwrap();
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.len()
        );
        client.write_all(headers.as_bytes()).await.unwrap();
        client.write_all(response.as_bytes()).await.unwrap();
    });
    let name = "anthropic/claude-sonnet-4-6";
    let model: Arc<dyn ModelAdapter> = Arc::from(
        create_adapter_from_config(ProviderRuntimeConfig {
            model: name.into(),
            api_key: Some("local-test".into()),
            api_key_env: None,
            api_url: Some(url),
            max_tokens: None,
        })
        .unwrap(),
    );
    let config = AgentConfig::builder("async-run-test", name)
        .system_prompt("Answer briefly.")
        .build()
        .unwrap();
    let (run, mut rx) = AgentRun::start(config, "fixture".into(), model, ToolRegistry::new());
    let mut actual = Vec::new();
    while let Some(event) = rx.recv().await {
        let mut event = runtime_event_to_wire_value(&event).unwrap();
        for key in ["run_id", "message_id"] {
            if event[key].is_string() {
                event[key] = format!("<{key}>").into();
            }
        }
        actual.push(event);
    }
    run.wait().await;
    server.await.unwrap();
    assert_eq!(Value::Array(actual), fixture["expected"]);
}
