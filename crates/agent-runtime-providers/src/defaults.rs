//! Per-provider configuration defaults. All provider constants live here so
//! they can be found and updated in one place.

pub const MAX_TOKENS: u32 = 4096;

pub mod anthropic {
    pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
    pub const API_VERSION: &str = "2023-06-01";
    pub const API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
    pub const AUTH_TOKEN_ENV: &str = "ANTHROPIC_AUTH_TOKEN";
    pub const API_URL_ENV: &str = "ANTHROPIC_API_URL";
}

pub mod openai {
    pub const API_URL: &str = "https://api.openai.com/v1/chat/completions";
    pub const API_KEY_ENV: &str = "OPENAI_API_KEY";
}

pub mod deepseek {
    pub const API_URL: &str = "https://api.deepseek.com";
    pub const API_KEY_ENV: &str = "DEEPSEEK_API_KEY";
}

pub mod openrouter {
    pub const API_URL: &str = "https://openrouter.ai/api";
    pub const API_KEY_ENV: &str = "OPENROUTER_API_KEY";
}

pub mod volcengine {
    /// Base URL for Volcengine Ark (火山方舟) OpenAI-compatible Chat API.
    pub const API_URL: &str = "https://ark.cn-beijing.volces.com/api/v3/chat/completions";
    pub const API_KEY_ENV: &str = "ARK_API_KEY";
}

pub mod minimax {
    /// 默认 API URL。锚点:`docs/external/minimax/llm/api.md:37`(`https://api.minimaxi.com`),
    /// 同源 `/anthropic/v1/messages`。`normalize_messages_url` 会自动补 `/v1/messages`。
    pub const API_URL: &str = "https://api.minimaxi.com/anthropic/v1/messages";
    pub const API_KEY_ENV: &str = "MINIMAX_API_KEY";
    pub const API_URL_ENV: &str = "MINIMAX_API_URL";
}
