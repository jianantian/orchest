//! Command orchestration for `run` and `resume`. Keeps the pipeline sequence
//! separate from tool implementations (`tools.rs`, issue 003) and media
//! implementations (`media.rs`, issue 005 for the real gateways).
//!
//! Eval and product execution both consume typed preparation from `execution`.

use std::path::PathBuf;
use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::model::ModelAdapter;
use orchest::run::{AgentRun, EventReceiver, RunHandle};
use orchest::session::{SessionSnapshot, SessionStore};

use crate::execution::{self, PreparedResume, PreparedRun, ResolvedExecutionEnvironment};

pub type DemoError = Box<dyn std::error::Error + Send + Sync>;

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
    let env = ResolvedExecutionEnvironment::from_env()?;
    println!("[model] {}/{}", env.chat.provider, env.chat.model);
    let outcome = run_with_environment(args, env, |_| {}).await?;
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

pub async fn run_with_environment<F>(
    args: RunArgs,
    env: ResolvedExecutionEnvironment,
    observer: F,
) -> Result<CapturedRunOutcome, DemoError>
where
    F: FnMut(&RuntimeEvent),
{
    let model = Arc::clone(&env.chat.adapter);
    let prepared = execution::prepare_run(args, env)?;
    execute_prepared_run(prepared, model, observer).await
}

async fn execute_prepared_run<F>(
    prepared: PreparedRun,
    model: Arc<dyn ModelAdapter>,
    observer: F,
) -> Result<CapturedRunOutcome, DemoError>
where
    F: FnMut(&RuntimeEvent),
{
    let (handle, rx) = AgentRun::start(prepared.config, prepared.input, model, prepared.registry);
    let final_text = drain_events(handle, rx, observer).await?;
    Ok(CapturedRunOutcome {
        final_text,
        output_path: prepared.output,
        no_tts: prepared.no_tts,
    })
}

pub async fn resume(args: ResumeArgs) -> Result<(), DemoError> {
    let store = execution::open_session_store(&args.session)?;
    let mut snapshot = store
        .load(&args.session)
        .await
        .map_err(|e| format!("loading session '{}': {e}", args.session))?
        .ok_or_else(|| {
            format!(
                "no persisted session found for '{}' at {}; run with --session {} first",
                args.session,
                execution::session_db_path(&args.session).display(),
                args.session
            )
        })?;

    let env = ResolvedExecutionEnvironment::from_env()?;
    println!("[model] {}/{}", env.chat.provider, env.chat.model);

    let store: Arc<dyn SessionStore> = Arc::new(store);
    snapshot.active_config = snapshot
        .active_config
        .with_session_store(Arc::clone(&store), args.session.clone());

    let outcome = resume_with_environment(snapshot, args, env, |_| {}).await?;
    println!("[done] follow-up answer: {}", outcome.final_text);
    Ok(())
}

pub async fn resume_with_environment<F>(
    snapshot: SessionSnapshot,
    args: ResumeArgs,
    env: ResolvedExecutionEnvironment,
    observer: F,
) -> Result<CapturedRunOutcome, DemoError>
where
    F: FnMut(&RuntimeEvent),
{
    let model = Arc::clone(&env.chat.adapter);
    let prepared = execution::prepare_resume(snapshot, args, env)?;
    execute_prepared_resume(prepared, model, observer).await
}

async fn execute_prepared_resume<F>(
    mut prepared: PreparedResume,
    model: Arc<dyn ModelAdapter>,
    observer: F,
) -> Result<CapturedRunOutcome, DemoError>
where
    F: FnMut(&RuntimeEvent),
{
    prepared.snapshot.active_config = prepared.config;
    let (handle, rx) =
        AgentRun::resume_with_input(prepared.snapshot, prepared.input, model, prepared.registry)?;
    let answer = drain_events(handle, rx, observer).await?;

    std::fs::write(&prepared.output, &answer)
        .map_err(|e| format!("writing {}: {e}", prepared.output.display()))?;
    println!(
        "[report] follow-up answer written to {}",
        prepared.output.display()
    );

    if prepared.no_tts {
        println!("[synthesize] skipped (--no-tts)");
    } else if let Some(tts) = prepared.tts.take() {
        let result = tts
            .synthesize(orchest_protocol::SynthesizeRequest {
                text: answer.clone(),
                voice: None,
                format: orchest_protocol::AudioFormat::Wav,
                options: serde_json::Value::Null,
            })
            .await
            .map_err(|e| format!("TTS synthesis failed: {e}"))?;
        let audio_path = prepared.output.with_extension("wav");
        std::fs::write(&audio_path, &result.audio[..])
            .map_err(|e| format!("writing {}: {e}", audio_path.display()))?;
        println!(
            "[synthesize] audio brief written to {}",
            audio_path.display()
        );
    }

    Ok(CapturedRunOutcome {
        final_text: answer,
        output_path: prepared.output,
        no_tts: prepared.no_tts,
    })
}

/// Outcome of a captured product pipeline execution.
#[derive(Debug, Clone)]
pub struct CapturedRunOutcome {
    pub final_text: String,
    pub output_path: PathBuf,
    pub no_tts: bool,
}

/// Env var name for the required live chat model (eval refuses if unset).
pub const CHAT_MODEL_ENV: &str = execution::CHAT_MODEL_ENV;

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
