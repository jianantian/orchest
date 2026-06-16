//! Static model catalogs for discovery — no credentials needed.
//!
//! Split by media type: [`image`] for image generation models/providers,
//! [`video`] for video generation. Both are re-exported flat at the crate
//! root for backwards compatibility.

pub mod image;
pub mod video;

pub use image::{list_models, list_providers, ImageModelEntry, ImageModelList, ImageProviderInfo};
pub use video::{list_video_models, list_video_providers, VideoModelEntry, VideoProviderInfo};
