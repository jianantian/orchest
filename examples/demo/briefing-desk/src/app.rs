//! Command orchestration for `run` and `resume`. Keeps the pipeline sequence
//! separate from tool implementations (`tools.rs`, issue 003) and media
//! implementations (`media.rs`, issue 005 for the real gateways).

use std::path::PathBuf;
use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::model::{ContentBlock, Message, ModelAdapter, Role};
use orchest::run::{AgentConfig, AgentRun, EventReceiver, RunHandle};
use orchest::session::{SessionStore, SqliteSessionStore};
use orchest::tool::agent_as_tool::ContextMode;
use orchest::tool::registry::ToolRegistry;
use orchest::tool::ToolError;

use crate::fake_model::{FakeModel, ReviewerFakeModel};
use crate::media;
use crate::tools::{ReadFixtureTool, SearchFixturesTool, WriteReportTool};

pub type DemoError = Box<dyn std::error::Error + Send + Sync>;

/// Set (to any value) in `--fake` mode to make the approval loop auto-deny
/// instead of auto-approve the report write, so both paths are testable from
/// the CLI without an interactive prompt. Not consulted in live mode.
const FAKE_DENY_APPROVAL_ENV: &str = "BRIEFING_DESK_FAKE_DENY_APPROVAL";

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

    for audio in &corpus.audio {
        println!("[transcribe] {}", media::fake_transcribe(audio));
    }
    for image in &corpus.images {
        println!("[vision] {}", media::fake_read_image(image));
    }

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
    registry.register(Arc::new(WriteReportTool::new(args.output.clone())))?;

    let mut builder = AgentConfig::builder("fake/fake").system_prompt(
        "You are Briefing Desk, a research-brief assistant. Search the materials, \
         read the most relevant one, have review_report check your draft, then \
         call write_report.",
    );

    if let Some(id) = &args.session {
        let store: Arc<dyn SessionStore> = Arc::new(open_session_store(id)?);
        builder = builder.session_store(store, id.clone());
        println!("[session] persisting to {}", session_db_path(id).display());
    } else {
        println!("[session] no --session given; this run will not be resumable");
    }

    let config = builder
        .max_steps(8)
        .build()
        .map_err(|e| format!("building agent config: {e}"))?;

    let auto_deny = std::env::var_os(FAKE_DENY_APPROVAL_ENV).is_some();
    let (handle, rx) =
        AgentRun::start(config, args.question.clone(), Arc::new(FakeModel), registry);
    let brief = drain_events(handle, rx, auto_deny).await?;
    println!("[done] final message: {brief}");

    if args.output.exists() {
        println!("[report] written to {}", args.output.display());
    } else {
        println!("[report] not written (denied, or the agent chose not to write)");
    }

    if args.no_tts {
        println!("[synthesize] skipped (--no-tts)");
    } else if args.output.exists() {
        let audio_path = args.output.with_extension("wav");
        media::fake_synthesize(&brief, &audio_path)?;
        println!(
            "[synthesize] audio brief written to {}",
            audio_path.display()
        );
    } else {
        println!("[synthesize] skipped (no report was written)");
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

    let auto_deny = std::env::var_os(FAKE_DENY_APPROVAL_ENV).is_some();
    let (handle, rx) = AgentRun::resume(snapshot, Arc::new(FakeModel), ToolRegistry::new());
    let answer = drain_events(handle, rx, auto_deny).await?;
    println!("[done] follow-up answer: {answer}");

    std::fs::write(&args.output, &answer)
        .map_err(|e| format!("writing {}: {e}", args.output.display()))?;
    println!(
        "[report] follow-up answer written to {}",
        args.output.display()
    );

    if args.no_tts {
        println!("[synthesize] skipped (--no-tts)");
    } else {
        let audio_path = args.output.with_extension("wav");
        media::fake_synthesize(&answer, &audio_path)?;
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
/// any `ApprovalRequested` by auto-approving unless `auto_deny`, and returns
/// the run's final text.
async fn drain_events(
    handle: RunHandle,
    mut rx: EventReceiver,
    auto_deny: bool,
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
                let decision = !auto_deny;
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
