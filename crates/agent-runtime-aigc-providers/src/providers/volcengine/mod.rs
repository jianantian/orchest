//! Volcengine Ark (火山方舟) adapters — one provider, two capabilities.

pub mod image;
pub mod video;

pub use image::{VolcengineImageAdapter, VolcengineImageConfig};
pub use video::{VolcengineVideoAdapter, VolcengineVideoConfig};
