//! Credential-gated live example for Doubao / Volcengine realtime voice.
//!
//! Required environment variables:
//! - VOLCENGINE_APP_ID
//! - VOLCENGINE_ACCESS_TOKEN
//!
//! Optional environment variables are documented in
//! docs/iteration/v0_9_11/provider-decision.md.
//!
//! Run:
//! cargo run -p agent-runtime-realtime-providers --features volcengine \
//!   --example volcengine_realtime_live

use std::{env, fs, time::Duration};

use agent_runtime_realtime_providers::providers::volcengine::realtime::{
    VolcengineRealtimeConfig, VolcengineRealtimeEvent, VolcengineRealtimeMappedEvent,
    VolcengineRealtimeSession,
};

const PCM_CHUNK_BYTES: usize = 640;
const PCM_CHUNK_INTERVAL: Duration = Duration::from_millis(20);
const TRAILING_SILENCE_CHUNKS: usize = 100;
const PROVIDER_EVENT_IDLE_WAIT: Duration = Duration::from_secs(5);
const PROVIDER_EVENT_TOTAL_WAIT: Duration = Duration::from_secs(75);
const DEFAULT_TTS_OUTPUT_PATH: &str = "/tmp/orchest_realtime_live_tts_24k_s16le.pcm";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match VolcengineRealtimeConfig::from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("missing Volcengine realtime configuration: {err}");
            eprintln!(
                "set VOLCENGINE_APP_ID, VOLCENGINE_ACCESS_TOKEN and VOLCENGINE_REALTIME_RESOURCE_ID"
            );
            std::process::exit(2);
        }
    };

    println!("connecting to Volcengine realtime: {}", config.ws_url);
    let (mut session, mut events) = VolcengineRealtimeSession::connect_live(config).await?;
    println!("started realtime session: {}", session.session_id());
    if let Some(log_id) = session.handshake_log_id() {
        println!("Volcengine X-Tt-Logid: {log_id}");
    }

    let mut audio = load_pcm_input()?;
    append_trailing_silence(&mut audio);
    let mut sent = 0_usize;
    for chunk in audio.chunks(PCM_CHUNK_BYTES) {
        session.send_audio_chunk(chunk).await?;
        sent += chunk.len();
        tokio::time::sleep(PCM_CHUNK_INTERVAL).await;
    }
    println!("sent {sent} bytes of PCM audio");

    let (observations, tts_audio) = collect_provider_events(&mut events).await;
    println!("provider observations: {observations:?}");
    if !tts_audio.is_empty() {
        fs::write(DEFAULT_TTS_OUTPUT_PATH, &tts_audio)?;
        println!(
            "wrote {} bytes of PCM TTS audio to {}",
            tts_audio.len(),
            DEFAULT_TTS_OUTPUT_PATH
        );
    }

    session.close().await?;
    wait_for_session_finished(&mut events).await;
    session.finish_connection().await?;
    println!("closed realtime session and finished websocket connection");
    Ok(())
}

#[derive(Debug, Default)]
struct ProviderObservations {
    transcript_events: usize,
    final_transcript_events: usize,
    model_text_events: usize,
    audio_output_bytes: usize,
    saw_asr_started: bool,
    saw_asr_ended: bool,
    saw_chat_ended: bool,
    saw_tts_ended: bool,
    saw_provider_error: bool,
}

impl ProviderObservations {
    fn record(&mut self, event: &VolcengineRealtimeEvent) {
        match event {
            VolcengineRealtimeEvent::ProviderError { .. } => {
                self.saw_provider_error = true;
            }
            VolcengineRealtimeEvent::ServerEvent { event } => match event {
                VolcengineRealtimeMappedEvent::Lifecycle { event_id: 359, .. } => {
                    self.saw_tts_ended = true;
                }
                VolcengineRealtimeMappedEvent::Lifecycle { event_id: 459, .. } => {
                    self.saw_asr_ended = true;
                }
                VolcengineRealtimeMappedEvent::Lifecycle { event_id: 559, .. } => {
                    self.saw_chat_ended = true;
                }
                VolcengineRealtimeMappedEvent::Metadata { event_id: 450, .. } => {
                    self.saw_asr_started = true;
                }
                VolcengineRealtimeMappedEvent::Transcript { is_interim, .. } => {
                    self.transcript_events += 1;
                    if !is_interim {
                        self.final_transcript_events += 1;
                    }
                }
                VolcengineRealtimeMappedEvent::ModelText { .. } => {
                    self.model_text_events += 1;
                }
                VolcengineRealtimeMappedEvent::AudioOutput { bytes, .. } => {
                    self.audio_output_bytes += bytes.len();
                }
                VolcengineRealtimeMappedEvent::Error { .. } => {
                    self.saw_provider_error = true;
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.saw_provider_error || self.saw_tts_ended
    }
}

fn load_pcm_input() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if let Some(path) = env::args().nth(1) {
        println!("loading PCM input from {path}");
        return Ok(fs::read(path)?);
    }

    println!("no PCM path provided; using checked-in 20ms silence fixture");
    Ok(include_bytes!(
        "../../../docs/archive/iteration/v0_9_11/fixtures/silence_20ms_16k_s16le.pcm"
    )
    .to_vec())
}

fn append_trailing_silence(audio: &mut Vec<u8>) {
    let silence_bytes = PCM_CHUNK_BYTES * TRAILING_SILENCE_CHUNKS;
    audio.resize(audio.len() + silence_bytes, 0);
    println!(
        "appended {} ms of trailing PCM silence",
        TRAILING_SILENCE_CHUNKS * 20
    );
}

async fn collect_provider_events(
    events: &mut tokio::sync::mpsc::Receiver<VolcengineRealtimeEvent>,
) -> (ProviderObservations, Vec<u8>) {
    let mut observations = ProviderObservations::default();
    let mut tts_audio = Vec::new();
    let started = tokio::time::Instant::now();
    let mut index = 0_usize;

    while started.elapsed() < PROVIDER_EVENT_TOTAL_WAIT {
        match tokio::time::timeout(PROVIDER_EVENT_IDLE_WAIT, events.recv()).await {
            Ok(Some(event)) => {
                observations.record(&event);
                if let VolcengineRealtimeEvent::ServerEvent {
                    event: VolcengineRealtimeMappedEvent::AudioOutput { bytes, .. },
                } = &event
                {
                    tts_audio.extend_from_slice(bytes);
                }
                print_provider_event(index, &event);
                index += 1;
                if observations.is_complete() {
                    break;
                }
            }
            Ok(None) => {
                println!("event stream closed by provider");
                break;
            }
            Err(_) => {
                println!(
                    "idle waiting for provider event after {} seconds",
                    started.elapsed().as_secs()
                );
            }
        }
    }

    (observations, tts_audio)
}

fn print_provider_event(index: usize, event: &VolcengineRealtimeEvent) {
    match event {
        VolcengineRealtimeEvent::Handshake { log_id } => {
            println!("event[{index}] Handshake log_id={log_id:?}");
        }
        VolcengineRealtimeEvent::ServerEvent { event } => match event {
            VolcengineRealtimeMappedEvent::Lifecycle { event_id, name } => {
                println!("event[{index}] {name}({event_id})");
            }
            VolcengineRealtimeMappedEvent::AudioOutput { event_id, bytes } => {
                println!(
                    "event[{index}] TTSResponse({event_id}) bytes={}",
                    bytes.len()
                );
            }
            VolcengineRealtimeMappedEvent::Transcript {
                event_id,
                text,
                is_interim,
            } => {
                println!(
                    "event[{index}] ASRResponse({event_id}) interim={is_interim} text={text:?}"
                );
            }
            VolcengineRealtimeMappedEvent::ModelText { event_id, content } => {
                println!("event[{index}] ChatResponse({event_id}) content={content:?}");
            }
            VolcengineRealtimeMappedEvent::Error { event_id, message } => {
                println!("event[{index}] ProviderError({event_id}) message={message}");
            }
            VolcengineRealtimeMappedEvent::Metadata {
                event_id,
                name,
                payload,
            } => {
                println!("event[{index}] {name}({event_id}) payload={payload}");
            }
            VolcengineRealtimeMappedEvent::Unsupported {
                event_id,
                name,
                reason,
            } => {
                println!("event[{index}] Unsupported {name}({event_id}) reason={reason}");
            }
        },
        other => println!("event[{index}] {other:?}"),
    }
}

async fn wait_for_session_finished(
    events: &mut tokio::sync::mpsc::Receiver<VolcengineRealtimeEvent>,
) {
    for index in 0..8 {
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Some(VolcengineRealtimeEvent::ServerEvent {
                event: VolcengineRealtimeMappedEvent::Lifecycle { event_id: 152, .. },
            })) => {
                println!("received SessionFinished");
                return;
            }
            Ok(Some(event)) => print_provider_event(index, &event),
            Ok(None) => {
                println!("event stream closed while waiting for SessionFinished");
                return;
            }
            Err(_) => {
                println!("timed out waiting for SessionFinished");
                return;
            }
        }
    }
}
