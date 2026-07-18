//! Shared application state extracted from route handlers so every layer
//! (routes, tools, agent) can depend on it without circular imports.

use std::path::PathBuf;
use std::sync::Arc;

use orchest::tool::Tool;
use orchest_protocol::{ChatModel, GenTask};


use crate::auth::AuthStore;
use crate::gift::GiftStore;

#[derive(Clone)]
pub struct AppState {
    pub chat_model: Arc<dyn ChatModel>,
    pub music_prompt_model: Arc<dyn ChatModel>,
    pub gen_task: Arc<dyn GenTask>,
    /// Music provider identity resolved at startup (see `config::load_config`).
    pub music_provider: String,
    pub gift_store: GiftStore,
    pub auth_store: AuthStore,
    pub data_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub countdown_tool: Option<Arc<dyn Tool>>,
}
