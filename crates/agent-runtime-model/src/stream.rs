use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::response::TokenUsage;

// ---------------------------------------------------------------------------
// Streaming events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamEvent {
    Text {
        delta: String,
    },
    ThinkingStart,
    Thinking {
        delta: String,
    },
    ThinkingEnd {
        signature: Option<String>,
        provider_details: Option<Value>,
    },
    ToolUseStart {
        id: String,
        name: String,
    },
    ToolUseArgsChunk {
        id: String,
        delta: String,
    },
    ToolUseEnd {
        id: String,
    },
    Done {
        usage: TokenUsage,
    },
}
