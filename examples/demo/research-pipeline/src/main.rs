use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use orchest::{events::RuntimeEvent, model::ModelAdapter, tool::agent_as_tool::ContextMode};
use research_pipeline_demo::{
    events::render_event,
    supervisor::{
        build_supervisor, start_with_live_watchers, StartedSupervisor, LIVE_ATTACHMENT_BOUNDARY,
    },
    worker::Worker,
};

#[derive(Debug, Parser)]
#[command(
    name = "research-pipeline",
    about = "Research Pipeline supervised-delegation evidence demo"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        #[arg(long)]
        question: String,
        #[arg(long)]
        materials: String,
        #[arg(long)]
        fault: bool,
    },
}

fn live_chat_model() -> Result<Arc<dyn ModelAdapter>> {
    let model = std::env::var("RESEARCH_PIPELINE_CHAT_MODEL").context(
        "no live chat model configured: set RESEARCH_PIPELINE_CHAT_MODEL to provider/model",
    )?;
    let config = orchest_provider::ProviderRuntimeConfig {
        model,
        api_key: std::env::var("RESEARCH_PIPELINE_API_KEY").ok(),
        api_key_env: None,
        api_url: std::env::var("RESEARCH_PIPELINE_API_URL").ok(),
        max_tokens: std::env::var("RESEARCH_PIPELINE_MAX_TOKENS")
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok()),
    };
    let adapter = orchest_provider::create_adapter_from_config(config)
        .map_err(|error| anyhow::anyhow!("constructing Research Pipeline chat model: {error}"))?;
    Ok(Arc::from(adapter))
}

async fn approve_worker_request(handle: &orchest::run::RunHandle, event: &RuntimeEvent) {
    if let RuntimeEvent::SubAgentEvent {
        child_run_id,
        event,
        ..
    } = event
    {
        if matches!(event.as_ref(), RuntimeEvent::ApprovalRequested { .. }) {
            if let Err(error) = handle.respond_approval(*child_run_id, true).await {
                eprintln!("[approval] {error}");
            }
        }
    }
}

async fn run_live(question: String, materials: String, fault: bool) -> Result<()> {
    let model = live_chat_model()?;
    let materials = PathBuf::from(materials);
    let worker = Worker::from_paths(
        &materials,
        PathBuf::from(".research-pipeline").join("draft.md"),
    )?;
    let (config, registry) =
        build_supervisor(&worker, Arc::clone(&model), ContextMode::Fresh, fault)?;
    let StartedSupervisor {
        handle,
        mut events,
        watcher_events,
        ..
    } = start_with_live_watchers(config, question, model, registry).await;
    println!("[watchers] {LIVE_ATTACHMENT_BOUNDARY}");

    while let Some(event) = events.recv().await {
        println!("{}", render_event(&event));
        approve_worker_request(&handle, &event).await;
    }
    let watcher_event_count = watcher_events
        .lock()
        .map(|events| events.len())
        .unwrap_or_default();
    handle.wait().await;
    println!(
        "[watchers] observed {watcher_event_count} supervisor actor-emitted events; \
         forwarded child events remain on the primary EventReceiver"
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            question,
            materials,
            fault,
        } => run_live(question, materials, fault).await?,
    }
    Ok(())
}
