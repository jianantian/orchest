//! Decisions HTTP dialect (the TypeSafe wire) and its deployments: OpenRouter
//! Decisions and Vercel AI Gateway.

mod catalog;
mod typesafe;
mod wire;

pub use catalog::{decision_entries, decision_models};
