//! Realtime omni provider experiments for the Orchest runtime.
//!
//! This crate is intentionally separate from ASR/TTS/LLM provider crates because
//! realtime omni sessions interleave audio input, transcript/model text, audio
//! output, lifecycle events and provider-specific control events in one duplex
//! interaction.
#![allow(clippy::result_large_err)]

pub mod error;
pub mod providers;

pub use error::{RealtimeError, RealtimeErrorCode};
