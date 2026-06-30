//! Signed / polled image+video generation dialects (the gen weight tier). These
//! implement the spine [`orchest_protocol::GenTask`] (submit → poll → fetch) over
//! `reqwest`, abstracted from the old concrete `ImageGateway`. Registered through
//! the wall via [`crate::gen_entries`].
//!
//! Synchronous upstreams (the result returns inline from submit, with no job id
//! to poll) reuse [`SyncGenCache`](orchest_provider_core::SyncGenCache) to present
//! the submit → poll → fetch surface.

pub use orchest_provider_core::SyncGenCache;

pub mod aliyun;
pub mod crazyrouter;
pub mod renderful;
pub mod volcengine;
pub mod volcengine_video;
