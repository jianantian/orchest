//! REST gen-task dialects (the light weight tier). Music generation dialects
//! (Minimax, Mureka, Aliyun fun-music, Suno) implementing the spine
//! [`orchest_protocol::GenTask`]. Signed/polled image+video generation lives
//! in `orchest-provider-visual`. Registered through the wall via
//! [`crate::gen_entries`].

pub mod aliyun_music;
pub mod minimax_music;
pub mod mureka;
pub mod suno;
