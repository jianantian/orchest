//! Acceptance-ruler shape tests for the protocol spine (Issue 002 / ADR-0001).
//!
//! These prove the two hard-acceptance rulers seat in `orchest-protocol` using
//! only protocol types — **no provider-local content/event structs**:
//!
//! * **Omni** — a full-duplex `RealtimeSession`: audio in / audio+text out /
//!   mid-stream tool use, with audio that never stops while a tool runs.
//! * **Chameleon** — a `ChatModel` whose output stream carries an `Image`
//!   (`ContentBlock::Image`) reusing the content model, no `GenTask`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::mpsc;

use orchest_protocol::{
    AudioFormat, Capability, ChatModel, ContentBlock, EventStream, MediaSource, Message, Modality,
    ModelCapabilities, ModelResponse, ProtocolError, RealtimeSession, RequestOptions, SessionInput,
    StopReason, StreamEvent, TokenUsage, ToolDef, TranscriptStability,
};

// ===========================================================================
// Omni ruler — RealtimeSession with concurrent audio + mid-stream tool use
// ===========================================================================

struct FakeOmniSession {
    sent: Arc<Mutex<Vec<SessionInput>>>,
    events: EventStream,
}

#[async_trait]
impl RealtimeSession for FakeOmniSession {
    async fn send(&self, input: SessionInput) -> Result<(), ProtocolError> {
        self.sent.lock().unwrap().push(input);
        Ok(())
    }

    fn events(&mut self) -> &mut EventStream {
        &mut self.events
    }

    async fn close(&mut self) -> Result<(), ProtocolError> {
        Ok(())
    }
}

#[tokio::test]
async fn omni_full_duplex_seats_in_protocol() {
    let (tx, stream) = EventStream::channel(32);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut session = FakeOmniSession {
        sent: sent.clone(),
        events: stream,
    };

    // The "server" streams audio out, a transcript, model text, then a
    // mid-stream tool call, and CONTINUES emitting audio after the tool call —
    // proving audio is not blocked by tool use. All via the unified StreamEvent.
    let server = tokio::spawn(async move {
        tx.send(StreamEvent::AudioDelta {
            data: Bytes::from_static(b"audio-0"),
            format: AudioFormat::Pcm16Le,
            sequence: 0,
        })
        .await
        .unwrap();
        tx.send(StreamEvent::Transcript {
            text: "what's the weather".into(),
            stability: TranscriptStability::Provisional,
            segment: None,
        })
        .await
        .unwrap();
        tx.send(StreamEvent::Text {
            delta: "Let me check".into(),
        })
        .await
        .unwrap();
        tx.send(StreamEvent::ToolUseStart {
            id: "t1".into(),
            name: "get_weather".into(),
        })
        .await
        .unwrap();
        tx.send(StreamEvent::ToolUseArgsChunk {
            id: "t1".into(),
            delta: "{\"city\":\"SF\"}".into(),
        })
        .await
        .unwrap();
        tx.send(StreamEvent::ToolUseEnd { id: "t1".into() })
            .await
            .unwrap();
        // audio continues *after* the tool call — never blocked
        tx.send(StreamEvent::AudioDelta {
            data: Bytes::from_static(b"audio-1"),
            format: AudioFormat::Pcm16Le,
            sequence: 1,
        })
        .await
        .unwrap();
        tx.send(StreamEvent::Done {
            usage: TokenUsage::default(),
        })
        .await
        .unwrap();
    });

    let mut audio_chunks = 0;
    let mut saw_tool_end = false;
    let mut audio_after_tool = false;

    while let Some(ev) = session.events().next().await {
        match ev {
            StreamEvent::AudioDelta { .. } => {
                audio_chunks += 1;
                if saw_tool_end {
                    audio_after_tool = true;
                }
            }
            StreamEvent::ToolUseEnd { id } => {
                saw_tool_end = true;
                // Tool runs "on a separate task"; result is fed back through the
                // SAME send channel as audio, so it doesn't stall the audio out.
                session
                    .send(SessionInput::ToolResult {
                        tool_use_id: id,
                        content: serde_json::json!({ "temp_c": 18 }),
                    })
                    .await
                    .unwrap();
            }
            StreamEvent::Done { .. } => break,
            _ => {}
        }
    }
    server.await.unwrap();

    assert!(
        saw_tool_end,
        "mid-stream tool use must surface as ToolUseEnd"
    );
    assert!(
        audio_after_tool,
        "audio must keep flowing after a mid-stream tool call"
    );
    assert_eq!(audio_chunks, 2, "both audio chunks delivered");
    // The tool result was sent back as a unified SessionInput::ToolResult.
    let sent = sent.lock().unwrap();
    assert!(matches!(sent.as_slice(), [SessionInput::ToolResult { .. }]));
}

// ===========================================================================
// Chameleon ruler — a ChatModel whose output stream carries an Image
// ===========================================================================

struct ChameleonModel;

#[async_trait]
impl ChatModel for ChameleonModel {
    fn provider_name(&self) -> &str {
        "fake"
    }

    fn model_name(&self) -> &str {
        "chameleon-1"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            streaming: true,
            ..Default::default()
        }
    }

    fn descriptor(&self) -> orchest_protocol::CapabilityDescriptor {
        // Declares it natively *emits* images — no GenTask involved.
        orchest_protocol::CapabilityDescriptor::new(
            self.provider_name().to_string(),
            self.model_name().to_string(),
            Capability::Chat,
        )
        .streaming(true)
        .with_input_modalities([Modality::Text])
        .with_output_modalities([Modality::Text, Modality::Image])
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, orchest_protocol::ModelError> {
        let image = ContentBlock::Image {
            source: MediaSource::Url {
                url: "https://example.test/cat.png".into(),
            },
            detail: None,
        };
        if let Some(tx) = tx {
            tx.send(StreamEvent::Text {
                delta: "here is a cat".into(),
            })
            .await
            .ok();
            // The turn natively emits an Image via the content model.
            tx.send(StreamEvent::Content {
                block: image.clone(),
            })
            .await
            .ok();
            tx.send(StreamEvent::Done {
                usage: TokenUsage::default(),
            })
            .await
            .ok();
        }
        Ok(ModelResponse {
            content: vec![ContentBlock::Text("here is a cat".into()), image],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        })
    }
}

#[tokio::test]
async fn chameleon_turn_emits_image_in_protocol() {
    let model = ChameleonModel;

    // The descriptor advertises image output with no separate gen capability.
    let desc = model.descriptor();
    assert_eq!(desc.capability, Capability::Chat);
    assert!(desc.emits(&[Modality::Image]));

    let (tx, mut rx) = mpsc::channel(16);
    let resp = model
        .complete(&[], &[], &RequestOptions::default(), Some(tx))
        .await
        .unwrap();

    // The streamed output carried an Image content block — reusing the content
    // model, no provider-local event/content struct.
    let mut streamed_image = false;
    while let Some(ev) = rx.recv().await {
        if let StreamEvent::Content {
            block: ContentBlock::Image { .. },
        } = ev
        {
            streamed_image = true;
        }
    }
    assert!(
        streamed_image,
        "the turn must stream an Image content block"
    );

    // And the final response content holds the Image.
    assert!(resp
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::Image { .. })));
}
