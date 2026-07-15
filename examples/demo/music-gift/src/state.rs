//! Shared application state extracted from route handlers so every layer
//! (routes, tools, agent) can depend on it without circular imports.

use std::path::PathBuf;
use std::sync::Arc;

use orchest::tool::Tool;
use orchest_protocol::{ChatModel, GenTask};


use crate::gift::GiftStore;

#[derive(Clone)]
pub struct AppState {
    pub chat_model: Arc<dyn ChatModel>,
    pub countdown_model: Arc<dyn ChatModel>,
    pub music_prompt_model: Arc<dyn ChatModel>,
    pub gen_task: Arc<dyn GenTask>,
    pub gift_store: GiftStore,
    #[allow(dead_code)]
    pub data_dir: PathBuf,
    pub countdown_tool: Option<Arc<dyn Tool>>,
}
