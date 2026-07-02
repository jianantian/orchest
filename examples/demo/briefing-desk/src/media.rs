//! Materials discovery, ASR/TTS gateway tools, and the vision placeholder.
//!
//! ASR and TTS are wired for real against `orchest_protocol::{Asr, Tts}`
//! through `orchest_provider::Registry` (live path, env-var gated) with a
//! deterministic fake impl of each for offline `--fake` smoke. Vision stays
//! fake-only in both modes — see [`DescribeImageTool`]'s doc comment for why.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use orchest::tool::{Approval, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput};
use orchest_protocol::{
    Asr, AudioFormat, Capability, CapabilityDescriptor, ErrorCode, EventStream, Language, Modality,
    ProtocolError, RealtimeHandle, StreamingTranscribeRequest, SynthesizeRequest, SynthesizeResult,
    TranscribeRequest, TranscribeResult, Tts,
};
use serde_json::{json, Value};

use crate::app::DemoError;

#[derive(Debug, Default)]
pub struct Corpus {
    pub text: Vec<PathBuf>,
    pub images: Vec<PathBuf>,
    pub audio: Vec<PathBuf>,
}

/// Classify every file directly inside `dir` by extension. Unrecognized
/// extensions are ignored rather than treated as an error, since a materials
/// directory may reasonably contain notes, editor swap files, etc.
pub fn discover(dir: &Path) -> Result<Corpus, DemoError> {
    let mut corpus = Corpus::default();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))? {
        let path = entry
            .map_err(|e| format!("reading {}: {e}", dir.display()))?
            .path();
        if !path.is_file() {
            continue;
        }
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "md" | "txt" => corpus.text.push(path),
            "png" | "jpg" | "jpeg" => corpus.images.push(path),
            "wav" | "mp3" => corpus.audio.push(path),
            _ => {}
        }
    }
    corpus.text.sort();
    corpus.images.sort();
    corpus.audio.sort();
    Ok(corpus)
}

fn audio_format_for(path: &Path) -> AudioFormat {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => AudioFormat::Mp3,
        _ => AudioFormat::Wav,
    }
}

// ── Live provider construction (manual, env-var gated — see README) ────────

/// Constructs a real `Asr` from the registry. Not exercised by any automated
/// test in this repo (no network/credentials in CI); run manually per the
/// README's "Live provider run" section and record the outcome in the
/// validation report.
pub fn live_asr(provider: &str, model: &str, api_key: &str) -> Result<Box<dyn Asr>, DemoError> {
    let registry = orchest_provider::Registry::with_builtin();
    registry
        .asr()
        .provider(provider)
        .build(&orchest_provider::ProviderConfig::new(provider, model).with_api_key(api_key))
        .map_err(|e| format!("constructing live ASR provider '{provider}/{model}': {e}").into())
}

/// Constructs a real `Tts` from the registry. Same caveats as [`live_asr`].
pub fn live_tts(provider: &str, model: &str, api_key: &str) -> Result<Box<dyn Tts>, DemoError> {
    let registry = orchest_provider::Registry::with_builtin();
    registry
        .tts()
        .provider(provider)
        .build(&orchest_provider::ProviderConfig::new(provider, model).with_api_key(api_key))
        .map_err(|e| format!("constructing live TTS provider '{provider}/{model}': {e}").into())
}

// ── Fake Asr/Tts (offline --fake smoke) ─────────────────────────────────────

/// The quotable line that exists only in `interview.wav` (see issue 001's
/// fixture spec) — returning it here makes the ASR extraction requirement in
/// the demo README ("what the agent must extract, checkable") concretely
/// checkable in a test, without needing a real ASR credential.
const FAKE_TRANSCRIPT: &str = "Interviewer: What would happen if Loom disappeared tomorrow? \
Priya: Honestly? My team would go straight back to spreadsheets and lose about two hours a \
day. That's the real return on investment nobody puts in a slide deck.";

pub struct FakeAsr;

#[async_trait]
impl Asr for FakeAsr {
    fn provider_name(&self) -> &str {
        "fake"
    }

    fn model_name(&self) -> &str {
        "fake-asr"
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("fake", "fake-asr", Capability::Asr)
            .with_input_modalities([Modality::Audio])
            .with_output_modalities([Modality::Text])
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(&self, req: TranscribeRequest) -> Result<TranscribeResult, ProtocolError> {
        Ok(TranscribeResult {
            text: FAKE_TRANSCRIPT.to_string(),
            language: None,
            diagnostic_metadata: json!({"fake": true, "input_bytes": req.audio.len()}),
        })
    }

    async fn start_stream(
        &self,
        _req: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "FakeAsr does not support streaming",
        ))
    }
}

pub struct FakeTts;

#[async_trait]
impl Tts for FakeTts {
    fn provider_name(&self) -> &str {
        "fake"
    }

    fn model_name(&self) -> &str {
        "fake-tts"
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("fake", "fake-tts", Capability::Tts)
            .with_input_modalities([Modality::Text])
            .with_output_modalities([Modality::Audio])
    }

    async fn synthesize(&self, req: SynthesizeRequest) -> Result<SynthesizeResult, ProtocolError> {
        let marker = format!("FAKE AUDIO (fake TTS)\ntext_len={}\n", req.text.len());
        Ok(SynthesizeResult {
            audio: bytes::Bytes::from(marker.into_bytes()),
            format: req.format,
            diagnostic_metadata: json!({"fake": true}),
        })
    }

    async fn stream_synthesize(
        &self,
        _req: SynthesizeRequest,
    ) -> Result<EventStream, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "FakeTts does not support streaming",
        ))
    }

    async fn start_duplex_stream(&self) -> Result<RealtimeHandle, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "FakeTts does not support duplex",
        ))
    }
}

// ── Tools ────────────────────────────────────────────────────────────────

/// Transcribes an audio source from the materials corpus via `Asr::transcribe`
/// (real or fake, chosen by the caller at construction time). Never requires
/// approval — read-only.
pub struct TranscribeAudioTool {
    allowed: Vec<PathBuf>,
    asr: Box<dyn Asr>,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl TranscribeAudioTool {
    pub fn new(allowed: Vec<PathBuf>, asr: Box<dyn Asr>) -> Self {
        // `default` carries the real, discovered corpus path so a caller
        // (including the offline FakeModel, which has no other way to learn
        // where --materials actually pointed) has a concrete value to use
        // instead of guessing a path.
        let default_path = allowed.first().map(|p| p.display().to_string());
        Self {
            allowed,
            asr,
            metadata: ToolMetadata {
                side_effect: false,
                approval: Approval::Never,
                ..ToolMetadata::default()
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "an audio source path from the materials corpus",
                        "default": default_path
                    }
                },
                "required": ["path"]
            }),
        }
    }
}

#[async_trait]
impl Tool for TranscribeAudioTool {
    fn name(&self) -> &str {
        "transcribe_audio"
    }

    fn description(&self) -> &str {
        "Transcribe a recorded audio source from the materials corpus via ASR."
    }

    fn input_schema(&self) -> &Value {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&Value> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let path_str = input
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_input("missing required parameter 'path'"))?;
        let path = PathBuf::from(path_str);
        if !self.allowed.iter().any(|p| p == &path) {
            return Err(ToolError::invalid_input(format!(
                "'{path_str}' is not part of the materials corpus"
            ))
            .with_code("PATH_NOT_ALLOWED"));
        }
        let bytes = std::fs::read(&path).map_err(|e| {
            ToolError::fatal(format!("reading {path_str}: {e}")).with_code("READ_FAILED")
        })?;

        let result = self
            .asr
            .transcribe(TranscribeRequest {
                audio: bytes::Bytes::from(bytes),
                format: audio_format_for(&path),
                language: None,
                options: Value::Null,
            })
            .await
            .map_err(|e| {
                ToolError::fatal(format!("ASR transcription failed: {e}")).with_code("ASR_FAILED")
            })?;

        Ok(ToolOutput::Immediate(
            json!({"path": path_str, "transcript": result.text}),
        ))
    }
}

/// Fake-only placeholder for image understanding. Real vision input needs a
/// `ContentBlock::Image` inside the model's message history, but the only
/// public entry point — `AgentRun::start` — takes a plain `String`. The
/// method that *does* accept `initial_messages: Vec<Message>`
/// (`AgentRun::start_with_bus`) is `pub(crate)`, and `ToolResult.content` is
/// hard-typed `serde_json::Value`, so a tool cannot inject an image into the
/// next model turn either. There is currently no public Orchest API path to
/// real vision-through-agent-loop at all — recorded as a release-blocker
/// finding in `docs/archive/iteration/v0_10/validation-notes.md` rather than
/// worked around by adding new surface to `orchest` itself (tracked at
/// https://github.com/jianantian/orchest/issues/195). This tool always
/// returns a fixed description, in both `--fake` and (hypothetical) live
/// mode, until that API gap closes.
pub struct DescribeImageTool {
    allowed: Vec<PathBuf>,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl DescribeImageTool {
    pub fn new(allowed: Vec<PathBuf>) -> Self {
        let default_path = allowed.first().map(|p| p.display().to_string());
        Self {
            allowed,
            metadata: ToolMetadata {
                side_effect: false,
                approval: Approval::Never,
                ..ToolMetadata::default()
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "an image source path from the materials corpus",
                        "default": default_path
                    }
                },
                "required": ["path"]
            }),
        }
    }
}

const FAKE_IMAGE_DESCRIPTION: &str = "Bar chart of referral-channel 30-day retention by \
quarter: Q1 38%, Q2 40%, Q3 42% — a steady upward trend, matching the 42% figure in \
001-retention-dashboard-notes.md rather than the 35% figure in 002-support-ticket-summary.md.";

#[async_trait]
impl Tool for DescribeImageTool {
    fn name(&self) -> &str {
        "describe_image"
    }

    fn description(&self) -> &str {
        "Describe an image source from the materials corpus. Placeholder: real vision-model \
         input has no public Orchest API today (see validation-notes.md); this always returns \
         a fixed description regardless of provider mode."
    }

    fn input_schema(&self) -> &Value {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&Value> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let path_str = input
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_input("missing required parameter 'path'"))?;
        let path = PathBuf::from(path_str);
        if !self.allowed.iter().any(|p| p == &path) {
            return Err(ToolError::invalid_input(format!(
                "'{path_str}' is not part of the materials corpus"
            ))
            .with_code("PATH_NOT_ALLOWED"));
        }
        Ok(ToolOutput::Immediate(json!({
            "path": path_str,
            "description": FAKE_IMAGE_DESCRIPTION,
        })))
    }
}

/// Synthesizes an audio version of the final brief via `Tts::synthesize`
/// (real or fake, chosen by the caller at construction time). Writes only to
/// a fixed path decided by the CLI. Always requires approval.
pub struct SynthesizeBriefTool {
    output_path: PathBuf,
    tts: Box<dyn Tts>,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl SynthesizeBriefTool {
    pub fn new(output_path: PathBuf, tts: Box<dyn Tts>) -> Self {
        Self {
            output_path,
            tts,
            metadata: ToolMetadata {
                side_effect: true,
                approval: Approval::Always,
                ..ToolMetadata::default()
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "brief text to synthesize as audio"}
                },
                "required": ["text"]
            }),
        }
    }
}

#[async_trait]
impl Tool for SynthesizeBriefTool {
    fn name(&self) -> &str {
        "synthesize_brief"
    }

    fn description(&self) -> &str {
        "Synthesize an audio version of the final brief via TTS. Requires approval."
    }

    fn input_schema(&self) -> &Value {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&Value> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let text = input
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_input("missing required parameter 'text'"))?;

        let result = self
            .tts
            .synthesize(SynthesizeRequest {
                text: text.to_string(),
                voice: None,
                format: AudioFormat::Wav,
                options: Value::Null,
            })
            .await
            .map_err(|e| {
                ToolError::fatal(format!("TTS synthesis failed: {e}")).with_code("TTS_FAILED")
            })?;

        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::fatal(format!("creating {}: {e}", parent.display()))
                    .with_code("WRITE_FAILED")
            })?;
        }
        std::fs::write(&self.output_path, &result.audio[..]).map_err(|e| {
            ToolError::fatal(format!("writing {}: {e}", self.output_path.display()))
                .with_code("WRITE_FAILED")
        })?;

        Ok(ToolOutput::Immediate(json!({
            "path": self.output_path.display().to_string(),
            "bytes_written": result.audio.len(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext {
            run_id: orchest::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "test-call".into(),
            event_tx: None,
            webhook_base_url: None,
            approval_bus: orchest::run::ApprovalBus::default(),
            remaining_budget: Default::default(),
            parent_messages: vec![],
        }
    }

    #[tokio::test]
    async fn fake_asr_transcribes_deterministically() {
        let asr = FakeAsr;
        let result = asr
            .transcribe(TranscribeRequest {
                audio: bytes::Bytes::from_static(b"not really audio"),
                format: AudioFormat::Wav,
                language: None,
                options: Value::Null,
            })
            .await
            .expect("fake transcribe should succeed");
        assert_eq!(result.text, FAKE_TRANSCRIPT);
    }

    #[tokio::test]
    async fn fake_tts_synthesizes_deterministically() {
        let tts = FakeTts;
        let result = tts
            .synthesize(SynthesizeRequest {
                text: "hello".into(),
                voice: None,
                format: AudioFormat::Wav,
                options: Value::Null,
            })
            .await
            .expect("fake synthesize should succeed");
        assert!(!result.audio.is_empty());
    }

    #[tokio::test]
    async fn transcribe_audio_tool_returns_transcript_for_allowed_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("interview.wav");
        std::fs::write(&path, b"fake audio bytes").expect("write fixture");

        let tool = TranscribeAudioTool::new(vec![path.clone()], Box::new(FakeAsr));
        let output = tool
            .execute(json!({"path": path.to_str().unwrap()}), &test_ctx())
            .await
            .expect("transcribe should succeed");
        let ToolOutput::Immediate(value) = output else {
            panic!("expected immediate output");
        };
        assert_eq!(value["transcript"], FAKE_TRANSCRIPT);
    }

    #[tokio::test]
    async fn transcribe_audio_tool_rejects_disallowed_path() {
        let tool = TranscribeAudioTool::new(vec![PathBuf::from("allowed.wav")], Box::new(FakeAsr));
        let err = tool
            .execute(json!({"path": "not-allowed.wav"}), &test_ctx())
            .await
            .expect_err("disallowed path should error");
        assert_eq!(err.code.as_deref(), Some("PATH_NOT_ALLOWED"));
    }

    #[tokio::test]
    async fn describe_image_tool_returns_fixed_description() {
        let path = PathBuf::from("chart.png");
        let tool = DescribeImageTool::new(vec![path.clone()]);
        let output = tool
            .execute(json!({"path": path.to_str().unwrap()}), &test_ctx())
            .await
            .expect("describe should succeed");
        let ToolOutput::Immediate(value) = output else {
            panic!("expected immediate output");
        };
        assert_eq!(value["description"], FAKE_IMAGE_DESCRIPTION);
    }

    #[tokio::test]
    async fn synthesize_brief_tool_writes_exactly_one_file_and_marks_side_effect() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output_path = dir.path().join("brief.wav");
        let tool = SynthesizeBriefTool::new(output_path.clone(), Box::new(FakeTts));

        tool.execute(json!({"text": "the brief"}), &test_ctx())
            .await
            .expect("synthesize should succeed");

        assert!(output_path.exists());
        assert!(
            tool.metadata().side_effect,
            "TTS tool must mark side_effect"
        );
        assert_eq!(tool.metadata().approval, Approval::Always);
    }
}
