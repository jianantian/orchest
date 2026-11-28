//! Static model catalog tables for the stream weight tier.

pub mod asr;
pub mod tts;

pub use asr::{stream_asr_models, STREAM_ASR_MODELS};
pub use tts::{stream_tts_models, STREAM_TTS_MODELS};
