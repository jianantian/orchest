//! Command orchestration for `run` and `resume`. Keeps the pipeline sequence
//! separate from tool implementations (`tools.rs`, issue 003) and media
//! implementations (`media.rs`, issue 005 for the real gateways).

use std::path::PathBuf;
use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::model::{ContentBlock, Message, ModelAdapter, Role};
use orchest::run::{AgentConfig, AgentRun, EventReceiver, RunHandle, RunInput};
use orchest::session::{SessionStore, SqliteSessionStore};
use orchest::tool::agent_as_tool::ContextMode;
use orchest::tool::registry::ToolRegistry;
use orchest::tool::ToolError;

use crate::fake_model::{DescribeImageFakeModel, FakeModel, ReviewerFakeModel};
use crate::media::{
    self, DescribeImageTool, FakeAsr, FakeTts, SynthesizeBriefTool, TranscribeAudioTool,
};
use crate::tools::{ReadFixtureTool, SearchFixturesTool, WriteReportTool};

pub type DemoError = Box<dyn std::error::Error + Send + Sync>;

/// Set (to any value) in `--fake` mode to make the approval loop auto-deny
/// `write_report` instead of auto-approving it, so both paths are testable
/// from the CLI without an interactive prompt. Not consulted in live mode.
const FAKE_DENY_APPROVAL_ENV: &str = "BRIEFING_DESK_FAKE_DENY_APPROVAL";

/// Same idea as `FAKE_DENY_APPROVAL_ENV`, scoped to `synthesize_brief`
/// specifically, so "write approved, TTS denied" is independently testable.
const FAKE_DENY_TTS_APPROVAL_ENV: &str = "BRIEFING_DESK_FAKE_DENY_TTS_APPROVAL";

/// If set (along with `_MODEL` and `_API_KEY`), `run` constructs a real ASR
/// provider via the registry instead of `FakeAsr`. Manual/env-var-gated per
/// the PRD — not exercised by any automated test in this repo.
const LIVE_ASR_PROVIDER_ENV: &str = "BRIEFING_DESK_ASR_PROVIDER";
const LIVE_ASR_MODEL_ENV: &str = "BRIEFING_DESK_ASR_MODEL";
const LIVE_ASR_API_KEY_ENV: &str = "BRIEFING_DESK_ASR_API_KEY";

/// Same idea as the `LIVE_ASR_*` triplet, for TTS.
const LIVE_TTS_PROVIDER_ENV: &str = "BRIEFING_DESK_TTS_PROVIDER";
const LIVE_TTS_MODEL_ENV: &str = "BRIEFING_DESK_TTS_MODEL";
const LIVE_TTS_API_KEY_ENV: &str = "BRIEFING_DESK_TTS_API_KEY";

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
    pub fake: bool,
    pub no_tts: bool,
}

pub struct ResumeArgs {
    pub session: String,
    pub question: String,
    pub output: PathBuf,
    pub fake: bool,
    pub no_tts: bool,
}

pub async fn run(args: RunArgs) -> Result<(), DemoError> {
    if !args.fake {
        return Err(
            "live provider mode is not implemented yet (--fake required); \
             live model/ASR/TTS wiring lands in issue 005"
                .into(),
        );
    }

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
    registry.register(reviewer_tool())?;

    if !corpus.audio.is_empty() {
        let asr = match live_asr_env() {
            Some((provider, model, key)) => {
                println!("[asr] live provider={provider} model={model}");
                media::live_asr(&provider, &model, &key)?
            }
            None => Box::new(FakeAsr),
        };
        registry.register(Arc::new(TranscribeAudioTool::new(
            corpus.audio.clone(),
            asr,
        )))?;
    }
    if !corpus.images.is_empty() {
        let vision_model: Arc<dyn ModelAdapter> = Arc::new(DescribeImageFakeModel);
        registry.register(Arc::new(DescribeImageTool::new(
            corpus.images.clone(),
            vision_model,
        )))?;
    }

    registry.register(Arc::new(WriteReportTool::new(args.output.clone())))?;

    if !args.no_tts {
        let tts = match live_tts_env() {
            Some((provider, model, key)) => {
                println!("[tts] live provider={provider} model={model}");
                media::live_tts(&provider, &model, &key)?
            }
            None => Box::new(FakeTts),
        };
        let audio_path = args.output.with_extension("wav");
        registry.register(Arc::new(SynthesizeBriefTool::new(audio_path, tts)))?;
    }

    let mut builder = AgentConfig::builder("fake/fake").system_prompt(
        "You are Briefing Desk, a research-brief assistant. Search the materials, read the \
         most relevant one, transcribe any audio source and describe any image source if \
         those tools are available, have review_report check your draft, then call \
         write_report. If synthesize_brief is available, call it last.",
    );

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

    let deny_write = std::env::var_os(FAKE_DENY_APPROVAL_ENV).is_some();
    let deny_tts = std::env::var_os(FAKE_DENY_TTS_APPROVAL_ENV).is_some();
    let (handle, rx) = AgentRun::start(
        config,
        RunInput::text(args.question.clone()),
        Arc::new(FakeModel),
        registry,
    );
    let brief = drain_events(handle, rx, deny_write, deny_tts).await?;
    println!("[done] final message: {brief}");

    if args.output.exists() {
        println!("[report] written to {}", args.output.display());
    } else {
        println!("[report] not written (denied, or the agent chose not to write)");
    }

    let audio_path = args.output.with_extension("wav");
    if args.no_tts {
        println!("[synthesize] skipped (--no-tts)");
    } else if audio_path.exists() {
        println!(
            "[synthesize] audio brief written to {}",
            audio_path.display()
        );
    } else {
        println!("[synthesize] skipped (no report was written, or TTS approval was denied)");
    }

    Ok(())
}

pub async fn resume(args: ResumeArgs) -> Result<(), DemoError> {
    if !args.fake {
        return Err(
            "live provider mode is not implemented yet (--fake required); \
             live model/ASR/TTS wiring lands in issue 005"
                .into(),
        );
    }

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

    snapshot.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text(args.question.clone())],
    });
    let store: Arc<dyn SessionStore> = Arc::new(store);
    snapshot.active_config = snapshot
        .active_config
        .with_session_store(Arc::clone(&store), args.session.clone());

    let (handle, rx) = AgentRun::resume(snapshot, Arc::new(FakeModel), ToolRegistry::new());
    let answer = drain_events(handle, rx, false, false).await?;
    println!("[done] follow-up answer: {answer}");

    std::fs::write(&args.output, &answer)
        .map_err(|e| format!("writing {}: {e}", args.output.display()))?;
    println!(
        "[report] follow-up answer written to {}",
        args.output.display()
    );

    // Resume has no registered tools (it answers directly from persisted
    // history, see fake_model.rs), so TTS runs as a direct capability call
    // here rather than through synthesize_brief.
    if args.no_tts {
        println!("[synthesize] skipped (--no-tts)");
    } else {
        let tts: Box<dyn orchest_protocol::Tts> = match live_tts_env() {
            Some((provider, model, key)) => {
                println!("[tts] live provider={provider} model={model}");
                media::live_tts(&provider, &model, &key)?
            }
            None => Box::new(FakeTts),
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

    Ok(())
}

/// Wraps a lightweight reviewer sub-agent (Agent-as-Tool, `ContextMode::Fresh`
/// so it never sees the parent's conversation) as a `review_report` tool the
/// parent model calls before `write_report`.
fn reviewer_tool() -> Arc<dyn orchest::tool::Tool> {
    let reviewer_model: Arc<dyn ModelAdapter> = Arc::new(ReviewerFakeModel);
    let reviewer_config = AgentConfig::builder("fake/reviewer")
        .system_prompt(
            "You are a report reviewer. Check the draft for accuracy against the corpus.",
        )
        .max_steps(2)
        .build()
        .expect("reviewer config is static and always valid");

    reviewer_config
        .as_tool(
            "review_report",
            "Reviews a draft report before it is finalized. Call this before write_report.",
        )
        .model(reviewer_model)
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
}

/// Drives an already-started run's event stream to completion: renders
/// model/tool/approval/sub-agent/run-completion events to stdout, resolves
/// any `ApprovalRequested` by auto-approving unless the relevant `deny_*`
/// flag is set for that specific tool, and returns the run's final text.
async fn drain_events(
    handle: RunHandle,
    mut rx: EventReceiver,
    deny_write: bool,
    deny_tts: bool,
) -> Result<String, DemoError> {
    let mut answer = None;
    while let Some(event) = rx.recv().await {
        match event {
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
                let deny = match tool_call.name.as_str() {
                    "synthesize_brief" => deny_tts,
                    _ => deny_write,
                };
                let decision = !deny;
                println!(
                    "[approval] requested for {} -> auto-{}",
                    tool_call.name,
                    if decision { "approving" } else { "denying" }
                );
                let _ = handle.respond_approval(handle.run_id, decision).await;
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
                if let RuntimeEvent::RunCompleted { output } = event.as_ref() {
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
            RuntimeEvent::RunCompleted { output } => {
                answer = Some(output.as_str().unwrap_or_default().to_string());
            }
            RuntimeEvent::RunFailed { error } => {
                return Err(format!("agent run failed: {error}").into());
            }
            other => println!("[event] {other:?}"),
        }
    }
    handle.wait().await;

    answer.ok_or_else(|| "agent run ended without producing output".into())
}
