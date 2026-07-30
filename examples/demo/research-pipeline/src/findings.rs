//! Canonical Research Pipeline finding/evidence contract and projection.

mod model;
mod render;
mod validation;

pub use model::*;
pub use render::{is_report_path, render_markdown};
pub use validation::{is_safe_repository_path, validate_document, validate_json};
