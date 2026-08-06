use std::sync::{Arc, Mutex};

use bytes::Bytes;
use napi::bindgen_prelude::Buffer;
use napi::threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction};
use napi::{Env, JsFunction, JsObject};
use napi_derive::napi;
use orchest_protocol::{
    ErrorCode, Language, ProtocolError, SessionInput, StreamingTranscribeRequest,
};
use orchest_provider::{ProviderConfig, Registry};
use tokio::sync::Notify;

#[napi(object)]
pub struct AsrStreamOptions {
    pub format: String,
    pub sample_rate: u32,
    pub language: Option<String>,
    pub provider: Option<String>,
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
    let id = input
        .provider
        .unwrap_or_else(|| "aliyun/qwen-audio-3.0-asr-flash-streaming".into());
    let key = super::asr::resolve_key(
        input.api_key,
        input.api_key_env.as_deref(),
        super::asr::default_key_env(&id),
        &id,
    )?;
    let registry = Registry::with_builtin();
    let entry = registry
        .asr()
        .id(&id)
        .select()
        .map_err(super::error::protocol_error)?;
    let (provider, model) = id
        .split_once('/')
        .ok_or_else(|| napi::Error::from_reason("provider must be provider/model"))?;
    let handle = entry
        .instantiate(&ProviderConfig {
            provider: provider.into(),
            model: model.into(),
            api_key: Some(key),
            api_url: input.api_url,
            max_tokens: None,
            options: serde_json::Value::Null,
        })
        .map_err(super::error::protocol_error)?;
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
                orchest_protocol::StreamEvent::Error {
                    error, fatal: true, ..
                } => Some(super::error::ProviderErrorData::from(error.clone()).encoded()),
                _ => None,
            };
            match serde_json::to_value(&event) {
                Ok(value) => {
                    if let Err(error) = tsfn.call_async::<()>(value).await {
                        result = Err(error.to_string());
                        break;
                    }
                }
                Err(error) => {
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
