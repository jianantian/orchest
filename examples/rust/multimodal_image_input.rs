//! Multimodal image input — the public API path added by issue #195.
//!
//! `AgentRun::start` takes a `RunInput`, which can carry an image alongside
//! text for a single user turn. This example builds one from a URL; for a
//! local file, read it with `std::fs::read`, base64-encode it (e.g. with the
//! `base64` crate), and use `MediaSource::Base64 { media_type, data }`
//! instead — see `examples/demo/briefing-desk/src/media.rs`'s
//! `DescribeImageTool` for that path.
//!
//! Run with:
//!   ANTHROPIC_API_KEY=sk-... cargo run --example multimodal_image_input

use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::model::{MediaSource, ModelAdapter, StreamEvent};
use orchest::run::{AgentConfig, AgentRun, RunInput};
use orchest::tool::registry::ToolRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model: Arc<dyn ModelAdapter> = Arc::from(orchest_provider::create_adapter_from_config(
        orchest_provider::ProviderRuntimeConfig {
            model: "anthropic/claude-sonnet-4-6".into(),
            api_key: None,
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
            api_url: None,
            max_tokens: Some(1024),
        },
    )?);

    let config = AgentConfig::builder("anthropic/claude-sonnet-4-6")
        .system_prompt("You are a helpful assistant.")
        .max_steps(3)
        .build()?;

    // A single user turn: text plus one image. Swap the URL for a real,
    // publicly reachable image before running this for real.
    let input = RunInput::text("What's in this image?").with_image(MediaSource::Url {
        url: "https://example.com/photos/chart.jpg".to_string(),
    });

    let (handle, mut rx) = AgentRun::start(config, input, model, ToolRegistry::new());

    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ModelStreamChunk {
                delta: StreamEvent::Text { delta },
            } => print!("{delta}"),
            RuntimeEvent::RunCompleted { output, .. } => println!("\n[done] {output}"),
            RuntimeEvent::RunFailed { error, .. } => eprintln!("\n[failed] {error}"),
            _ => {}
        }
    }
    handle.wait().await;
    Ok(())
}
