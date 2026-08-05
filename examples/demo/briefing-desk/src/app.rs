//! Command orchestration for `run` and `resume`. Keeps the pipeline sequence
//! separate from tool implementations (`tools.rs`, issue 003) and media
//! implementations (`media.rs`, issue 005 for the real gateways).
//!
//! Eval runner uses [`run_with_model`] / [`resume_with_model`] so tests can
//! inject a scripted `ModelAdapter` without duplicating the product pipeline.

use std::path::PathBuf;
use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::model::ModelAdapter;
use orchest::run::{AgentConfig, AgentRun, EventReceiver, RunHandle, RunInput};
use orchest::session::{SessionSnapshot, SessionStore, SqliteSessionStore};
use orchest::tool::agent_as_tool::ContextMode;
use orchest::tool::registry::ToolRegistry;
use orchest::tool::ToolError;

use crate::harness;
use crate::media::{self, DescribeImageTool, SynthesizeBriefTool, TranscribeAudioTool};
use crate::tools::{ReadFixtureTool, SearchFixturesTool, WriteReportTool};

pub type DemoError = Box<dyn std::error::Error + Send + Sync>;

/// If set (along with `_MODEL` and `_API_KEY`), `run` constructs a real ASR
/// provider via the registry instead of `FakeAsr`. Manual/env-var-gated per
/// the PRD - not exercised by any automated test in this repo.
const LIVE_ASR_PROVIDER_ENV: &str = "BRIEFING_DESK_ASR_PROVIDER";
const LIVE_ASR_MODEL_ENV: &str = "BRIEFING_DESK_ASR_MODEL";
const LIVE_ASR_API_KEY_ENV: &str = "BRIEFING_DESK_ASR_API_KEY";

/// Same idea as the `LIVE_ASR_*` triplet, for TTS.
const LIVE_TTS_PROVIDER_ENV: &str = "BRIEFING_DESK_TTS_PROVIDER";
const LIVE_TTS_MODEL_ENV: &str = "BRIEFING_DESK_TTS_MODEL";
const LIVE_TTS_API_KEY_ENV: &str = "BRIEFING_DESK_TTS_API_KEY";

/// Chat model configuration. Set `BRIEFING_DESK_CHAT_MODEL` to a
/// `provider/model` string (e.g. `anthropic/claude-sonnet-4-6`,
/// `deepseek/deepseek-v4-flash`). The API key goes in `_API_KEY`; if unset,
/// the provider factory's default key env is used (e.g. `ANTHROPIC_API_KEY`
/// for the anthropic provider). `_API_URL` overrides the endpoint;
/// `_MAX_TOKENS` overrides the output token ceiling.
const LIVE_CHAT_MODEL_ENV: &str = "BRIEFING_DESK_CHAT_MODEL";
const LIVE_CHAT_API_KEY_ENV: &str = "BRIEFING_DESK_CHAT_API_KEY";
const LIVE_CHAT_API_URL_ENV: &str = "BRIEFING_DESK_CHAT_API_URL";
const LIVE_CHAT_MAX_TOKENS_ENV: &str = "BRIEFING_DESK_CHAT_MAX_TOKENS";

fn live_asr_env() -> Option<(String, String, String)> {
    Some((
        std::env::var(LIVE_ASR_PROVIDER_ENV).ok()?,
        std::env::var(LIVE_ASR_MODEL_ENV).ok()?,
        std::env::var(LIVE_ASR_API_KEY_ENV).ok()?,
    ))
}

fn live_tts_env() -> Option<(String, String, String)> {
    Some((
        std::env::var(LIVE_TTS_PROVIDER_ENV).ok()?,
        std::env::var(LIVE_TTS_MODEL_ENV).ok()?,
        std::env::var(LIVE_TTS_API_KEY_ENV).ok()?,
    ))
}

/// Constructs the chat model adapter from `BRIEFING_DESK_CHAT_*` env vars.
/// The same adapter is reused for the main agent, vision (`describe_image`),
/// and the `review_report` sub-agent.
fn chat_model() -> Result<Arc<dyn ModelAdapter>, DemoError> {
    let model = std::env::var(LIVE_CHAT_MODEL_ENV).map_err(|_| {
        format!(
            "no chat model configured: set {LIVE_CHAT_MODEL_ENV} to a provider/model string \
                 (e.g. anthropic/claude-sonnet-4-6). See .env.example for all \
                 BRIEFING_DESK_CHAT_* variables."
        )
    })?;
    let config = orchest_provider::ProviderRuntimeConfig {
        model,
        api_key: std::env::var(LIVE_CHAT_API_KEY_ENV).ok(),
        api_key_env: None,
        api_url: std::env::var(LIVE_CHAT_API_URL_ENV).ok(),
        max_tokens: std::env::var(LIVE_CHAT_MAX_TOKENS_ENV)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok()),
    };
    let adapter = orchest_provider::create_adapter_from_config(config)
        .map_err(|e| format!("constructing chat model: {e}"))?;
    Ok(Arc::from(adapter))
}

/// Sessions persist under `.briefing-desk-sessions/<session-id>.sqlite3`,
/// relative to the current working directory, so `run --session X` and a
/// later `resume --session X` (a separate process) agree on the same file.
fn session_db_path(session_id: &str) -> PathBuf {
    PathBuf::from(".briefing-desk-sessions").join(format!("{session_id}.sqlite3"))
}

fn open_session_store(session_id: &str) -> Result<SqliteSessionStore, DemoError> {
    let path = session_db_path(session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    SqliteSessionStore::open(&path)
        .map_err(|e| format!("opening session store {}: {e}", path.display()).into())
}

pub struct RunArgs {
    pub materials: PathBuf,
    pub question: String,
    pub output: PathBuf,
    pub session: Option<String>,
    pub no_tts: bool,
}

pub struct ResumeArgs {
    pub session: String,
    pub question: String,
    pub output: PathBuf,
    pub no_tts: bool,
}

pub async fn run(args: RunArgs) -> Result<(), DemoError> {
    let model = chat_model()?;
    println!(
        "[model] {}",
        std::env::var(LIVE_CHAT_MODEL_ENV).unwrap_or_default()
    );
    let outcome = run_with_model(args, model, |_| {}).await?;
    println!("[done] final message: {}", outcome.final_text);
    if outcome.output_path.exists() {
        println!("[report] written to {}", outcome.output_path.display());
    } else {
        println!("[report] not written (the agent chose not to write)");
    }
    let audio_path = outcome.output_path.with_extension("wav");
    if outcome.no_tts {
        println!("[synthesize] skipped (--no-tts)");
    } else if audio_path.exists() {
        println!(
            "[synthesize] audio brief written to {}",
            audio_path.display()
        );
    } else {
        println!("[synthesize] skipped (no report was written)");
    }
    Ok(())
}

/// Product pipeline with an injected model and optional event observer.
///
/// Used by the eval runner (scripted or live model). Ordinary CLI `run`
/// constructs the model from env and passes a no-op observer.
pub async fn run_with_model<F>(
    args: RunArgs,
    model: Arc<dyn ModelAdapter>,
    observer: F,
) -> Result<CapturedRunOutcome, DemoError>
where
    F: FnMut(&RuntimeEvent),
{
    let corpus = media::discover(&args.materials)?;
    println!(
        "[materials] {} text, {} image, {} audio source(s) in {}",
        corpus.text.len(),
        corpus.images.len(),
        corpus.audio.len(),
        args.materials.display()
    );

    let text_entries: Vec<(PathBuf, String)> = corpus
        .text
        .iter()
        .map(|path| {
            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            Ok::<_, DemoError>((path.clone(), content))
        })
        .collect::<Result<_, _>>()?;

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(SearchFixturesTool::new(text_entries)))?;
    registry.register(Arc::new(ReadFixtureTool::new(corpus.text.clone())))?;
    registry.register(reviewer_tool(&model)?)?;

    if !corpus.audio.is_empty() {
        let asr = match live_asr_env() {
            Some((provider, model_name, key)) => {
                println!("[asr] live provider={provider} model={model_name}");
                media::live_asr(&provider, &model_name, &key)?
            }
            None => Box::new(media::fake_asr()),
        };
        registry.register(Arc::new(TranscribeAudioTool::new(
            corpus.audio.clone(),
            asr,
        )))?;
    }
    if !corpus.images.is_empty() {
        registry.register(Arc::new(DescribeImageTool::new(
            corpus.images.clone(),
            Arc::clone(&model),
        )))?;
    }

    registry.register(Arc::new(WriteReportTool::new(args.output.clone())))?;

    if !args.no_tts {
        let tts = match live_tts_env() {
            Some((provider, model_name, key)) => {
                println!("[tts] live provider={provider} model={model_name}");
                media::live_tts(&provider, &model_name, &key)?
            }
            None => Box::new(media::fake_tts()),
        };
        let audio_path = args.output.with_extension("wav");
        registry.register(Arc::new(SynthesizeBriefTool::new(audio_path, tts)))?;
    }

    let mut builder = AgentConfig::builder("briefing-agent", "briefing-desk/run")
        .system_prompt(harness::MAIN_SYSTEM_PROMPT);

    if let Some(id) = &args.session {
        let store: Arc<dyn SessionStore> = Arc::new(open_session_store(id)?);
        builder = builder.session_store(store, id.clone());
        println!("[session] persisting to {}", session_db_path(id).display());
    } else {
        println!("[session] no --session given; this run will not be resumable");
    }

    let config = builder
        .max_steps(10)
        .build()
        .map_err(|e| format!("building agent config: {e}"))?;

    let (handle, rx) = AgentRun::start(
        config,
        RunInput::text(args.question.clone()),
        model,
        registry,
    );
    let final_text = drain_events(handle, rx, observer).await?;
    Ok(CapturedRunOutcome {
        final_text,
        output_path: args.output,
        no_tts: args.no_tts,
    })
}

pub async fn resume(args: ResumeArgs) -> Result<(), DemoError> {
    let store = open_session_store(&args.session)?;
    let mut snapshot = store
        .load(&args.session)
        .await
        .map_err(|e| format!("loading session '{}': {e}", args.session))?
        .ok_or_else(|| {
            format!(
                "no persisted session found for '{}' at {}; run with --session {} first",
                args.session,
                session_db_path(&args.session).display(),
                args.session
            )
        })?;

    let model = chat_model()?;
    println!(
        "[model] {}",
        std::env::var(LIVE_CHAT_MODEL_ENV).unwrap_or_default()
    );

    let store: Arc<dyn SessionStore> = Arc::new(store);
    snapshot.active_config = snapshot
        .active_config
        .with_session_store(Arc::clone(&store), args.session.clone());

    let outcome = resume_with_model(snapshot, args, model, |_| {}).await?;
    println!("[done] follow-up answer: {}", outcome.final_text);
    Ok(())
}

/// Resume path with injected model + observer (eval follow-up attempts).
pub async fn resume_with_model<F>(
    snapshot: SessionSnapshot,
    args: ResumeArgs,
    model: Arc<dyn ModelAdapter>,
    observer: F,
) -> Result<CapturedRunOutcome, DemoError>
where
    F: FnMut(&RuntimeEvent),
{
    let (handle, rx) = AgentRun::resume_with_input(
        snapshot,
        RunInput::text(args.question.clone()),
        model,
        ToolRegistry::new(),
    )?;
    let answer = drain_events(handle, rx, observer).await?;

    std::fs::write(&args.output, &answer)
        .map_err(|e| format!("writing {}: {e}", args.output.display()))?;
    println!(
        "[report] follow-up answer written to {}",
        args.output.display()
    );

    if args.no_tts {
        println!("[synthesize] skipped (--no-tts)");
    } else {
        let tts: Box<dyn orchest_protocol::Tts> = match live_tts_env() {
            Some((provider, model_name, key)) => {
                println!("[tts] live provider={provider} model={model_name}");
                media::live_tts(&provider, &model_name, &key)?
            }
            None => Box::new(media::fake_tts()),
        };
        let result = tts
            .synthesize(orchest_protocol::SynthesizeRequest {
                text: answer.clone(),
                voice: None,
                format: orchest_protocol::AudioFormat::Wav,
                options: serde_json::Value::Null,
            })
            .await
            .map_err(|e| format!("TTS synthesis failed: {e}"))?;
        let audio_path = args.output.with_extension("wav");
        std::fs::write(&audio_path, &result.audio[..])
            .map_err(|e| format!("writing {}: {e}", audio_path.display()))?;
        println!(
            "[synthesize] audio brief written to {}",
            audio_path.display()
        );
    }

    Ok(CapturedRunOutcome {
        final_text: answer,
        output_path: args.output,
        no_tts: args.no_tts,
    })
}

/// Outcome of a captured product pipeline execution.
#[derive(Debug, Clone)]
pub struct CapturedRunOutcome {
    pub final_text: String,
    pub output_path: PathBuf,
    pub no_tts: bool,
}

/// Build the same main-agent config the product run uses (for seed materialize).
pub fn main_agent_config() -> Result<AgentConfig, DemoError> {
    AgentConfig::builder("briefing-agent", "briefing-desk/run")
        .system_prompt(harness::MAIN_SYSTEM_PROMPT)
        .max_steps(10)
        .build()
        .map_err(|e| format!("building agent config: {e}").into())
}

/// Expose chat model construction for the eval runner's live path.
pub fn live_chat_model() -> Result<Arc<dyn ModelAdapter>, DemoError> {
    chat_model()
}

/// Env var name for the required live chat model (eval refuses if unset).
pub const CHAT_MODEL_ENV: &str = LIVE_CHAT_MODEL_ENV;

/// Wraps a lightweight reviewer sub-agent (Agent-as-Tool, `ContextMode::Fresh`
/// so it never sees the parent's conversation) as a `review_report` tool the
/// parent model calls before `write_report`.
fn reviewer_tool(model: &Arc<dyn ModelAdapter>) -> Result<Arc<dyn orchest::tool::Tool>, DemoError> {
    let reviewer_config = AgentConfig::builder("briefing-reviewer", "briefing-desk/reviewer")
        .system_prompt(harness::REVIEWER_SYSTEM_PROMPT)
        .max_steps(2)
        .build()
        .map_err(|e| format!("building reviewer config: {e}"))?;

    let tool = reviewer_config
        .as_tool(
            "review_report",
            harness::REVIEW_REPORT_TOOL_DESCRIPTION,
        )
        .model(Arc::clone(model))
        .registry(ToolRegistry::new())
        .context_mode(ContextMode::Fresh)
        .input_mapper(|input: serde_json::Value| {
            input
                .get("draft")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| ToolError::fatal("missing required parameter 'draft'"))
        })
        .output_extractor(|details: serde_json::Value| {
            serde_json::json!({"output": details.get("output").cloned().unwrap_or(details)})
        })
        .build()
        .map_err(|e| format!("building reviewer_tool: {e}"))?;
    Ok(tool)
}

/// Drives an already-started run's event stream to completion: renders
/// model/tool/approval/sub-agent/run-completion events to stdout, auto-approves
/// any `ApprovalRequested`, and returns the run's final text.
///
/// `observer` is invoked for every event **before** stdout rendering so eval
/// recorders can capture a sanitized trajectory without changing CLI output.
/// Ordinary `run` / `resume` pass a no-op observer.
pub async fn drain_events<F>(
    handle: RunHandle,
    mut rx: EventReceiver,
    mut observer: F,
) -> Result<String, DemoError>
where
    F: FnMut(&RuntimeEvent),
{
    let mut answer = None;
    while let Some(event) = rx.recv().await {
        observer(&event);
        match &event {
            RuntimeEvent::ModelCallStarted { step } => println!("[model] step {step} started"),
            RuntimeEvent::ModelCallCompleted { tokens, .. } => println!(
                "[model] step completed ({} in / {} out tokens)",
                tokens.input_tokens, tokens.output_tokens
            ),
            RuntimeEvent::ToolCallStarted { tool, input, .. } => {
                println!("[tool] {tool} started input={input}")
            }
            RuntimeEvent::ToolCallCompleted { tool, output, .. } => {
                println!("[tool] {tool} completed output={output}")
            }
            RuntimeEvent::ToolCallFailed { tool, error } => println!(
                "[tool] {tool} failed: {} (kind={:?} retry={:?} code={:?} next_step={:?})",
                error.message, error.kind, error.retry, error.code, error.next_step
            ),
            RuntimeEvent::ApprovalRequested { tool_call, .. } => {
                println!("[approval] auto-approving {}", tool_call.name);
                let _ = handle.respond_approval(handle.run_id, true).await;
            }
            RuntimeEvent::ApprovalGranted { tool_call, .. } => {
                println!("[approval] granted for {}", tool_call.name)
            }
            RuntimeEvent::ApprovalDenied { tool_call, .. } => {
                println!("[approval] denied for {}", tool_call.name)
            }
            RuntimeEvent::SubAgentStarted { child_run_id, .. } => {
                println!("[reviewer] started child={child_run_id}")
            }
            RuntimeEvent::SubAgentEvent {
                child_run_id,
                event,
                ..
            } => {
                if let RuntimeEvent::RunCompleted { output, .. } = event.as_ref() {
                    println!("[reviewer] child={child_run_id} verdict={output}");
                }
            }
            RuntimeEvent::SubAgentCompleted {
                child_run_id,
                output,
                ..
            } => {
                println!("[reviewer] completed child={child_run_id} output={output}")
            }
            RuntimeEvent::SubAgentFailed {
                child_run_id,
                error,
            } => {
                println!("[reviewer] failed child={child_run_id} error={error}")
            }
            RuntimeEvent::RunCompleted { output, .. } => {
                answer = Some(output.as_str().unwrap_or_default().to_string());
            }
            RuntimeEvent::RunFailed { error, .. } => {
                return Err(format!("agent run failed: {error}").into());
            }
            other => println!("[event] {other:?}"),
        }
    }
    handle.wait().await;

    answer.ok_or_else(|| "agent run ended without producing output".into())
}
