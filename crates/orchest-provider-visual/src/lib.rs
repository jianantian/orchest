//! `orchest-provider-visual` — signed/polled generation (the gen weight tier).
//!
//! **Internal crate — not for direct use.** It is published only because
//! `orchest-provider` depends on it; its API carries no SemVer guarantee and
//! `orchest-provider` pins it to an exact version (ADR-0003 D2). Depend on
//! `orchest-provider` instead.
//!
//! Houses the `GenTask` impls (submit → poll → fetch) abstracted from the old
//! concrete `ImageGateway`/video gateway, absorbing `agent-runtime-aigc-providers`
//! (Issue 007). Each dialect is one module under [`gen`]; `gen_entries` registers
//! them through the wall.

pub mod gen;

use orchest_protocol::GenTask;
use orchest_provider_core::registry::Entry;

/// Signed/polled gen-task dialects (renderful; volc-visual / aliyun / crazyrouter
/// follow). Construction is synchronous (the submit/poll/fetch HTTP happens in the
/// `GenTask` calls), so it fits the sync factory.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn gen_entries() -> Vec<Entry<Box<dyn GenTask>>> {
    vec![
        Entry::new(gen::renderful::entry_descriptor(), |cfg| {
            Ok(Box::new(gen::renderful::from_provider_config(cfg)?) as Box<dyn GenTask>)
        }),
        Entry::new(gen::aliyun::entry_descriptor(), |cfg| {
            Ok(Box::new(gen::aliyun::from_provider_config(cfg)?) as Box<dyn GenTask>)
        }),
        Entry::new(gen::crazyrouter::entry_descriptor(), |cfg| {
            Ok(Box::new(gen::crazyrouter::from_provider_config(cfg)?) as Box<dyn GenTask>)
        }),
        Entry::new(gen::volcengine::entry_descriptor(), |cfg| {
            Ok(Box::new(gen::volcengine::from_provider_config(cfg)?) as Box<dyn GenTask>)
        }),
        Entry::new(gen::volcengine_video::entry_descriptor(), |cfg| {
            Ok(Box::new(gen::volcengine_video::from_provider_config(cfg)?) as Box<dyn GenTask>)
        }),
    ]
}
