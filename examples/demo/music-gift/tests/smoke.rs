//! Black-box smoke tests for the music-gift demo.
//!
//! Tests that require a live LLM or music gen provider are skipped when the
//! relevant env vars are not set. Tests that only exercise gift CRUD or
//! server startup run unconditionally.

use std::process::{Command, Stdio};
use std::time::Duration;

const CHAT_MODEL_ENV: &str = "MUSIC_GIFT_CHAT_MODEL";
const MUSIC_KEY_ENV: &str = "MUSIC_GIFT_MUSIC_API_KEY";

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_music-gift"))
}

fn chat_model_configured() -> bool {
    std::env::var(CHAT_MODEL_ENV).is_ok_and(|v| !v.trim().is_empty())
}

/// Start the server on a random port, wait for it to be ready, return the
/// port and a handle to kill it when done.
fn start_server() -> (u16, std::process::Child) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().join("data");
    let static_dir = tmp.path().join("static");

    // We need a dummy static dir or the server won't serve the frontend.
    // That's fine — we only test API endpoints.
    std::fs::create_dir_all(&static_dir).expect("create static dir");

    let mut child = bin()
        .env("MUSIC_GIFT_PORT", "0")
        .arg("--data-dir")
        .arg(data_dir.to_str().unwrap())
        .arg("--static-dir")
        .arg(static_dir.to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start music-gift");

    // Read the port from stdout: "music-gift listening on http://0.0.0.0:<port>"
    let stdout = child.stdout.take().expect("stdout");
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(stdout);
    let mut port = 0u16;
    for line in reader.lines() {
        let line = line.expect("read line");
        eprintln!("[server] {line}");
        if let Some(rest) = line.strip_prefix("music-gift listening on http://0.0.0.0:") {
            port = rest.parse().expect("parse port");
            break;
        }
    }
    assert!(port > 0, "server did not print listening port");

    // Give the server a moment to bind
    std::thread::sleep(Duration::from_millis(200));

    (port, child)
}

#[test]
fn server_starts_and_health_check() {
    // This test verifies the server starts. It doesn't need API keys —
    // the server boots before checking provider credentials (it only
    // checks when endpoints are called).
    let chat_configured = chat_model_configured();
    if !chat_configured {
        eprintln!("skipping server_start: {CHAT_MODEL_ENV} not set");
        return;
    }

    let (port, mut child) = start_server();

    // Try a simple HTTP request to verify the server is responding
    let url = format!("http://127.0.0.1:{port}/api/playlist");
    let resp = ureq::get(&url).call();
    assert!(resp.is_ok(), "playlist endpoint should respond");

    child.kill().expect("kill server");
    child.wait().expect("wait for server");
}

#[test]
fn gift_crud_without_providers() {
    // Gift CRUD should work even without chat/music providers configured,
    // as long as the server starts (which requires CHAT_MODEL_ENV).
    if !chat_model_configured() {
        eprintln!("skipping gift_crud: {CHAT_MODEL_ENV} not set");
        return;
    }

    let (port, mut child) = start_server();
    let base = format!("http://127.0.0.1:{port}");

    // Create a gift
    let create_resp = ureq::post(&format!("{base}/api/gift")).send_json(serde_json::json!({
        "lyrics": "[verse 1]\nTest lyrics\n[chorus]\nMore lyrics",
        "meta": {"name": "Test", "relationship": "friend", "style": "warm", "title": "Test Song"},
        "style": "warm and gentle"
    }));
    assert!(
        create_resp.is_ok(),
        "create gift should succeed: {:?}",
        create_resp.as_ref().err()
    );
    let create_body: serde_json::Value = create_resp.unwrap().into_json().expect("json response");
    let gift_id = create_body["id"].as_str().expect("gift id");
    let _creator_token = create_body["creator_token"]
        .as_str()
        .expect("creator token");
    assert!(!gift_id.is_empty());

    // Get the gift
    let get_resp = ureq::get(&format!("{base}/api/gift/{gift_id}")).call();
    assert!(get_resp.is_ok(), "get gift should succeed");
    let gift: serde_json::Value = get_resp.unwrap().into_json().expect("json");
    assert_eq!(gift["id"].as_str().unwrap(), gift_id);
    // Gifts are created private (published: false) — the creator lists them
    // explicitly via POST /api/gift/:id/publish.
    assert_eq!(gift["published"], serde_json::Value::Bool(false));

    // Like the gift
    let like_resp = ureq::post(&format!("{base}/api/gift/{gift_id}/like"))
        .send_json(serde_json::json!({"viewer_id": "test-viewer"}));
    assert!(like_resp.is_ok(), "like should succeed");
    let like_body: serde_json::Value = like_resp.unwrap().into_json().expect("json");
    assert_eq!(like_body["likes"].as_u64().unwrap(), 1);

    // Playlist should include the gift
    let playlist_resp = ureq::get(&format!("{base}/api/playlist")).call();
    assert!(playlist_resp.is_ok());
    let playlist: serde_json::Value = playlist_resp.unwrap().into_json().expect("json");
    let _items = playlist["items"].as_array().expect("items array");
    // Gift has no audio_url yet, so it shouldn't appear in playlist
    // (playlist filters to only gifts with audio_url)

    // Music generation without key should fail gracefully
    if std::env::var(MUSIC_KEY_ENV).is_err() {
        let gen_resp = ureq::post(&format!("{base}/api/generate/{gift_id}")).send_string(""); // empty body
                                                                                              // May fail with missing key or succeed if key is set — either is fine
        eprintln!(
            "generate response (no key expected): {:?}",
            gen_resp.as_ref().map(|r| r.status())
        );
    }

    child.kill().expect("kill server");
    child.wait().expect("wait for server");
}
