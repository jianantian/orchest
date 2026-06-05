use bytes::Bytes;
use tokio::sync::mpsc;

use crate::error::{AsrError, AsrErrorCode};
use crate::types::{AsrFinalOutput, AsrStreamEvent, AudioChunk, AudioChunkBoundary};

pub struct AsrAudioSink {
    inner: mpsc::Sender<AudioChunk>,
}

impl AsrAudioSink {
    pub fn new(sender: mpsc::Sender<AudioChunk>) -> Self {
        Self { inner: sender }
    }

    pub async fn send_audio(&self, data: Bytes, timestamp_ms: Option<u64>) -> Result<(), AsrError> {
        self.inner
            .send(AudioChunk {
                data,
                timestamp_ms,
                boundary: AudioChunkBoundary::None,
            })
            .await
            .map_err(|_| AsrError::new(AsrErrorCode::Cancelled, "stream closed"))
    }

    pub async fn flush_segment(&self) -> Result<(), AsrError> {
        self.inner
            .send(AudioChunk {
                data: Bytes::new(),
                timestamp_ms: None,
                boundary: AudioChunkBoundary::Flush,
            })
            .await
            .map_err(|_| AsrError::new(AsrErrorCode::Cancelled, "stream closed"))
    }

    pub async fn end_stream(&self) -> Result<(), AsrError> {
        self.inner
            .send(AudioChunk {
                data: Bytes::new(),
                timestamp_ms: None,
                boundary: AudioChunkBoundary::End,
            })
            .await
            .map_err(|_| AsrError::new(AsrErrorCode::Cancelled, "stream closed"))
    }
}

pub struct AsrEventStream {
    inner: mpsc::Receiver<AsrStreamEvent>,
}

impl AsrEventStream {
    pub fn new(receiver: mpsc::Receiver<AsrStreamEvent>) -> Self {
        Self { inner: receiver }
    }

    pub async fn next(&mut self) -> Option<AsrStreamEvent> {
        self.inner.recv().await
    }

    pub async fn next_final(&mut self) -> Result<AsrFinalOutput, AsrError> {
        loop {
            match self.inner.recv().await {
                Some(AsrStreamEvent::AsrFinal { final_output }) => return Ok(*final_output),
                Some(AsrStreamEvent::Error {
                    error, fatal: true, ..
                }) => return Err(error),
                Some(_) => continue,
                None => {
                    return Err(AsrError::new(
                        AsrErrorCode::Cancelled,
                        "event stream closed without final output",
                    ))
                }
            }
        }
    }
}

pub struct AsrStream {
    pub input: AsrAudioSink,
    pub events: AsrEventStream,
}

impl AsrStream {
    pub fn new(input: AsrAudioSink, events: AsrEventStream) -> Self {
        Self { input, events }
    }

    pub fn split(self) -> (AsrAudioSink, AsrEventStream) {
        (self.input, self.events)
    }

    pub async fn flush_and_wait_final(&mut self) -> Result<AsrFinalOutput, AsrError> {
        self.input.flush_segment().await?;
        self.events.next_final().await
    }

    pub async fn end_and_wait_final(&mut self) -> Result<AsrFinalOutput, AsrError> {
        self.input.end_stream().await?;
        self.events.next_final().await
    }
}

pub fn create_stream_pair(
    audio_buffer: usize,
    event_buffer: usize,
) -> (
    AsrAudioSink,
    mpsc::Receiver<AudioChunk>,
    mpsc::Sender<AsrStreamEvent>,
    AsrEventStream,
) {
    let (audio_tx, audio_rx) = mpsc::channel(audio_buffer);
    let (event_tx, event_rx) = mpsc::channel(event_buffer);
    (
        AsrAudioSink::new(audio_tx),
        audio_rx,
        event_tx,
        AsrEventStream::new(event_rx),
    )
}
