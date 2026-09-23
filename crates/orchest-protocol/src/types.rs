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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
    /// Minimax-only: 设定用户的角色和人设(角色扮演场景中定义用户身份)。
    /// 非 Minimax provider 在请求构造时记录 OptionAdjustment 并降级为 `System` 语义。
    UserSystem,
    /// Minimax-only: 对话分组 / 场景名称。降级为 `User`。
    Group,
    /// Minimax-only: few-shot 示例的用户输入。降级为 `User`。
    SampleMessageUser,
    /// Minimax-only: few-shot 示例的模型输出。降级为 `User`。
    SampleMessageAi,
}

/// 多模态资源来源:URL 引用或 base64 内联。
///
/// 对齐 Minimax `MediaSource`(`docs/external/minimax/llm/api.md:1245-1305`)与
/// Anthropic image source 形态(`{type:url}` / `{type:base64, media_type, data}`)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "type")]
#[non_exhaustive]
pub enum MediaSource {
    /// 远程 URL。
    Url { url: String },
    /// base64 内联,需附带 MIME 类型(例如 `image/png` / `video/mp4`)。
    Base64 { media_type: String, data: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
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
    /// 图片输入 block。`detail` 是 Minimax / OpenAI vision 的可选粗细度参数。
    Image {
        source: MediaSource,
        detail: Option<String>,
    },
    /// 视频输入 block。`fps` 与 `max_long_side_pixel` 为 Minimax 专属字段。
    Video {
        source: MediaSource,
        fps: Option<f32>,
        detail: Option<String>,
        max_long_side_pixel: Option<u32>,
    },
    /// 音频输入 block —— Step 2 omni 前向占位,当前迭代无 provider 序列化它。
    Audio {
        source: MediaSource,
    },
    /// Minimax-only: 对话中途插入的系统指令(`mid_conv_system`)。
    MidConvSystem(String),
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
