//! ASR helpers shared by the Python and Node bindings: model selection for
//! `transcribe` / `start_asr_stream`, default key variables, `finalize()`,
//! and the SDK event shape of a streaming session (ADR-0006).

use orchest_protocol::{
    CapabilityDescriptor, CapabilityEventExt, ErrorCode, LifecycleEvent, ProtocolError, SegmentRef,
    SessionInput, StreamEvent, TranscriptStability, TranscriptUpdateKind,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// Which ASR surface a selection is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AsrMode {
    /// One-shot `transcribe`: request/response models only.
    Batch,
    /// `start_asr_stream`: bidirectional streaming models only.
    Streaming,
}

impl AsrMode {
    fn matches(self, streaming: bool) -> bool {
        streaming == matches!(self, Self::Streaming)
    }

    fn describe(self) -> &'static str {
        match self {
            Self::Batch => "one-shot",
            Self::Streaming => "streaming",
        }
    }

    fn other_api(self) -> &'static str {
        match self {
            Self::Batch => "start_asr_stream",
            Self::Streaming => "transcribe",
        }
    }
}

/// Pick the ASR model of `provider` for the `mode` surface among `rows`, the
/// provider's ASR catalog descriptors; returns the chosen row's index. With
/// `model` that row is used. Without it the provider's default for that
/// surface is: the unique candidate, else the unique `default_for_provider`
/// one, else `fallback_model` (the binding's own default) when it is a
/// candidate. A model or provider that only exists on the other surface gets
/// an error naming the API to use instead.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn select_asr_model(
    rows: &[&CapabilityDescriptor],
    provider: &str,
    model: Option<&str>,
    mode: AsrMode,
    fallback_model: Option<&str>,
) -> Result<usize, ProtocolError> {
    let wrong_surface = |what: String| {
        ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            format!(
                "{what} has no {} ASR model; use {}",
                mode.describe(),
                mode.other_api()
            ),
        )
        .with_provider(provider.to_string())
    };
    let named = |model: &str| rows.iter().position(|row| row.model.as_ref() == model);
    if let Some(model) = model {
        let index = named(model).ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::NoMatchingProvider,
                format!("no ASR model {provider}/{model}"),
            )
            .with_provider(provider.to_string())
        })?;
        if !mode.matches(rows[index].streaming) {
            return Err(wrong_surface(format!("{provider}/{model}")));
        }
        return Ok(index);
    }
    let candidates: Vec<usize> = (0..rows.len())
        .filter(|&index| mode.matches(rows[index].streaming))
        .collect();
    if candidates.is_empty() {
        return Err(if rows.is_empty() {
            ProtocolError::new(
                ErrorCode::NoMatchingProvider,
                format!("no ASR provider named {provider}"),
            )
        } else {
            wrong_surface(provider.to_string())
        });
    }
    let defaults: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(|&index| rows[index].default_for_provider)
        .collect();
    let fallback = fallback_model
        .and_then(named)
        .filter(|index| candidates.contains(index));
    match (candidates.as_slice(), defaults.as_slice(), fallback) {
        ([one], _, _) | (_, [one], _) => Ok(*one),
        (_, _, Some(one)) => Ok(one),
        _ => Err(ProtocolError::new(
            ErrorCode::NoMatchingProvider,
            format!(
                "{provider} has several {} ASR models and no default; pass model= one of {}",
                mode.describe(),
                candidates
                    .iter()
                    .map(|&index| rows[index].model.as_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
        .with_provider(provider.to_string())),
    }
}

/// The environment variable an ASR provider's key is read from by default.
pub fn default_asr_key_env(provider: &str) -> &'static str {
    match provider {
        "assemblyai" => "ASSEMBLYAI_API_KEY",
        "speechmatics" => "SPEECHMATICS_API_KEY",
        "deepgram" => "DEEPGRAM_API_KEY",
        "elevenlabs" => "ELEVENLABS_API_KEY",
        "soniox" => "SONIOX_API_KEY",
        "volcengine" => "VOLCENGINE_ASR_API_KEY",
        _ => "DASHSCOPE_API_KEY",
    }
}

/// The static check of the SDK `finalize()`: the error to raise, without
/// sending anything, when the stream's model does not honour
/// `SessionInput::Finalize` (its descriptor's `client_finalize` is false).
pub fn finalize_unsupported(
    client_finalize: bool,
    provider: &str,
    model: &str,
) -> Option<ProtocolError> {
    (!client_finalize).then(|| {
        ProtocolError::new(
            ErrorCode::UnsupportedOption,
            format!("{provider}/{model} does not support finalize"),
        )
        .with_provider(provider)
        .with_model(model)
    })
}

/// Why the SDK `finalize()` could not submit its request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FinalizeEnqueueError {
    /// `finish()` was called; the input is closing.
    Finishing,
    /// The session ended and no longer reads input.
    Closed,
}

impl std::fmt::Display for FinalizeEnqueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Finishing => "ASR session is finishing or closed",
            Self::Closed => "ASR session input is closed",
        })
    }
}

/// The enqueue step of the SDK `finalize()`: returning `Ok` only means the
/// request was submitted. The dialect's answer arrives later on the event
/// stream, as a Turn or a non-fatal error whose `operation` is `"finalize"`.
pub async fn enqueue_finalize(
    sender: Option<mpsc::Sender<SessionInput>>,
) -> Result<(), FinalizeEnqueueError> {
    sender
        .ok_or(FinalizeEnqueueError::Finishing)?
        .send(SessionInput::Finalize)
        .await
        .map_err(|_| FinalizeEnqueueError::Closed)
}

/// Every `kind` [`sdk_asr_event`] emits. `js/types.d.ts` and
/// `python/orchest/__init__.pyi` declare one event type per kind; a test
/// keeps the three in step.
pub const ASR_EVENT_KINDS: &[&str] = &[
    "draft",
    "stable",
    "turn",
    "session_started",
    "route_selected",
    "session_closed",
    "error",
    "extras",
];

/// Project a protocol event onto the SDK ASR event shape (ADR-0006 D2): a
/// `kind` discriminant plus snake_case fields. Events with no ASR meaning
/// (text deltas, usage, …) are dropped.
pub fn sdk_asr_event(event: &StreamEvent) -> Option<Value> {
    let segment_id =
        |segment: &Option<SegmentRef>| segment.as_ref().and_then(|s| s.segment_id.clone());
    Some(match event {
        StreamEvent::Transcript {
            text,
            stability: TranscriptStability::Provisional,
            segment,
        } => json!({"kind": "draft", "text": text, "segment_id": segment_id(segment)}),
        StreamEvent::Transcript {
            text,
            stability: TranscriptStability::Committed,
            segment,
        } => {
            let update = match segment.as_ref().map(|segment| segment.update_kind) {
                Some(TranscriptUpdateKind::Append) => "append",
                _ => "snapshot",
            };
            json!({
                "kind": "stable",
                "text": text,
                "segment_id": segment_id(segment),
                "update": update,
            })
        }
        StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { segment }) => {
            json!({"kind": "turn", "segment_id": segment_id(segment)})
        }
        StreamEvent::Lifecycle(LifecycleEvent::SessionStarted { session_id }) => {
            json!({"kind": "session_started", "session_id": session_id})
        }
        StreamEvent::Lifecycle(LifecycleEvent::RouteSelected {
            provider, model, ..
        }) => json!({"kind": "route_selected", "provider": provider, "model": model}),
        StreamEvent::Lifecycle(LifecycleEvent::SessionClosed) => json!({"kind": "session_closed"}),
        StreamEvent::Error { error, fatal } => json!({
            "kind": "error",
            "code": serde_json::to_value(error.code).ok(),
            "message": error.message,
            "fatal": fatal,
            // `"finalize"` when the error answers a `finalize()` (ADR-0006 D7.2)
            "operation": error.diagnostic_metadata.get("operation").and_then(Value::as_str),
        }),
        StreamEvent::Extension(CapabilityEventExt::Asr(data)) => {
            json!({"kind": "extras", "data": data})
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use orchest_protocol::{Capability, CapabilityDescriptor};

    use super::*;

    fn segment(id: &str, update_kind: TranscriptUpdateKind) -> Option<SegmentRef> {
        Some(SegmentRef {
            segment_id: Some(id.into()),
            update_kind,
        })
    }

    fn sample_events() -> Vec<StreamEvent> {
        vec![
            StreamEvent::Transcript {
                text: "hel".into(),
                stability: TranscriptStability::Provisional,
                segment: segment("s0", TranscriptUpdateKind::Snapshot),
            },
            StreamEvent::Transcript {
                text: "hello".into(),
                stability: TranscriptStability::Committed,
                segment: segment("s0", TranscriptUpdateKind::Append),
            },
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
                segment: segment("s0", TranscriptUpdateKind::Append),
            }),
            StreamEvent::Lifecycle(LifecycleEvent::SessionStarted {
                session_id: Some("x".into()),
            }),
            StreamEvent::Lifecycle(LifecycleEvent::RouteSelected {
                provider: "p".into(),
                model: "m".into(),
                trace_id: None,
            }),
            StreamEvent::Lifecycle(LifecycleEvent::SessionClosed),
            StreamEvent::Error {
                error: ProtocolError {
                    diagnostic_metadata: json!({"operation": "finalize"}),
                    ..ProtocolError::new(ErrorCode::UnsupportedOption, "no finalize")
                },
                fatal: false,
            },
            StreamEvent::Extension(CapabilityEventExt::Asr(json!({"words": []}))),
        ]
    }

    #[test]
    fn finalize_is_refused_statically_when_unsupported() {
        let error = finalize_unsupported(false, "aliyun", "paraformer").unwrap();
        assert_eq!(error.code, ErrorCode::UnsupportedOption);
        assert_eq!(error.provider.as_deref(), Some("aliyun"));
        assert_eq!(error.model.as_deref(), Some("paraformer"));
        assert!(finalize_unsupported(true, "deepgram", "nova-3").is_none());
    }

    #[tokio::test]
    async fn finalize_enqueue_fails_once_the_session_input_is_gone() {
        assert_eq!(
            enqueue_finalize(None).await,
            Err(FinalizeEnqueueError::Finishing)
        );
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        assert_eq!(
            enqueue_finalize(Some(tx)).await,
            Err(FinalizeEnqueueError::Closed)
        );
    }

    #[tokio::test]
    async fn finalize_enqueue_only_submits_and_the_answer_is_an_event() {
        let (tx, mut rx) = mpsc::channel(1);
        assert_eq!(enqueue_finalize(Some(tx)).await, Ok(()));
        assert_eq!(rx.recv().await, Some(SessionInput::Finalize));
        // a runtime rejection reaches the SDK as a non-fatal tagged error event
        let rejection = StreamEvent::Error {
            error: ProtocolError {
                diagnostic_metadata: json!({"operation": "finalize"}),
                ..ProtocolError::new(ErrorCode::Timeout, "no answer")
            },
            fatal: false,
        };
        let event = sdk_asr_event(&rejection).unwrap();
        assert_eq!(event["kind"], "error");
        assert_eq!(event["code"], "timeout");
        assert_eq!(event["operation"], "finalize");
        assert_eq!(event["fatal"], false);
        // an ordinary error carries no operation
        let other = StreamEvent::Error {
            error: ProtocolError::new(ErrorCode::ProviderStreamError, "x"),
            fatal: true,
        };
        assert_eq!(sdk_asr_event(&other).unwrap()["operation"], Value::Null);
    }

    #[test]
    fn transcript_events_carry_kind_and_segment() {
        let events: Vec<_> = sample_events().iter().filter_map(sdk_asr_event).collect();
        assert_eq!(
            events[0],
            json!({"kind": "draft", "text": "hel", "segment_id": "s0"})
        );
        assert_eq!(
            events[1],
            json!({"kind": "stable", "text": "hello", "segment_id": "s0", "update": "append"})
        );
        assert_eq!(events[2], json!({"kind": "turn", "segment_id": "s0"}));
        assert_eq!(
            events[6],
            json!({
                "kind": "error",
                "code": "unsupported_option",
                "message": "no finalize",
                "fatal": false,
                "operation": "finalize",
            })
        );
        assert_eq!(events[7], json!({"kind": "extras", "data": {"words": []}}));
        assert_eq!(
            sdk_asr_event(&StreamEvent::Text { delta: "x".into() }),
            None
        );
    }

    /// Kinds that appear as `kind: "…"` / `kind: Literal["…"]` in a declaration file.
    /// A repository file outside this crate, or `None` when it is not there
    /// (as in the published package), so the guard below skips instead of
    /// failing to build.
    fn repo_file(path: &str) -> Option<String> {
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path);
        // allow-blocking-io: a synchronous test reading a repository file
        let text = std::fs::read_to_string(&full).ok();
        if text.is_none() {
            eprintln!("skipping: {} is not available", full.display());
        }
        text
    }

    /// The fields each declared ASR event type has, keyed by its `kind`. A
    /// declaration is a TS `interface` or a Python `TypedDict` class whose
    /// body holds `kind: "<k>"` / `kind: Literal["<k>"]`.
    fn declared_events(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut events = BTreeMap::new();
        let mut fields: Option<BTreeSet<String>> = None;
        let mut kind: Option<String> = None;
        let mut flush = |fields: &mut Option<BTreeSet<String>>, kind: &mut Option<String>| {
            if let (Some(f), Some(k)) = (fields.take(), kind.take()) {
                events.insert(k, f);
            }
        };
        for line in source.lines() {
            let opens_block = (line.starts_with("export interface Asr")
                || line.starts_with("class Asr"))
                && (line.ends_with('{') || line.ends_with("(TypedDict):"));
            if opens_block {
                flush(&mut fields, &mut kind);
                fields = Some(BTreeSet::new());
                continue;
            }
            let Some(current) = fields.as_mut() else {
                continue;
            };
            if line == "}" || (!line.is_empty() && !line.starts_with(' ')) {
                flush(&mut fields, &mut kind);
                continue;
            }
            let Some((name, value)) = line.trim().split_once(':') else {
                continue;
            };
            let name = name.trim_end_matches('?');
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                continue;
            }
            if name == "kind" {
                kind = value.split('"').nth(1).map(str::to_string);
            }
            current.insert(name.to_string());
        }
        flush(&mut fields, &mut kind);
        events
    }

    #[test]
    fn sdk_events_match_the_declarations() {
        let mut emitted: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for event in sample_events().iter().filter_map(sdk_asr_event) {
            let kind = event["kind"].as_str().unwrap_or_default().to_string();
            let keys = event
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            emitted.insert(kind, keys);
        }
        let listed: BTreeSet<String> = ASR_EVENT_KINDS.iter().map(|k| k.to_string()).collect();
        assert_eq!(
            emitted.keys().cloned().collect::<BTreeSet<_>>(),
            listed,
            "ASR_EVENT_KINDS vs sdk_asr_event"
        );
        for path in [
            "js/types.d.ts",
            "python/orchest/__init__.pyi",
            "python/orchest/__init__.py",
        ] {
            let Some(source) = repo_file(path) else {
                continue;
            };
            assert_eq!(declared_events(&source), emitted, "{path} ASR event types");
        }
    }

    fn row(model: &'static str, streaming: bool, default: bool) -> CapabilityDescriptor {
        let mut descriptor =
            CapabilityDescriptor::new("acme", model, Capability::Asr).streaming(streaming);
        descriptor.default_for_provider = default;
        descriptor
    }

    fn pick(
        rows: &[CapabilityDescriptor],
        model: Option<&str>,
        mode: AsrMode,
        fallback: Option<&str>,
    ) -> Result<String, ProtocolError> {
        let refs: Vec<_> = rows.iter().collect();
        select_asr_model(&refs, "acme", model, mode, fallback)
            .map(|index| rows[index].model.to_string())
    }

    #[test]
    fn selection_stays_on_its_surface() {
        // Mirrors aliyun: two batch rows without a default, a default stream row.
        let mixed = [
            row("batch-a", false, false),
            row("batch-b", false, false),
            row("stream", true, true),
        ];
        let batch = |model, fallback| pick(&mixed, model, AsrMode::Batch, fallback);
        assert_eq!(batch(None, Some("batch-b")).unwrap(), "batch-b");
        assert_eq!(
            batch(None, Some("stream")).unwrap_err().code,
            ErrorCode::NoMatchingProvider
        );
        let ambiguous = batch(None, None).unwrap_err();
        assert!(
            ambiguous.message.contains("batch-a, batch-b"),
            "{}",
            ambiguous.message
        );
        assert_eq!(batch(Some("batch-a"), None).unwrap(), "batch-a");
        let wrong = batch(Some("stream"), None).unwrap_err();
        assert_eq!(wrong.code, ErrorCode::UnsupportedOperation);
        assert!(
            wrong.message.contains("use start_asr_stream"),
            "{}",
            wrong.message
        );
        assert_eq!(
            batch(Some("missing"), None).unwrap_err().code,
            ErrorCode::NoMatchingProvider
        );
        assert_eq!(
            pick(&mixed, None, AsrMode::Streaming, None).unwrap(),
            "stream"
        );

        let stream_only = [row("stream", true, true)];
        let error = pick(&stream_only, None, AsrMode::Batch, None).unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedOperation);
        assert!(
            error.message.contains("acme has no one-shot ASR model"),
            "{}",
            error.message
        );
        let none = pick(&[], None, AsrMode::Batch, None).unwrap_err();
        assert_eq!(none.code, ErrorCode::NoMatchingProvider);
    }
}
