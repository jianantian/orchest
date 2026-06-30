//! Signed / polled image+video generation dialects (the gen weight tier). These
//! implement the spine [`orchest_protocol::GenTask`] (submit → poll → fetch) over
//! `reqwest`, abstracted from the old concrete `ImageGateway`. Registered through
//! the wall via [`crate::gen_entries`].

pub mod aliyun;
pub mod renderful;
