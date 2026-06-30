//! REST gen-task dialects (the light weight tier). Synchronous REST generation
//! (e.g. Minimax music) implementing the spine [`orchest_protocol::GenTask`].
//! Signed/polled image+video generation lives in `orchest-provider-visual`.
//! Registered through the wall via [`crate::gen_entries`].

pub mod minimax_music;
