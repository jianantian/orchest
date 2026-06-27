//! `orchest-provider-visual` — signed/polled generation (the gen weight tier).
//!
//! **Skeleton (Issue 004).** The `GenTask` impls (volc-visual, aliyun,
//! crazyrouter, renderful) abstracted from the concrete `ImageGateway`/video
//! gateway, plus pricing reconciliation, are added in Issue 007, which also
//! dissolves `agent-runtime-aigc-providers`. The entry-producing function below
//! returns an empty vector for now.

use orchest_protocol::GenTask;
use orchest_provider_core::registry::Entry;

/// Signed/polled gen-task dialects (volc-visual, aliyun, …). Filled in Issue 007.
pub fn gen_entries() -> Vec<Entry<Box<dyn GenTask>>> {
    Vec::new()
}
