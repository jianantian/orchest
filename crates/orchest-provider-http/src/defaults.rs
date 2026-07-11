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

pub mod mureka {
    /// 默认 API URL。锚点: Mureka API Platform quickstart (`https://api.mureka.ai`)。
    pub const API_URL: &str = "https://api.mureka.ai";
    pub const API_KEY_ENV: &str = "MUREKA_API_KEY";
    pub const API_URL_ENV: &str = "MUREKA_API_URL";
    pub const DEFAULT_MODEL: &str = "auto";
}

pub mod aliyun_music {
    /// 默认 API URL。锚点: `docs/external/aliyun/music-generation.md`
    /// (`https://dashscope.aliyuncs.com/api/v1/services/audio/music/generation`)。
    /// Workspace 专属域名 `{WorkspaceId}.cn-beijing.maas.aliyuncs.com` 也可用。
    pub const API_URL: &str =
        "https://dashscope.aliyuncs.com/api/v1/services/audio/music/generation";
    pub const API_KEY_ENV: &str = "DASHSCOPE_API_KEY";
    pub const API_URL_ENV: &str = "DASHSCOPE_API_URL";
    pub const DEFAULT_MODEL: &str = "fun-music-v1";
}

pub mod suno {
    /// 默认 API URL。锚点: Suno API (`https://api.sunoapi.org`)。
    /// Suno 无官方公开 API,此处为第三方代理;base URL 可通过 `SUNO_API_URL` 覆盖。
    pub const API_URL: &str = "https://api.sunoapi.org";
    pub const API_KEY_ENV: &str = "SUNO_API_KEY";
    pub const API_URL_ENV: &str = "SUNO_API_URL";
    pub const DEFAULT_MODEL: &str = "V5_5";
}

pub mod elss {
    /// 默认 API URL。锚点: Elss API gateway (`https://api.elss.ai`)。
    /// Elss 是 OpenAI/Anthropic 兼容的 API 代理,同时接受 `x-api-key` 和
    pub const API_URL: &str = "https://api.elss.ai";
    pub const API_KEY_ENV: &str = "ELSS_API_KEY";
    pub const API_URL_ENV: &str = "ELSS_API_URL";
}
