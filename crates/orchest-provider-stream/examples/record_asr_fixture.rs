//! Record a live streaming-ASR session as a contract-test fixture
//! (`tests/fixtures/asr/<dialect>/*.jsonl`, see `tests/asr_contract.rs`).
//!
//! ```text
//! DEEPGRAM_API_KEY=… cargo run -p orchest-provider-stream --features record-fixtures \
//!     --example record_asr_fixture -- --dialect deepgram --audio hello.wav \
//!     --finalize-at-ms 2200 --out tests/fixtures/asr/deepgram/hello.jsonl
//! ```
//!
//! The audio is 16 kHz mono PCM s16le, raw or in a WAV file, streamed in
//! `--chunk-ms` chunks at real-time pace. `--finalize-at-ms` sends
//! `SessionInput::Finalize` once that much audio has been sent. The key comes
//! from `--key-env` (default per dialect below); `--api-url` and repeated
//! `--option key=value` fill the rest of the `ProviderConfig`. The fixture
//! keeps input sizes and frame kinds, never audio or uplink payloads, and
//! redacts downlink identifiers.
//!
//! Use a recording with no personal information: downlink transcripts are
//! stored as-is.

use std::time::Duration;

use bytes::Bytes;
use orchest_protocol::{Asr, AudioFormat, SessionInput, StreamingTranscribeRequest};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_stream::asr::{aliyun, deepgram, elevenlabs, soniox, volcengine};
use orchest_provider_stream::fixture::{
    install_recorder, record_input, render_fixture, FixtureInput, FixtureLine,
};
use serde_json::{json, Value};

const SAMPLE_RATE: usize = 16_000;

fn default_key_env(dialect: &str) -> &'static str {
    match dialect {
        "deepgram" => "DEEPGRAM_API_KEY",
        "elevenlabs" => "ELEVENLABS_API_KEY",
        "soniox" => "SONIOX_API_KEY",
        "volcengine" => "VOLCENGINE_ASR_API_KEY",
        _ => "DASHSCOPE_API_KEY",
    }
}

struct Args {
    dialect: String,
    model: String,
    audio: String,
    out: String,
    key_env: Option<String>,
    api_url: Option<String>,
    options: serde_json::Map<String, Value>,
    finalize_at_ms: Option<u64>,
    chunk_ms: u64,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        dialect: String::new(),
        model: String::new(),
        audio: String::new(),
        out: String::new(),
        key_env: None,
        api_url: None,
        options: serde_json::Map::new(),
        finalize_at_ms: None,
        chunk_ms: 100,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        let mut value = || iter.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--dialect" => args.dialect = value()?,
            "--model" => args.model = value()?,
            "--audio" => args.audio = value()?,
            "--out" => args.out = value()?,
            "--key-env" => args.key_env = Some(value()?),
            "--api-url" => args.api_url = Some(value()?),
            "--option" => {
                let pair = value()?;
                let (key, raw) = pair.split_once('=').ok_or("--option takes key=value")?;
                let parsed = serde_json::from_str(raw).unwrap_or_else(|_| json!(raw));
                args.options.insert(key.into(), parsed);
            }
            "--finalize-at-ms" => {
                args.finalize_at_ms = Some(value()?.parse().map_err(|e| format!("{e}"))?)
            }
            "--chunk-ms" => args.chunk_ms = value()?.parse().map_err(|e| format!("{e}"))?,
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if args.dialect.is_empty() || args.audio.is_empty() || args.out.is_empty() {
        return Err("--dialect, --audio and --out are required".into());
    }
    Ok(args)
}

async fn read_pcm(path: &str) -> Result<Vec<u8>, String> {
    let data = tokio::fs::read(path)
        .await
        .map_err(|e| format!("{path}: {e}"))?;
    if !data.starts_with(b"RIFF") {
        return Ok(data);
    }
    // Skip WAV chunks up to `data`.
    let mut offset = 12;
    while offset + 8 <= data.len() {
        let size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if &data[offset..offset + 4] == b"data" {
            let end = (offset + 8 + size).min(data.len());
            return Ok(data[offset + 8..end].to_vec());
        }
        offset += 8 + size + (size & 1);
    }
    Err(format!("{path}: no WAV data chunk"))
}

fn build_asr(args: &Args, config: &ProviderConfig) -> Result<Box<dyn Asr>, String> {
    let built: Result<Box<dyn Asr>, _> = match args.dialect.as_str() {
        "aliyun" => aliyun::from_provider_config(config).map(|a| Box::new(a) as Box<dyn Asr>),
        "deepgram" => deepgram::from_provider_config(config).map(|a| Box::new(a) as Box<dyn Asr>),
        "elevenlabs" => {
            elevenlabs::from_provider_config(config).map(|a| Box::new(a) as Box<dyn Asr>)
        }
        "soniox" => soniox::from_provider_config(config).map(|a| Box::new(a) as Box<dyn Asr>),
        "volcengine" => {
            volcengine::from_provider_config(config).map(|a| Box::new(a) as Box<dyn Asr>)
        }
        other => return Err(format!("unknown dialect {other}")),
    };
    built.map_err(|e| e.message)
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = parse_args()?;
    let key_env = args
        .key_env
        .clone()
        .unwrap_or_else(|| default_key_env(&args.dialect).into());
    let api_key = std::env::var(&key_env).map_err(|_| format!("{key_env} is not set"))?;
    let mut config = ProviderConfig::new(args.dialect.clone(), args.model.clone());
    config.api_key = Some(api_key);
    config.api_url = args.api_url.clone();
    config.options = Value::Object(args.options.clone());
    let asr = build_asr(&args, &config)?;
    let pcm = read_pcm(&args.audio).await?;

    let log = install_recorder();
    let mut handle = asr
        .start_stream(StreamingTranscribeRequest {
            format: AudioFormat::Pcm16Le,
            language: None,
            options: Value::Null,
        })
        .await
        .map_err(|e| e.message)?;
    let chunk = SAMPLE_RATE * 2 * args.chunk_ms as usize / 1000;
    let input = handle.input.clone();
    let finalize_at = args.finalize_at_ms;
    let chunk_ms = args.chunk_ms;
    let sender = tokio::spawn(async move {
        let mut sent_ms = 0;
        let mut finalized = false;
        for piece in pcm.chunks(chunk) {
            record_input(FixtureInput::Audio { bytes: piece.len() });
            if input
                .send(SessionInput::Audio(Bytes::copy_from_slice(piece)))
                .await
                .is_err()
            {
                return;
            }
            sent_ms += chunk_ms;
            if !finalized && finalize_at.is_some_and(|at| sent_ms >= at) {
                finalized = true;
                record_input(FixtureInput::Finalize);
                let _ = input.send(SessionInput::Finalize).await;
            }
            tokio::time::sleep(Duration::from_millis(chunk_ms)).await;
        }
        record_input(FixtureInput::End);
    });
    drop(handle.input);
    while let Some(event) = handle.events.next().await {
        eprintln!("{event:?}");
    }
    let _ = sender.await;

    let mut lines = vec![FixtureLine::Meta(json!({
        "dialect": args.dialect,
        "model": asr.model_name(),
        "source": "recorded",
        "audio": std::path::Path::new(&args.audio).file_name().map(|n| n.to_string_lossy()),
        "finalize_at_ms": args.finalize_at_ms,
    }))];
    lines.extend(log.lock().expect("recorder lock").drain(..));
    tokio::fs::write(&args.out, render_fixture(&lines))
        .await
        .map_err(|e| format!("{}: {e}", args.out))?;
    eprintln!("wrote {} lines to {}", lines.len(), args.out);
    Ok(())
}
