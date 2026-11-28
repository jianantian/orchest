use std::sync::Arc;

use napi::bindgen_prelude::Buffer;
use napi::{Env, JsObject, JsUnknown};
use napi_derive::napi;
use orchest::bindings::tts::TtsStream;
use orchest_provider::tts::TtsOptions;
use serde_json::{json, Value};

use super::error::protocol_error;

#[napi(object)]
pub struct TtsResult {
    pub audio: Buffer,
    pub format: String,
    pub usage: Value,
}

#[napi(js_name = "_synthesize")]
pub async fn synthesize(text: String, options: Value) -> napi::Result<TtsResult> {
    let result = TtsOptions::parse(options)
        .map_err(protocol_error)?
        .synthesize(text)
        .await
        .map_err(protocol_error)?;
    Ok(TtsResult {
        audio: Vec::<u8>::from(result.audio).into(),
        format: serde_json::to_value(result.format)
            .map_err(|error| napi::Error::from_reason(error.to_string()))?
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        usage: json!(result.voice_usage),
    })
}

#[napi]
pub struct NativeTtsStream {
    inner: Arc<TtsStream>,
}

#[napi]
impl NativeTtsStream {
    #[napi]
    pub fn next_event(&self, env: Env) -> napi::Result<JsObject> {
        let stream = self.inner.clone();
        env.execute_tokio_future(
            async move { stream.next().await.map_err(protocol_error) },
            |env, event| -> napi::Result<JsUnknown> {
                let Some(event) = event else {
                    return Ok(env.get_null()?.into_unknown());
                };
                let mut value: JsObject = env.to_js_value(&event.metadata)?.coerce_to_object()?;
                if let Some(audio) = event.audio {
                    let buffer = env.create_buffer_with_data(Vec::<u8>::from(audio))?;
                    value.set_named_property("audio", buffer.into_raw())?;
                }
                Ok(value.into_unknown())
            },
        )
    }

    #[napi]
    pub async fn command(&self, action: String, text: Option<String>) -> napi::Result<()> {
        self.inner
            .command(&action, text)
            .await
            .map_err(protocol_error)
    }

    #[napi]
    pub async fn usage(&self) -> Value {
        json!(self.inner.usage().await)
    }

    #[napi]
    pub async fn abort(&self) {
        self.inner.abort().await;
    }
}

impl Drop for NativeTtsStream {
    fn drop(&mut self) {
        self.inner.cancel_pending();
    }
}

#[napi(js_name = "_openTtsStream")]
pub async fn open_stream(options: Value, text: Option<String>) -> napi::Result<NativeTtsStream> {
    let options = TtsOptions::parse(options).map_err(protocol_error)?;
    let stream = match text {
        Some(text) => options
            .stream_synthesize(text)
            .await
            .map(|events| TtsStream::new(events, None)),
        None => options
            .start_duplex()
            .await
            .map(|handle| TtsStream::new(handle.events, Some(handle.input))),
    }
    .map_err(protocol_error)?;
    Ok(NativeTtsStream {
        inner: Arc::new(stream),
    })
}
