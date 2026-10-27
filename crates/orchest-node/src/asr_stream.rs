use std::sync::{Arc, Mutex};

use bytes::Bytes;
use napi::bindgen_prelude::Buffer;
use napi::threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction};
use napi::{Env, JsFunction, JsObject};
use napi_derive::napi;
use orchest_protocol::{
    CapabilityDescriptor, CapabilityExt, ErrorCode, Language, LifecycleEvent, ProtocolError,
    SessionInput, StreamEvent, StreamingTranscribeRequest, TranscriptStability,
    TranscriptUpdateKind,
};
use orchest_provider::{ProviderConfig, Registry};
use tokio::sync::Notify;

#[napi(object)]
pub struct AsrStreamOptions {
    pub format: String,
    pub sample_rate: u32,
    pub language: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub api_url: Option<String>,
    pub context: Option<serde_json::Value>,
    pub options: Option<serde_json::Value>,
}

struct Completion {
    result: Mutex<Option<Result<(), String>>>,
    notify: Notify,
}

#[napi]
pub struct NativeAsrStream {
    input: Arc<Mutex<Option<tokio::sync::mpsc::Sender<SessionInput>>>>,
    completion: Arc<Completion>,
    /// Whether the selected model honours `SessionInput::Finalize`.
    client_finalize: bool,
    provider: String,
    model: String,
}

#[napi]
impl NativeAsrStream {
    #[napi]
    pub async fn send_audio(&self, audio: Buffer) -> napi::Result<()> {
        if audio.is_empty() {
            return Err(napi::Error::from_reason("audio chunk cannot be empty"));
        }
        let sender = self
            .input
            .lock()
            .map_err(|_| napi::Error::from_reason("ASR session lock poisoned"))?
            .clone()
            .ok_or_else(|| napi::Error::from_reason("ASR session is finishing or closed"))?;
        sender
            .send(SessionInput::Audio(Bytes::copy_from_slice(&audio)))
            .await
            .map_err(|_| napi::Error::from_reason("ASR session input is closed"))
    }

    #[napi]
    pub async fn finalize(&self) -> napi::Result<()> {
        if !self.client_finalize {
            return Err(super::error::protocol_error(
                ProtocolError::new(
                    ErrorCode::UnsupportedOption,
                    format!("{}/{} does not support finalize", self.provider, self.model),
                )
                .with_provider(self.provider.clone())
                .with_model(self.model.clone()),
            ));
        }
        let sender = self
            .input
            .lock()
            .map_err(|_| napi::Error::from_reason("ASR session lock poisoned"))?
            .clone()
            .ok_or_else(|| napi::Error::from_reason("ASR session is finishing or closed"))?;
        sender
            .send(SessionInput::Finalize)
            .await
            .map_err(|_| napi::Error::from_reason("ASR session input is closed"))
    }

    #[napi]
    pub fn finish(&self) -> napi::Result<()> {
        self.input
            .lock()
            .map_err(|_| napi::Error::from_reason("ASR session lock poisoned"))?
            .take();
        Ok(())
    }

    #[napi]
    pub async fn wait(&self) -> napi::Result<()> {
        loop {
            let notified = self.completion.notify.notified();
            if let Some(result) = self
                .completion
                .result
                .lock()
                .map_err(|_| napi::Error::from_reason("ASR completion lock poisoned"))?
                .clone()
            {
                return result.map_err(napi::Error::from_reason);
            }
            notified.await;
        }
    }
}

impl Drop for NativeAsrStream {
    fn drop(&mut self) {
        if let Ok(mut input) = self.input.lock() {
            input.take();
        }
    }
}

#[napi(js_name = "_startAsrStream")]
pub fn start_asr_stream(
    env: Env,
    input: AsrStreamOptions,
    on_event: JsFunction,
) -> napi::Result<JsObject> {
    let tsfn: ThreadsafeFunction<serde_json::Value, ErrorStrategy::Fatal> = on_event
        .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<serde_json::Value>| {
            Ok(vec![ctx.env.to_js_value(&ctx.value)?])
        })?;
    env.execute_tokio_future(start(input, tsfn), |_, stream| Ok(stream))
}

async fn start(
    input: AsrStreamOptions,
    tsfn: ThreadsafeFunction<serde_json::Value, ErrorStrategy::Fatal>,
) -> napi::Result<NativeAsrStream> {
    let (provider, model) = match (input.provider, input.model) {
        (None, None) => (
            "aliyun".to_string(),
            Some("qwen-audio-3.0-asr-flash-streaming".to_string()),
        ),
        (Some(provider), model) => (super::asr::validate_provider(provider)?, model),
        (None, Some(_)) => return Err(napi::Error::from_reason("model requires provider")),
    };
    let key = super::asr::resolve_key(
        input.api_key,
        input.api_key_env.as_deref(),
        super::asr::default_key_env(&provider),
        &provider,
    )?;
    let registry = Registry::with_builtin();
    let query = registry.asr().provider(provider.clone());
    let entry = match &model {
        Some(model) => query.id(&format!("{provider}/{model}")),
        None => query.bidirectional(),
    }
    .select()
    .map_err(super::error::protocol_error)?;
    let model = model.unwrap_or_else(|| entry.descriptor.model.to_string());
    let handle = entry
        .instantiate(&ProviderConfig {
            provider: provider.clone(),
            model: model.clone(),
            api_key: Some(key),
            api_url: input.api_url,
            max_tokens: None,
            options: serde_json::Value::Null,
        })
        .map_err(super::error::protocol_error)?;
    let client_finalize = client_finalize(&handle.descriptor());
    let mut options = input.options.unwrap_or_else(|| serde_json::json!({}));
    if !options.is_object() {
        return Err(napi::Error::from_reason("options must be an object"));
    }
    options["sample_rate"] = serde_json::json!(input.sample_rate);
    if let Some(context) = input.context {
        validate_context(&context)?;
        options["context"] = context;
    }
    let realtime = handle
        .start_stream(StreamingTranscribeRequest {
            format: super::asr::parse_audio_format(&input.format)?,
            language: input.language.map(Language),
            options,
        })
        .await
        .map_err(super::error::protocol_error)?;
    let input_sender = Arc::new(Mutex::new(Some(realtime.input)));
    let completion = Arc::new(Completion {
        result: Mutex::new(None),
        notify: Notify::new(),
    });
    let input_task = Arc::clone(&input_sender);
    let completion_task = Arc::clone(&completion);
    tokio::spawn(async move {
        let mut events = realtime.events;
        let mut result = Ok(());
        while let Some(event) = events.next().await {
            let fatal_error = match &event {
                StreamEvent::Error {
                    error, fatal: true, ..
                } => Some(super::error::ProviderErrorData::from(error.clone()).encoded()),
                _ => None,
            };
            if let Some(value) = sdk_event(&event) {
                if let Err(error) = tsfn.call_async::<()>(value).await {
                    result = Err(error.to_string());
                    break;
                }
            }
            if let Some(error) = fatal_error {
                result = Err(error);
                break;
            }
        }
        if result.is_ok() {
            match input_task.lock() {
                Ok(input) if input.is_some() => {
                    result = Err(super::error::ProviderErrorData::from(ProtocolError::new(
                        ErrorCode::ProviderStreamError,
                        "ASR event stream closed before finish",
                    ))
                    .encoded());
                }
                Err(_) => result = Err("ASR session lock poisoned".to_string()),
                _ => {}
            }
        }
        if let Ok(mut slot) = completion_task.result.lock() {
            *slot = Some(result);
        }
        completion_task.notify.notify_waiters();
    });
    Ok(NativeAsrStream {
        input: input_sender,
        completion,
        client_finalize,
        provider,
        model,
    })
}

/// Whether the descriptor declares `client_finalize` (ADR-0006 D8). Models
/// that do not declare it are treated as not supporting Finalize.
fn client_finalize(descriptor: &CapabilityDescriptor) -> bool {
    match &descriptor.ext {
        CapabilityExt::Asr(value) => value
            .get("client_finalize")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        _ => false,
    }
}

/// Project a protocol event onto the SDK ASR event shape (ADR-0006 D2): a
/// `kind` discriminant plus snake_case fields. Events with no ASR meaning
/// (text deltas, usage, …) are dropped.
fn sdk_event(event: &StreamEvent) -> Option<serde_json::Value> {
    use serde_json::json;
    let segment_id = |segment: &Option<orchest_protocol::SegmentRef>| {
        segment
            .as_ref()
            .and_then(|segment| segment.segment_id.clone())
    };
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
        }),
        StreamEvent::Extension(orchest_protocol::CapabilityEventExt::Asr(data)) => {
            json!({"kind": "extras", "data": data})
        }
        _ => return None,
    })
}

fn validate_context(context: &serde_json::Value) -> napi::Result<()> {
    let valid = context.as_array().is_some_and(|messages| {
        messages.iter().all(|message| {
            let role = message.get("role").and_then(serde_json::Value::as_str);
            let text = message.get("text").and_then(serde_json::Value::as_str);
            matches!(role, Some("user" | "assistant")) && text.is_some()
        })
    });
    if valid {
        Ok(())
    } else {
        Err(napi::Error::from_reason(
            "context must be an array of { role: 'user' | 'assistant', text: string } objects",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest_protocol::SegmentRef;
    use serde_json::json;

    fn segment(id: &str, update_kind: TranscriptUpdateKind) -> Option<SegmentRef> {
        Some(SegmentRef {
            segment_id: Some(id.into()),
            update_kind,
        })
    }

    #[test]
    fn transcript_events_carry_kind_and_segment() {
        let draft = StreamEvent::Transcript {
            text: "he".into(),
            stability: TranscriptStability::Provisional,
            segment: segment("s0", TranscriptUpdateKind::Snapshot),
        };
        assert_eq!(
            sdk_event(&draft),
            Some(json!({"kind": "draft", "text": "he", "segment_id": "s0"}))
        );
        let stable = StreamEvent::Transcript {
            text: "hello".into(),
            stability: TranscriptStability::Committed,
            segment: segment("final0", TranscriptUpdateKind::Append),
        };
        assert_eq!(
            sdk_event(&stable),
            Some(
                json!({"kind": "stable", "text": "hello", "segment_id": "final0", "update": "append"})
            )
        );
        let turn = StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment: segment("final0", TranscriptUpdateKind::Append),
        });
        assert_eq!(
            sdk_event(&turn),
            Some(json!({"kind": "turn", "segment_id": "final0"}))
        );
    }

    #[test]
    fn lifecycle_error_and_extras_events() {
        assert_eq!(
            sdk_event(&StreamEvent::Lifecycle(LifecycleEvent::SessionClosed)),
            Some(json!({"kind": "session_closed"}))
        );
        let error = StreamEvent::Error {
            error: ProtocolError::new(ErrorCode::UnsupportedOption, "no finalize"),
            fatal: false,
        };
        assert_eq!(
            sdk_event(&error),
            Some(
                json!({"kind": "error", "code": "unsupported_option", "message": "no finalize", "fatal": false})
            )
        );
        let extras = StreamEvent::Extension(orchest_protocol::CapabilityEventExt::Asr(
            json!({"words": []}),
        ));
        assert_eq!(
            sdk_event(&extras),
            Some(json!({"kind": "extras", "data": {"words": []}}))
        );
        assert_eq!(sdk_event(&StreamEvent::Text { delta: "x".into() }), None);
    }

    #[test]
    fn client_finalize_reads_the_asr_ext() {
        let base = CapabilityDescriptor::new("p", "m", orchest_protocol::Capability::Asr);
        assert!(!client_finalize(&base));
        let declared = base
            .clone()
            .with_ext(CapabilityExt::Asr(json!({"client_finalize": true})));
        assert!(client_finalize(&declared));
    }
}
