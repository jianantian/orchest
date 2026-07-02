//! Command orchestration for `run` and `resume`. Keeps the pipeline sequence
//! separate from tool implementations (issue 003) and media implementations
//! (`media.rs`, issue 005 for the real gateways).

use std::path::PathBuf;
use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::registry::ToolRegistry;

use crate::fake_model::FakeModel;
use crate::media;

pub type DemoError = Box<dyn std::error::Error + Send + Sync>;

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

    let brief = run_fake_agent(&args.question).await?;

    std::fs::write(&args.output, &brief)
        .map_err(|e| format!("writing {}: {e}", args.output.display()))?;
    println!("[write] brief written to {}", args.output.display());

    if args.no_tts {
        println!("[synthesize] skipped (--no-tts)");
    } else {
        let audio_path = args.output.with_extension("wav");
        media::fake_synthesize(&brief, &audio_path)?;
        println!(
            "[synthesize] audio brief written to {}",
            audio_path.display()
        );
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

/// Runs a minimal agent loop against `FakeModel` and returns the completed
/// brief text. No tools are registered yet — issue 003 adds the
/// search/read/write/report tool flow that gives the agent real reasoning
/// material; today the fake model ignores its input and returns a canned
/// response, which is enough to exercise the run pipeline offline.
async fn run_fake_agent(question: &str) -> Result<String, DemoError> {
    let config = AgentConfig::builder("fake/fake")
        .system_prompt("You are Briefing Desk, a research-brief assistant.")
        .max_steps(3)
        .build()
        .map_err(|e| format!("building agent config: {e}"))?;

    let (handle, mut rx) = AgentRun::start(
        config,
        question.to_string(),
        Arc::new(FakeModel),
        ToolRegistry::new(),
    );

    let mut brief = None;
    while let Some(event) = rx.recv().await {
        match event {
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
