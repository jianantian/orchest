//! Materials discovery, ASR/TTS gateway tools, and the vision tool.
//!
//! ASR and TTS are wired for real against `orchest_protocol::{Asr, Tts}`
//! through `orchest_provider::Registry` (live path, env-var gated) with a
//! deterministic fake impl of each for offline `--fake` smoke. `describe_image`
//! (issue #195) follows the same shape: it builds a real `ContentBlock::Image`
//! from the corpus file and drives a real `ModelAdapter::complete()` call —
//! see [`DescribeImageTool`]'s doc comment.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use orchest::model::{ContentBlock, MediaSource, Message, ModelAdapter, RequestOptions, Role};
use orchest::tool::{Approval, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput};
use orchest_protocol::{Asr, AudioFormat, SynthesizeRequest, TranscribeRequest, Tts};
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
//
// Issue #196: these used to be local, hand-written `Asr`/`Tts` impls (the
// exact gap the issue found — no reusable fake existed anywhere in the
// workspace). Now backed by `orchest_provider::fakes`, gated behind that
// crate's `testing` feature.

/// The quotable line that exists only in `interview.wav` (see issue 001's
/// fixture spec) — returning it here makes the ASR extraction requirement in
/// the demo README ("what the agent must extract, checkable") concretely
/// checkable in a test, without needing a real ASR credential.
const FAKE_TRANSCRIPT: &str = "Interviewer: What would happen if Loom disappeared tomorrow? \
Priya: Honestly? My team would go straight back to spreadsheets and lose about two hours a \
day. That's the real return on investment nobody puts in a slide deck.";

/// The demo's offline `Asr`: always transcribes to [`FAKE_TRANSCRIPT`].
pub fn fake_asr() -> orchest_provider::fakes::FakeAsr {
    orchest_provider::fakes::FakeAsr::new(FAKE_TRANSCRIPT)
}

/// The demo's offline `Tts`: synthesizes the shared fake's default marker.
pub fn fake_tts() -> orchest_provider::fakes::FakeTts {
    orchest_provider::fakes::FakeTts::default()
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
        crate::harness::TRANSCRIBE_AUDIO_TOOL_DESCRIPTION
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

/// Describes an image from the materials corpus by constructing a real
/// `ContentBlock::Image` (base64-encoded file bytes) and driving a real
/// `ModelAdapter::complete()` call — the public API path issue #195 added.
/// Previously this returned a fixed string with no model call at all, since
/// there was no public runtime API to get an image in front of a model; see
/// `docs/archive/iteration/v0_10/validation-notes.md` for that history. The
/// adapter is the same vision-capable chat adapter the caller constructs for
/// the rest of the run.
pub struct DescribeImageTool {
    allowed: Vec<PathBuf>,
    model: Arc<dyn ModelAdapter>,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl DescribeImageTool {
    pub fn new(allowed: Vec<PathBuf>, model: Arc<dyn ModelAdapter>) -> Self {
        let default_path = allowed.first().map(|p| p.display().to_string());
        Self {
            allowed,
            model,
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

/// Maps a file extension to the MIME type `ContentBlock::Image`'s
/// `MediaSource::Base64` needs. Corpus discovery (`discover()` below) only
/// classifies png/jpg/jpeg/gif/webp as images, so this is exhaustive over
/// what can actually reach here.
fn media_type_for_extension(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

#[async_trait]
impl Tool for DescribeImageTool {
    fn name(&self) -> &str {
        "describe_image"
    }

    fn description(&self) -> &str {
        crate::harness::DESCRIBE_IMAGE_TOOL_DESCRIPTION
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

        let bytes = std::fs::read(&path)
            .map_err(|e| ToolError::fatal(format!("reading '{path_str}': {e}")))?;
        let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
        let media_type = media_type_for_extension(&path);

        let message = Message {
            role: Role::User,
            content: vec![
                ContentBlock::Text(
                    "Describe this image in one or two sentences for a research brief.".to_string(),
                ),
                ContentBlock::Image {
                    source: MediaSource::Base64 {
                        media_type: media_type.to_string(),
                        data,
                    },
                    detail: None,
                },
            ],
        };

        let response = self
            .model
            .complete(&[message], &[], &RequestOptions::default(), None)
            .await
            .map_err(|e| ToolError::fatal(format!("vision model call failed: {}", e.message)))?;

        let description = response
            .content
            .into_iter()
            .find_map(|block| match block {
                ContentBlock::Text(text) => Some(text),
                _ => None,
            })
            .ok_or_else(|| ToolError::fatal("vision model returned no text content"))?;

        let usage = response.usage;
        let model_output = json!({
            "path": path_str,
            "description": description,
        });
        // Full TokenUsage is event-only details; parent budget accounts for
        // vision input+output via external_usage.
        let external_usage = orchest::budget::BudgetUsage {
            tokens_used: usage.input_tokens.saturating_add(usage.output_tokens),
            tool_calls_used: 0,
            cost_usd: usage.cost_usd.unwrap_or(0.0),
        };
        let details = serde_json::to_value(&usage).unwrap_or_else(|_| {
            json!({
                "input_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens,
            })
        });

        Ok(ToolOutput::Structured {
            model_output,
            details,
            external_usage: Some(external_usage),
        })
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
        crate::harness::SYNTHESIZE_BRIEF_TOOL_DESCRIPTION
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

    use async_trait::async_trait;
    use orchest::model::{
        ContentBlock, Message, ModelCapabilities, ModelError, ModelResponse, RequestOptions,
        StopReason, StreamEvent, TokenUsage, ToolDef,
    };
    use tokio::sync::mpsc;

    /// Minimal inline fake for testing DescribeImageTool - returns a fixed
    /// description regardless of input. The real run uses a vision-capable
    /// chat adapter; this just exercises the tool's plumbing.
    struct FakeVisionModel;

    #[async_trait]
    impl ModelAdapter for FakeVisionModel {
        fn provider_name(&self) -> &str {
            "fake"
        }

        fn model_name(&self) -> &str {
            "fake-vision"
        }

        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }

        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            _tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("fake image description".into())],
                usage: TokenUsage {
                    input_tokens: 12,
                    output_tokens: 34,
                    ..TokenUsage::default()
                },
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }
    fn test_ctx() -> ToolContext {
        ToolContext::oneshot()
    }

    #[tokio::test]
    async fn fake_asr_transcribes_deterministically() {
        let asr = fake_asr();
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
        let tts = fake_tts();
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

        let tool = TranscribeAudioTool::new(vec![path.clone()], Box::new(fake_asr()));
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
        let tool =
            TranscribeAudioTool::new(vec![PathBuf::from("allowed.wav")], Box::new(fake_asr()));
        let err = tool
            .execute(json!({"path": "not-allowed.wav"}), &test_ctx())
            .await
            .expect_err("disallowed path should error");
        assert_eq!(err.code.as_deref(), Some("PATH_NOT_ALLOWED"));
    }

    #[tokio::test]
    async fn describe_image_tool_calls_model_with_image_block() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("chart.png");
        std::fs::write(&path, b"fake png bytes").expect("write fixture");

        let model: Arc<dyn ModelAdapter> = Arc::new(FakeVisionModel);
        let tool = DescribeImageTool::new(vec![path.clone()], model);
        let output = tool
            .execute(json!({"path": path.to_str().unwrap()}), &test_ctx())
            .await
            .expect("describe should succeed");
        let ToolOutput::Structured {
            model_output,
            details,
            external_usage,
        } = output
        else {
            panic!("expected structured output");
        };
        assert!(model_output["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty()));
        assert_eq!(model_output["path"], path.to_str().unwrap());
        assert_eq!(details["input_tokens"], 12);
        assert_eq!(details["output_tokens"], 34);
        let usage = external_usage.expect("vision external_usage");
        assert_eq!(usage.tokens_used, 46);
    }

    #[tokio::test]
    async fn describe_image_tool_rejects_disallowed_path() {
        let model: Arc<dyn ModelAdapter> = Arc::new(FakeVisionModel);
        let tool = DescribeImageTool::new(vec![PathBuf::from("chart.png")], model);
        let err = tool
            .execute(json!({"path": "not-allowed.png"}), &test_ctx())
            .await
            .expect_err("disallowed path should error");
        assert_eq!(err.code.as_deref(), Some("PATH_NOT_ALLOWED"));
    }

    #[tokio::test]
    async fn synthesize_brief_tool_writes_exactly_one_file_and_marks_side_effect() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output_path = dir.path().join("brief.wav");
        let tool = SynthesizeBriefTool::new(output_path.clone(), Box::new(fake_tts()));

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
