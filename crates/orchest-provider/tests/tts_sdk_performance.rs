#![cfg(feature = "stream")]

use orchest_protocol::StreamEvent;
use orchest_provider::tts::TtsOptions;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// Set ORCHEST_TTS_FIXTURE to the JSON startup line from sdk-tts-server.py.
#[tokio::test]
#[ignore = "uses the shared local TTS fixture for repeatable measurements"]
async fn tts_stream_fixture_load() {
    let fixture: Value =
        serde_json::from_str(&std::env::var("ORCHEST_TTS_FIXTURE").unwrap()).unwrap();
    for pause in [0, 100].into_iter().cycle().take(6) {
        let options = TtsOptions::parse(json!({
            "provider": "minimax", "voice": "fixture", "format": "mp3",
            "apiKey": "fixture-key", "apiUrl": format!("{}/one-shot", fixture["endpoint"].as_str().unwrap()),
            "providerOptions": {"ca_cert_pem": fixture["ca"]},
        })).unwrap();
        let start = Instant::now();
        let mut stream = options.stream_synthesize("benchmark".into()).await.unwrap();
        let mut first = None;
        let mut total = 0;
        let mut peak = 0;
        while let Some(event) = stream.next().await {
            if let StreamEvent::AudioDelta { data, .. } = event {
                if first.is_none() {
                    first = Some(start.elapsed());
                    tokio::time::sleep(Duration::from_millis(pause)).await;
                }
                total += data.len();
                peak = peak.max(stream.queued_events());
            }
        }
        assert_eq!(total, 128 * 4096);
        println!(
            "Rust pause_ms={pause} first_ms={:.2} total_ms={:.2} peak_queue={peak}",
            first.unwrap().as_secs_f64() * 1000.0,
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
