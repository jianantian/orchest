//! WebSocket-backed TTS dialects (Issue 006).
//!
//! The Volcengine openspeech TTS wire layer is built on the shared
//! [`crate::openspeech`] core, using the event-framing variant: `FLAG_WITH_EVENT`
//! with an `event` number and an optional `session_id`. The `Tts` trait impl and
//! the synthesize/duplex loops sit on top of this codec and land alongside the
//! live transport; minimax-ws TTS follows.

pub mod aliyun;
pub mod minimax;
pub mod volcengine;
