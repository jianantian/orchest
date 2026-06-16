//! Wire/domain types for the crate, split by concern:
//!
//! - [`common`] — shared between image and video (asset refs, error type,
//!   provider status, runtime config).
//! - [`image`] — image generation request/response/capability types.
//! - [`video`] — video generation (async task) request/response types.
//!
//! Everything is re-exported flat at the crate root, so callers don't need to
//! know about this split — `agent_runtime_aigc_providers::ImageGenerationRequest`
//! works the same as before.

pub mod common;
pub mod image;
pub mod video;

pub use common::*;
pub use image::*;
pub use video::*;
