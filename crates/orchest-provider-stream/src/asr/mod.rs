//! WebSocket-backed ASR dialects (Issue 006).
//!
//! The Volcengine openspeech ASR wire layer is built on the shared
//! [`crate::openspeech`] header/constants/gzip core. The `Asr` trait impl + the
//! streaming audio→event loop sit on top of this codec and land alongside the
//! live transport; the per-vendor streaming dialects (deepgram/soniox/…) follow.

pub mod aliyun;
pub mod deepgram;
pub mod elevenlabs;
pub mod soniox;
pub mod volcengine;
