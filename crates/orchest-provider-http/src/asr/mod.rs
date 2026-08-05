//! REST/batch speech-to-text dialects (the light weight tier). These implement
//! the one-shot [`orchest_protocol::Asr::transcribe`] over `reqwest`; streaming
//! ASR lives in `orchest-provider-stream`. Registered through the wall via
//! [`crate::asr_entries`].

pub mod aliyun;
pub mod assemblyai;
pub mod speechmatics;
