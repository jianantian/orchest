//! Command orchestration for `run` and `resume`. Keeps the pipeline sequence
//! separate from tool implementations (`tools.rs`, issue 003) and media
//! implementations (`media.rs`, issue 005 for the real gateways).

use std::path::PathBuf;
use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::registry::ToolRegistry;

use crate::fake_model::FakeModel;
use crate::media;
use crate::tools::{ReadFixtureTool, SearchFixturesTool, WriteReportTool};

pub type DemoError = Box<dyn std::error::Error + Send + Sync>;

/// Set (to any value) in `--fake` mode to make the approval loop auto-deny
/// instead of auto-approve the report write, so both paths are testable from
/// the CLI without an interactive prompt. Not consulted in live mode.
const FAKE_DENY_APPROVAL_ENV: &str = "BRIEFING_DESK_FAKE_DENY_APPROVAL";

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

    if let Some(session) = &args.session {
        println!("[session] {session} (persistence is a no-op in this issue; lands in issue 004)");
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
    registry.register(Arc::new(WriteReportTool::new(args.output.clone())))?;

    let auto_deny = std::env::var_os(FAKE_DENY_APPROVAL_ENV).is_some();
    let brief = run_agent(&args.question, registry, auto_deny).await?;
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
    println!(
        "[resume] session={} question={:?} output={} fake={} no_tts={} \
         (accepted but not yet implemented)",
        args.session,
        args.question,
        args.output.display(),
        args.fake,
        args.no_tts
    );
    Err("resume is not implemented yet; session persistence and resume land in issue 004".into())
}

/// Runs the agent loop against `FakeModel` with the search/read/write tools
/// registered, auto-resolving any approval request as soon as it is
/// requested (approve unless `auto_deny`), and returns the completed run's
/// final text.
async fn run_agent(
    question: &str,
    registry: ToolRegistry,
    auto_deny: bool,
) -> Result<String, DemoError> {
    let config = AgentConfig::builder("fake/fake")
        .system_prompt(
            "You are Briefing Desk, a research-brief assistant. Search the materials, \
             read the most relevant one, then write the report.",
        )
        .max_steps(6)
        .build()
        .map_err(|e| format!("building agent config: {e}"))?;

    let (handle, mut rx) =
        AgentRun::start(config, question.to_string(), Arc::new(FakeModel), registry);

    let mut brief = None;
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
            RuntimeEvent::RunCompleted { output } => {
                brief = Some(output.as_str().unwrap_or_default().to_string());
            }
            RuntimeEvent::RunFailed { error } => {
                return Err(format!("agent run failed: {error}").into());
            }
            other => println!("[event] {other:?}"),
        }
    }
    handle.wait().await;

    brief.ok_or_else(|| "agent run ended without producing output".into())
}
