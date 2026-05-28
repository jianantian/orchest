//! Core message, content, role, and tool definition types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContentBlock {
    Text(String),
    Thinking {
        text: Option<String>,
        signature: Option<String>,
        provider_details: Option<Value>,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Value,
    },
}

// ---------------------------------------------------------------------------
// Tool definition
// ---------------------------------------------------------------------------

pub type JsonSchema = serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}

// ---------------------------------------------------------------------------
// Model spec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub provider: String,
    pub model: String,
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_url: Option<String>,
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub context_window_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProviderRuntimeConfig {
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

impl From<ModelSpec> for ProviderRuntimeConfig {
    fn from(value: ModelSpec) -> Self {
        Self {
            model: format!("{}/{}", value.provider, value.model),
            api_key: None,
            api_key_env: value.api_key_env,
            api_url: value.api_url,
            max_tokens: value.max_tokens,
        }
    }
}
