//! Credential-gated live example for Doubao / Volcengine realtime voice.
//!
//! Required environment variables:
//! - VOLCENGINE_REALTIME_APP_ID
//! - VOLCENGINE_REALTIME_ACCESS_KEY
//!
//! Optional environment variables are documented in
//! docs/iteration/v0_9_11/provider-decision.md.
//!
//! Run:
//! cargo run -p agent-runtime-asr-providers --features volcengine \
//!   --example volcengine_realtime_live

use std::time::Duration;

use agent_runtime_asr_providers::providers::volcengine::realtime::{
    VolcengineRealtimeConfig, VolcengineRealtimeEvent, VolcengineRealtimeSession,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match VolcengineRealtimeConfig::from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("missing Volcengine realtime configuration: {err}");
            eprintln!("set VOLCENGINE_REALTIME_APP_ID and VOLCENGINE_REALTIME_ACCESS_KEY");
            std::process::exit(2);
        }
    };

    println!("connecting to Volcengine realtime: {}", config.ws_url);
    let (mut session, mut events) = VolcengineRealtimeSession::connect_live(config).await?;
    println!("started realtime session: {}", session.session_id());

    let audio =
        include_bytes!("../../../docs/iteration/v0_9_11/fixtures/silence_20ms_16k_s16le.pcm");
    session.send_audio_chunk(audio).await?;
    println!("sent {} bytes of fixture PCM audio", audio.len());

    for index in 0..8 {
        match tokio::time::timeout(Duration::from_secs(10), events.recv()).await {
            Ok(Some(VolcengineRealtimeEvent::ServerEvent { event })) => {
                println!("event[{index}] {event:?}");
            }
            Ok(Some(event)) => {
                println!("event[{index}] {event:?}");
            }
            Ok(None) => {
                println!("event stream closed by provider");
                break;
            }
            Err(_) => {
                println!("timed out waiting for provider event");
                break;
            }
        }
    }

    session.close().await?;
    println!("closed realtime session");
    Ok(())
}
