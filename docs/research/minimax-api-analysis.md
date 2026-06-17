# Minimax API 接入设计

> 目标:把 `docs/external/minimax/` 下 16 个 API 落到现有 Orchest crate 中。
> 每个 API 给出归属 crate、模块路径、复用的 trait/类型、需扩展的字段,以及**对应的供应商文档行号锚点**。
>
> 文档规范:本文每一个设计决策都必须引用 `docs/external/minimax/<file>:<行号>` 或具体 crate 路径。
> 没有引用的"应该""可能"必须删掉或补来源。
>
> 日期: 2026-06-14 | 状态: 设计阶段

---

## 一、API → Crate 路由表

| # | Minimax API | 文档锚点 | 归属 crate | 新增模块 | 复用 trait/抽象 |
|---|---|---|---|---|---|
| 1 | `POST /anthropic/v1/messages` | `docs/external/minimax/llm.md:25,42` | `agent-runtime-providers` | `providers/minimax/` | `ModelAdapter`,套 `anthropic/{request,response}.rs` |
| 2 | `WSS /ws/v1/t2a_v2` (同步 TTS) | `docs/external/minimax/tts_sync.md:23` | `agent-runtime-tts-providers` | `providers/minimax/sync.rs` + feature `minimax` | `TtsProvider::stream_synthesize` / `start_duplex_stream` |
| 3 | `POST /v1/t2a_async_v2` | `docs/external/minimax/tts_async.md:39,54` | `agent-runtime-tts-providers` | `providers/minimax/async.rs` | `TtsProvider::synthesize` + 新 `TtsOperation::Async` |
| 4 | `POST /v1/voice_clone` | `docs/external/minimax/voice_clone/clone.md:15,28` | `agent-runtime-tts-providers` | `providers/minimax/voice.rs` | 新 trait `VoiceManager` (见 §3.5) |
| 5 | `POST /v1/files/upload` | `docs/external/minimax/voice_clone/voice-upload.md:13,26` 和 `voice_clone/example-upload.md:13,26` | `agent-runtime-tts-providers` | `providers/minimax/files.rs` (内部 helper) | `reqwest::multipart` |
| 6 | `POST /v1/voice_design` | `docs/external/minimax/voice_design.md:25` | `agent-runtime-tts-providers` | `providers/minimax/voice.rs` | 同 #4 |
| 7 | `POST /v1/delete_voice` | `docs/external/minimax/delete_voice.md:16,29` | `agent-runtime-tts-providers` | `providers/minimax/voice.rs` | 同 #4 |
| 8 | `POST /v1/music_generation` | `docs/external/minimax/music/generation.md:13,28` | **新** `agent-runtime-music-providers` | `lib.rs` 整 crate | 新 trait `MusicProvider` (见 §4) |
| 9 | `POST /v1/lyrics_generation` | `docs/external/minimax/music/lyrics.md:13,26` | 同上 | `lyrics.rs` | `MusicProvider::generate_lyrics` |
| 10 | `POST /v1/music_cover_preprocess` | `docs/external/minimax/music/cover.md:13,28` | 同上 | `cover.rs` | `MusicProvider::preprocess_cover` |
| 11 | `POST /v1/video_generation` (T2V) | `docs/external/minimax/video/t2v.md:25` | `agent-runtime-aigc-providers` | `providers/minimax.rs` | `VideoProvider::create_video_generation` |
| 12 | `POST /v1/video_generation` (I2V) | `docs/external/minimax/video/i2v.md:13,26` | 同上 | 同上 | 同上 |
| 13 | `POST /v1/video_generation` (Frame2V) | `docs/external/minimax/video/frame2v.md:13,26` | 同上 | 同上 | 同上 |
| 14 | `POST /v1/video_generation` (Subject Ref) | `docs/external/minimax/video/refvideo.md:13,26` | 同上 | 同上 | 同上 |
| 15 | `GET /v1/query/video_generation` | `docs/external/minimax/video/status.md:13,26` | 同上 | 同上 | `VideoProvider::get_video_generation` + `VideoGateway::wait_for_completion` |
| 16 | `GET /v1/files/retrieve` | `docs/external/minimax/video/retrive.md:13,26` | 同上 | 同上(内部下载) | gateway 已有 asset 持久化 |

---

## 二、LLM 接入 — `/anthropic/v1/messages`

**归属**: `crates/agent-runtime-providers/`
**模板**: `crates/agent-runtime-providers/src/providers/anthropic/{mod,request,response}.rs`
**供应商文档**: `docs/external/minimax/llm.md`

### 2.1 协议兼容性 (可直接套现有代码)

Minimax LLM 自称 "Anthropic API 兼容 Messages 格式" (`llm.md:7`)。验证:

| 项 | Minimax 字段 | 现有 Anthropic adapter | 锚点 |
|---|---|---|---|
| 路径 | `POST /anthropic/v1/messages` | 同 | `llm.md:25,42` |
| 鉴权 | `Authorization: Bearer` + `x-api-key` 任一 | `Bearer` | `llm.md:38-40` |
| SSE 事件 | `message_start` / `content_block_delta` / `message_stop` / `thinking_delta` | 同 (见 `anthropic/response.rs:118`) | `llm.md:1019` |
| `thinking` schema | `{type: "adaptive" \| "disabled"}` + `display: summarized\|omitted` | 同 (见 `anthropic/request.rs:107-128`) | `llm.md:880-898` |
| `tool` / `tool_choice` / `tool_use` / `tool_result` | 同 Anthropic | 同 | (内嵌于 Messages schema) |

**结论**: `ModelAdapter` trait 不动,直接以 `AnthropicAdapter` 为模板做 `MinimaxAdapter`。

### 2.2 实现步骤

1. **新建 `providers/minimax/{mod,request,response}.rs`**,以 `providers/anthropic/` 为骨架 fork
   - v0.10 第一刀直接拷;后续若两个 adapter 共同点 ≥80%,再抽 `messages_common.rs`
2. **注册到 `ProviderRegistry`**: `crates/agent-runtime-providers/src/registry.rs` + `providers/mod.rs`
   - 前缀: `minimax/MiniMax-M3`、`minimax/MiniMax-M2.7` 等
3. **Catalog 条目** (`catalog.rs`):MiniMax-M3 / M2.7 / M2.5 / M2.1,定价待补
4. **默认 API URL**: `https://api.minimaxi.com` (见 `llm.md:36-37`)
5. **鉴权**: 用 `Authorization: Bearer ${api_key}` 与 `AnthropicAdapter` 行为对齐 (`llm.md:38-40` 允许两种)

### 2.3 需要扩展的字段

`ContentBlock` 当前定义在 `crates/agent-runtime-model/src/types.rs:25-41`,**只有 4 个 variant**: `Text` / `Thinking` / `ToolUse` / `ToolResult`——**没有 `Image` variant**(已验证 Anthropic adapter 内无 image 序列化逻辑)。Minimax 接入要把多模态从零搭起。

每项在 `crates/agent-runtime-model/src/types.rs` 加 variant/字段,其他 provider 走 `CompatibilityPolicy::Coerce` + `OptionAdjustment` 路径(参考 `crates/agent-runtime-providers/src/providers/openai.rs:243-251`)。

| 字段 | 锚点 | 当前是否支持 | 动作 |
|---|---|---|---|
| `image` content block | `llm.md:843,1188`(`ContentBlock` 当前列表) | ❌ 全无 | `ContentBlock::Image { source: MediaSource, detail: Option<String> }` 新增 variant + 顺带让现有 Anthropic adapter 也实现 image 序列化(`Anthropic Messages API` 本就支持) |
| `video` content block | `llm.md:843,1188,1334-1343` | ❌ | `ContentBlock::Video { source: MediaSource, fps, detail, max_long_side_pixel }` 新增 variant,Minimax 专属字段(`fps` / `max_long_side_pixel`)挂在此 variant 上 |
| `mid_conv_system` block | `llm.md:1136,1202-1211` | ❌ | `ContentBlock::MidConvSystem(String)` 新增 variant |
| `user_system` / `group` / `sample_message_user` / `sample_message_ai` 角色 | `llm.md:1088-1091` | ❌ (当前 `Role` 是固定 enum,位于 `crates/agent-runtime-model/src/types.rs:16-22`) | 给 `Role` 加 4 个 variant,序列化时只 Minimax adapter 输出这些值 |
| `service_tier`: `standard\|priority` | `llm.md:807,360,548` | ❌ | `RequestOptions.service_tier: Option<String>`(在 `agent-runtime-model/src/options.rs`)|

**命名冲突警告**: `crates/agent-runtime-aigc-providers/src/types/video.rs:88` 已有 `service_tier: Option<String>` 字段,但语义是"图片/视频生成调用优先级",与 LLM 的"standard / priority" 不同语义。两者不要互相借用;若担心读者混淆,可在 `agent-runtime-model` 的字段文档注释里明示。

**Image 实现的副作用**: 给 `ContentBlock` 加 `Image` variant 不只是给 Minimax 用——现有 Anthropic adapter 也应该顺势支持,否则 Anthropic 多模态用户继续被卡。但这超出 Minimax 接入范围,作为 follow-up issue 立项更合适。**v0.10 第一刀**: 加 variant + 在 MinimaxAdapter 内序列化,Anthropic adapter 暂时 `ContentBlock::Image { .. } => unimplemented!()` 或返回 `OptionAdjustment`,**不影响 Anthropic 现状**(没人在用)。

### 2.4 验证清单

- [ ] 用 `MiniMax-M3` 跑一次纯文本 + `thinking: adaptive`,确认 `ThinkingStart` / `Thinking { delta }` / `ThinkingEnd` 事件序列
- [ ] 用 `image` content block 跑一次多模态(依赖 §2.3 新增 `ContentBlock::Image`)
- [ ] 用 `tool_use` 跑一次 function calling,确认 `StopReason::ToolUse` 路径
- [ ] 用 `video` content block 跑一次(依赖 §2.3 新增 `ContentBlock::Video`)
- [ ] 用 4 个 Minimax-only Role 之一跑一次(依赖 §2.3 `Role` 扩展)

---

## 三、TTS 与 Voice (5 个 API)

**归属**: `crates/agent-runtime-tts-providers/`
**模板**:
- WSS → `providers/volcengine/{bidirectional,unidirectional}.rs`
- 单文件 provider → `providers/aliyun/mod.rs`

**供应商文档**:
- 同步 TTS: `docs/external/minimax/tts_sync.md`
- 异步 TTS: `docs/external/minimax/tts_async.md`
- Voice Clone: `docs/external/minimax/voice_clone/clone.md`
- Voice Upload: `docs/external/minimax/voice_clone/voice-upload.md`、`example-upload.md`
- Voice Design: `docs/external/minimax/voice_design.md`
- Delete Voice: `docs/external/minimax/delete_voice.md`

### 3.1 模块布局

```
crates/agent-runtime-tts-providers/src/providers/minimax/
├── mod.rs        # MinimaxTtsAdapter + impl TtsProvider + impl VoiceManager
├── protocol.rs   # task_start/continue/finish JSON 帧 + hex 音频解码 + base_resp 错误码映射
├── sync.rs       # WSS 客户端
├── async.rs      # POST /v1/t2a_async_v2
├── files.rs      # POST /v1/files/upload (internal helper)
└── voice.rs      # voice_clone / voice_design / delete_voice
```

`Cargo.toml`(与 volcengine/aliyun feature 平行):

```toml
[features]
minimax = ["dep:futures-util", "dep:tokio-tungstenite", "dep:base64", "dep:hex"]

[dependencies]
reqwest = { version = "0.12", features = ["json", "multipart"] }  # 新增 multipart
hex = { version = "0.4", optional = true }
```

`tokio-tungstenite` 已是其他 provider 的 optional dep(见当前 `Cargo.toml:9-10,23`),feature 复用同一依赖。`multipart` 在 `crates/agent-runtime-aigc-providers/Cargo.toml:12` 已验证可用。

### 3.2 TTS 同步 (WSS) — 实现要点

**端点**: `wss://api.minimaxi.com/ws/v1/t2a_v2`(`tts_sync.md:20-23`)

**协议帧序列**(`tts_sync.md:34-1320`):

```
C → { event: "task_start", model, voice_setting, audio_setting, ... }              # tts_sync.md:34-684
S → { event: "task_started" }                                                       # tts_sync.md:924-1009
C → { event: "task_continue", text: "..." }                                         # tts_sync.md:717-793
S → { event: "task_continued", data: { audio: "<hex>" }, extra_info, base_resp }   # 可多次,tts_sync.md:1011-1232
C → { event: "task_finish" }                                                        # tts_sync.md:794-831
S → { event: "task_finished", data: { audio: "<hex>" }, extra_info }                # tts_sync.md:1250-1320
```

**关键事实**(`tts_sync.md:1023-1026, 1113-1124`):
- 全程 **JSON 文本帧**,服务端不发二进制
- 音频在 `data.audio` 以 **hex 字符串** 编码,格式遵从 `audio_setting.format` (mp3/pcm/flac)
- `data` 可能为 `null`,要做非空判断(`tts_sync.md:1020`)
- `is_final: bool` 标志最终块(`tts_sync.md:1039-1041`)

落到 `TtsProvider` trait(`crates/agent-runtime-tts-providers/src/traits.rs:11-29`):

| trait 方法 | 实现策略 |
|---|---|
| `synthesize` | sync WSS: start → 单条 continue → finish,聚合所有 `data.audio` hex 解码后返回 `SynthesizeResult { audio: AudioData::Bytes(...) }` |
| `stream_synthesize` | 同上,每收到一帧立刻 push `TtsStreamEvent::Audio` |
| `start_duplex_stream` | 双工:把 `DuplexSynthesizeRequest` 的输入 channel 映射到 `task_continue` 序列 |

**鉴权**: WSS 握手时通过 `Authorization: Bearer ${api_key}` header 传(`tts_sync.md` 末尾 Python 示例段)。

**错误码映射**(`tts_sync.md:1091-1106`,16 个 `base_resp.status_code` → `TtsErrorCode`,后者 16 个 variant 见 `crates/agent-runtime-tts-providers/src/error.rs:5-24`):

| Minimax 状态码 | 含义 | 映射到 `TtsErrorCode` |
|---|---|---|
| 0 | 正常 | — |
| 1001 / 2201 | 请求超时 / 超时断开连接 | `Timeout` |
| 1002 / 1039 / 2205 | 限流 (QPS / TPM / 请求超限) | `ProviderHttpError`(`status = 429`,`upstream_code` 保留原码) |
| 1004 | 鉴权失败 | `InvalidApiKey` |
| 1042 / 2203 / 2204 | 非法字符 > 10% / 空文本 / 超字符限制 | `InvalidRequest` |
| 2013 | 参数信息不正常 | `InvalidRequest` |
| 2202 | 非法事件 | `ProviderStreamError` |
| 1000 / 其他 | 未知 | `ProviderTaskFailed` |

`TtsErrorCode` 当前**不区分 rate-limit 与一般 HTTP 错误**,故 1002/1039/2205 复用 `ProviderHttpError` 但通过 `TtsError.upstream_code` 保留 Minimax 原码。如果后续需要 rate-limit 重试策略,再补 `RateLimited` variant。

### 3.3 TTS 异步 — 实现要点

**端点**: `POST /v1/t2a_async_v2`(`tts_async.md:39,54`)

**请求结构**(`tts_async.md:74-99`):
- 输入: `text` (≤5万字符) 或 `text_file_id` (txt/zip,≤100万字符) 二选一(`tts_async.md:156,177,181`)
- `voice_setting` / `audio_setting` / `pronunciation_dict` / `voice_modify` 与同步 TTS 一致

**响应结构**(`tts_async.md:133-148, 249-264`):

```yaml
task_id: int64
task_token: string   # JWT
file_id: int64       # 任务完成后用于下载
base_resp: { status_code, status_msg }
```

**没有独立的 query 端点**——`tts_async.md` 全文只有一个 `paths` 条目(`tts_async.md:53-54`)。文件就绪通过 `/v1/files/retrieve?file_id=...` 拉取(与视频共用同一 Minimax 端点)。

**下载逻辑归属决策**(aigc crate 和 tts crate 都要打这个端点):

| 选项 | 优点 | 缺点 |
|---|---|---|
| A. tts crate 内复制一份小 helper | 实现快,无跨 crate 依赖 | DRY 违反,日后改动两处 |
| B. 抽到 `agent-runtime-model` 或新 `agent-runtime-http` 共享 crate | 单点维护 | model crate 当前不带 reqwest 依赖(见 `crates/agent-runtime-model/src/`,仅有 types/options/error),引入会扩大其依赖图 |
| C. tts crate 通过 `dep:agent-runtime-aigc-providers` 复用 | 不写新代码 | tts 依赖 aigc 是错误方向(aigc 反过来不该依赖 tts,但单向也奇怪) |

**决策**: **A**(v0.10 先复制)。Minimax 的 `/v1/files/retrieve` 调用很薄(GET + JSON 解析 + `download_url` 提取),两处复制可控;v0.11+ 若出现第三方调用方再抽。

**实现策略**:
1. `SynthesizeRequest` 的 `TtsOperation` enum(`types.rs:327-334`)增加 `Async` variant
2. `MinimaxTtsAdapter::synthesize` 根据 `request.operation` 分派到 sync WSS 或 async HTTP
3. async 路径返回 `SynthesizeResult { audio: AudioData::Url(...) }`,"是否预下载"留给上层
4. `text_file_id` 路径调用 §3.4 上传 helper 拿 file_id

### 3.4 文件上传 — `/v1/files/upload`

**端点**: `POST /v1/files/upload`
- voice_clone purpose: `voice_clone/voice-upload.md:13,26`
- prompt_audio purpose: `voice_clone/example-upload.md:13,26`

**不暴露给 trait**——它是 Voice Clone(§3.5)与 Async TTS text_file_id(§3.3)的内部前置步骤。

**约束**(`voice_clone/clone.md:55-65`):
- voice_clone 音频: mp3/m4a/wav, 10s-5min, ≤20MB

**实现**(`crates/agent-runtime-tts-providers/src/providers/minimax/files.rs`):

```rust
/// Minimax 文件上传的合法 `purpose` 值。
/// 来源: voice_clone/voice-upload.md:13,26 与 example-upload.md:13,26
#[derive(Debug, Clone, Copy)]
pub(super) enum FilePurpose {
    /// 音色复刻原始音频
    VoiceClone,
    /// 复刻时的"示例音频"(clone_prompt.prompt_audio)
    PromptAudio,
    /// 异步 TTS 的长文本输入(tts_async.md:181)
    T2aAsyncInput,
}

impl FilePurpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::VoiceClone => "voice_clone",
            Self::PromptAudio => "prompt_audio",
            Self::T2aAsyncInput => "t2a_async_input",
        }
    }
}

pub(super) async fn upload_file(
    client: &reqwest::Client,
    api_key: &str,
    purpose: FilePurpose,
    file_bytes: Vec<u8>,
    filename: String,
    mime: &str,
) -> Result<u64, TtsError> {
    let form = reqwest::multipart::Form::new()
        .text("purpose", purpose.as_str())
        .part(
            "file",
            reqwest::multipart::Part::bytes(file_bytes)
                .file_name(filename)
                .mime_str(mime)
                .map_err(|e| TtsError::new(TtsErrorCode::InvalidRequest, e.to_string()))?,
        );
    // POST https://api.minimaxi.com/v1/files/upload
    // 解析 { file: { file_id } } 返回 file_id
}
```

参考 `crates/agent-runtime-aigc-providers/src/providers/crazyrouter.rs:6,387` 的 multipart 用法。`TtsErrorCode::InvalidRequest` 来自 `error.rs:18`(`TtsErrorCode::InvalidInput` 不存在;§3.2 错误码表已对齐实际 variant)。

### 3.5 Voice Management — 3 个 API

**端点**:
- Voice Clone: `POST /v1/voice_clone`(`voice_clone/clone.md:15,28`)
- Voice Design: `POST /v1/voice_design`(`voice_design.md:25`)
- Delete Voice: `POST /v1/delete_voice`(`delete_voice.md:16,29`)

**与 `TtsProvider` trait 的关系**: 这 3 个不属于"合成"语义,**不应塞进 `TtsProvider`**——当前 `traits.rs:11-29` 只覆盖 synthesize / stream / duplex / list_voices。

**方案**: 新增独立 trait `VoiceManager`,放在 `crates/agent-runtime-tts-providers/src/traits.rs` 同文件:

```rust
#[async_trait]
pub trait VoiceManager: Send + Sync {
    async fn clone_voice(&self, req: CloneVoiceRequest)
        -> Result<CloneVoiceResponse, TtsError>;
    async fn design_voice(&self, req: DesignVoiceRequest)
        -> Result<DesignVoiceResponse, TtsError>;
    async fn delete_voice(&self, voice_id: &str, kind: VoiceKind) -> Result<(), TtsError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneVoiceResponse {
    pub voice: VoiceInfo,
    /// 试听 URL,仅当请求里同时给出 trial_text + trial_model
    pub demo_audio: Option<String>,
    /// 风控结果 (0-7),0 = 通过
    pub input_sensitive: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignVoiceResponse {
    pub voice: VoiceInfo,
    /// hex 编码的试听音频
    pub trial_audio: AudioData,
}
```

`MinimaxTtsAdapter` 同时 impl `TtsProvider` 和 `VoiceManager`。`VoiceKind` 已在 `types.rs:156-162` 定义(`System` / `Cloned` / `Designed` / `Custom`),`delete_voice` 的 `voice_type: "voice_cloning" | "voice_generation"`(`delete_voice.md`)直接从 `VoiceKind::{Cloned, Designed}` 映射。

#### Voice Clone — 请求/响应类型

**Rust 类型定义**(`crates/agent-runtime-tts-providers/src/types.rs` 同文件):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneVoiceRequest {
    /// 复刻原音频的 file_id,来自 §3.4 上传
    /// 锚点: voice_clone/clone.md:53,56
    pub file_id: u64,
    /// 自定义音色 ID,8-256 字符,字母开头
    /// 锚点: voice_clone/clone.md:54
    pub voice_id: String,
    /// 可选,提供示例音频提高相似度
    /// 锚点: voice_clone/clone.md:66-77,245-249
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone_prompt: Option<ClonePrompt>,
    /// 可选,与 `model` 同时存在时合成试听
    /// 锚点: voice_clone/clone.md:86,93
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_model: Option<String>,
    /// 锚点: voice_clone/clone.md:105
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_boost: Option<Language>,
    /// 锚点: voice_clone/clone.md:150
    #[serde(default)]
    pub need_noise_reduction: bool,
    /// 锚点: voice_clone/clone.md:154
    #[serde(default)]
    pub need_volume_normalization: bool,
    /// 锚点: voice_clone/clone.md:158
    #[serde(default)]
    pub aigc_watermark: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClonePrompt {
    /// 示例音频 file_id
    pub prompt_audio: u64,
    /// 示例音频对应的文本
    pub prompt_text: String,
}
```

**字段表**(`voice_clone/clone.md:50-260`):

| 字段 | Rust 类型 | 锚点 |
|---|---|---|
| `file_id` | `u64` 必填 | `voice_clone/clone.md:53,56` |
| `voice_id` | `String` 必填 | `voice_clone/clone.md:54` |
| `clone_prompt` | `Option<ClonePrompt>` | `voice_clone/clone.md:66-77,245-249` |
| `text` + `model` | `Option<String>` + `Option<String>`,**两者同时存在才合成试听** | `voice_clone/clone.md:86,93` |
| `language_boost` | `Option<Language>` | `voice_clone/clone.md:105` |
| `need_noise_reduction` / `need_volume_normalization` / `aigc_watermark` | `bool` 默认 false | `voice_clone/clone.md:150,154,158` |

**响应**: `demo_audio`(试听 URL,仅当 trial_text + trial_model 同时给出) + `input_sensitive.type`(风控 0-7)+ `extra_info`(试听元信息与计费,`voice_clone/clone.md` 响应段)。在 `VoiceInfo`(`types.rs:182-202`)以外用一个 `CloneVoiceResponse { voice: VoiceInfo, demo_audio: Option<String>, input_sensitive: u8 }` 包装。

**特殊规则**: 复刻得到的音色若 7 天内未正式调用,系统会删除(`voice_clone/clone.md:8`)。

#### Voice Design — 请求/响应类型

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignVoiceRequest {
    /// 音色描述,如"讲述悬疑故事的播音员,声音低沉富有磁性"
    pub prompt: String,
    /// 试听文本,≤500 字符
    pub preview_text: String,
    /// 可选,不传则自动生成 `ttv-voice-<ts>-<hash>`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,
}
```

字段表(`voice_design.md` 全文):

| 字段 | 类型 | 约束 |
|---|---|---|
| `prompt` | string | 音色描述 |
| `preview_text` | string | 试听文本,≤500 字符 |
| `voice_id` | string | 可选,不传则自动生成 |

**响应**: `voice_id` + `trial_audio`(hex 编码音频)。返回 `VoiceInfo { kind: Designed, .. }` + 单独的 `trial_audio: AudioData::Bytes`。

#### Delete Voice

`VoiceManager::delete_voice(voice_id, kind)` 内部映射:
- `VoiceKind::Cloned` → 请求体 `voice_type: "voice_cloning"`
- `VoiceKind::Designed` → 请求体 `voice_type: "voice_generation"`
- `VoiceKind::System` / `Custom` → 返回 `TtsError { code: UnsupportedOperation, .. }`(系统音色不可删,`delete_voice.md` 内说明)

### 3.6 验证清单

- [ ] 同步 WSS: `synthesize("你好")` 返回非空 `audio: AudioData::Bytes`,长度匹配 `extra_info.audio_size`
- [ ] 同步 WSS 流式: 至少收到 1 个 `task_continued` + 1 个 `task_finished` 帧
- [ ] 异步: `synthesize` with `TtsOperation::Async` 返回 `AudioData::Url`
- [ ] Voice Clone 全链路: upload → clone (with `text` + `model` 试听) → 用新 `voice_id` 跑一次 sync 合成
- [ ] Voice Design: `design_voice("低沉磁性男声")` → 用返回的 `voice_id` 跑一次 sync 合成
- [ ] Delete Voice: 删上一步的 voice_id,再合成应失败

---

## 四、Music — 新 crate `agent-runtime-music-providers`

**归属**: **新建** `crates/agent-runtime-music-providers/`(独立 crate,不并入 aigc)

**理由**: 音乐生成与图片/视频生成的参数体系差异大(歌词、翻唱预处理、流式 hex 输出),且 API 之间有强逻辑耦合(歌词 + 预处理 → 生成)。并入 aigc 会污染 `VideoProvider` / `ImageProvider` trait。后续若有第二家音乐 provider(如 Suno),复用价值明显。

**供应商文档**:
- Music Generation: `docs/external/minimax/music/generation.md`
- Lyrics Generation: `docs/external/minimax/music/lyrics.md`
- Music Cover Preprocess: `docs/external/minimax/music/cover.md`

### 4.1 Crate 布局

```
crates/agent-runtime-music-providers/
├── Cargo.toml
└── src/
    ├── lib.rs        # MusicProvider trait + create_music_provider_from_config
    ├── types.rs      # GenerateMusicRequest / GenerateLyricsRequest / CoverPreprocessRequest
    ├── error.rs      # MusicError (参考 tts/aigc error 形态)
    ├── catalog.rs    # 静态模型清单
    └── providers/
        ├── mod.rs
        └── minimax.rs
```

### 4.2 `MusicProvider` trait

```rust
#[async_trait]
pub trait MusicProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;

    async fn generate(&self, req: GenerateMusicRequest) -> Result<GenerateMusicResult, MusicError>;
    async fn stream_generate(&self, req: GenerateMusicRequest)
        -> Result<MusicStream, MusicError>;
    async fn generate_lyrics(&self, req: GenerateLyricsRequest)
        -> Result<GenerateLyricsResult, MusicError>;
    async fn preprocess_cover(&self, req: CoverPreprocessRequest)
        -> Result<CoverPreprocessResult, MusicError>;
}
```

### 4.3 Music Generation — `POST /v1/music_generation`(`music/generation.md:13,28`)

**模型**(`music/generation.md:64-77`): `music-2.6` / `music-cover` / `music-2.6-free` / `music-cover-free`

**通用参数**(`music/generation.md:78-141`):

| 字段 | 类型 | 锚点 | 说明 |
|---|---|---|---|
| `model` | enum | `music/generation.md:64-77` | 4 种之一 |
| `prompt` | string | `music/generation.md:78-88` | 风格描述,music-2.6 ≤2000 字符,music-cover 10-300 字符必填 |
| `lyrics` | string | `music/generation.md:90-106` | `\n` 分隔,支持结构标签,music-2.6 必填 ≤3500 字符 |
| `stream` | bool | `music/generation.md:112` | 流式返回 hex chunk |
| `output_format` | `url\|hex` | `music/generation.md:116-124` | 默认 `hex`;`stream=true` 时仅支持 `hex` |
| `audio_setting` | obj | `music/generation.md:125-128` | `{sample_rate, bitrate, format: mp3\|wav\|pcm}` |
| `aigc_watermark` | bool | `music/generation.md:129` | 仅非流式生效 |

**model 专属字段**:

| 字段 | 仅限模型 | 锚点 |
|---|---|---|
| `lyrics_optimizer: bool` | music-2.6 / music-2.6-free | `music/generation.md:131-136` |
| `is_instrumental: bool` | 同上 | `music/generation.md:138-141` |
| `audio_url` / `audio_base64` (二选一) | music-cover / music-cover-free | `music/generation.md:144-170` |
| `cover_feature_id` | 同上,与 audio_url/base64 互斥 | `music/generation.md:172-187` |

**响应**(`music/generation.md:213,254`): `data.audio` hex 字符串(或 URL),与 TTS WSS 同样 hex 编码模式。

### 4.4 Lyrics Generation — `POST /v1/lyrics_generation`(`music/lyrics.md:13,26`)

| 字段 | 说明 |
|---|---|
| `mode` | `write_full_song` 或 `edit` |
| `prompt` | 歌曲主题/风格,≤2000 字符 |
| `lyrics` | edit 模式下的已有歌词,≤3500 字符 |
| `title` | 可选指定标题 |

**响应**: `song_title` + `style_tags` + `lyrics`(含 14 种结构标签:`[Intro]` `[Verse]` `[Pre-Chorus]` `[Chorus]` `[Hook]` `[Drop]` `[Bridge]` `[Solo]` `[Build-up]` `[Instrumental]` `[Breakdown]` `[Break]` `[Interlude]` `[Outro]`,见 `music/lyrics.md` 响应段)。

### 4.5 Music Cover Preprocess — `POST /v1/music_cover_preprocess`(`music/cover.md:13,28`)

| 字段 | 说明 |
|---|---|
| `model` | `music-cover` |
| `audio_url` 或 `audio_base64` | 二选一 |

**约束**(`music/cover.md:75-76`): 时长 6 秒-6 分钟,大小 ≤50MB,格式 mp3/wav/flac

**响应**: `cover_feature_id`(24 小时有效,相同音频去重) + `formatted_lyrics`(ASR 提取) + `structure_result` + `audio_duration`(`music/generation.md:185-187` 交叉引用)

### 4.6 验证清单

- [ ] `generate_lyrics(write_full_song, "夏日恋爱")` 返回带结构标签的 lyrics
- [ ] `generate(music-2.6, prompt + lyrics, output_format=url)` 拿到 URL 并能下载
- [ ] `stream_generate(music-2.6, ..., stream=true)` 收到至少 1 个 hex chunk
- [ ] `preprocess_cover(audio_url)` → 拿 cover_feature_id → `generate(music-cover, cover_feature_id, prompt)` 全链路成功

---

## 五、Video — 扩展 `agent-runtime-aigc-providers`

**归属**: `crates/agent-runtime-aigc-providers/src/providers/minimax.rs`
**模板**: `providers/volcengine/video.rs`(同样的"提交 task → 轮询 → 下载"模式)

**供应商文档**: 5 个 video API 都打到 `POST /v1/video_generation`
- T2V: `docs/external/minimax/video/t2v.md:25`
- I2V: `docs/external/minimax/video/i2v.md:13,26`
- Frame2V: `docs/external/minimax/video/frame2v.md:13,26`
- Subject Ref: `docs/external/minimax/video/refvideo.md:13,26`
- 状态查询: `docs/external/minimax/video/status.md:13,26`
- 文件下载: `docs/external/minimax/video/retrive.md:13,26`

### 5.1 直接复用的现有能力

- `VideoProvider` trait(`crates/agent-runtime-aigc-providers/src/types/video.rs:162-180`)的 `create_video_generation` + `get_video_generation` 形态完全匹配 Minimax 的"提交+轮询"模式
- `VideoGateway::generate`(`gateway/video.rs:56`)已封装"创建 → `wait_for_completion` 轮询到终态 → 下载并持久化"全链路
- `VideoExecutionConfig.poll_interval`(`types/video.rs:26-31`)默认 5s,与视频生成节奏匹配
- gateway 的 asset 持久化覆盖了 `/v1/files/retrieve` 的下载需求(`video/retrive.md:90-93`,Minimax download_url 有效期 1 小时,必须及时下载入库)

### 5.2 5 个变体的请求路由

5 个生成 API 都打到 `POST /v1/video_generation`,只是 `model` 字段和必填字段不同:

| 变体 | 区分字段 | 可用模型 | 文档 |
|---|---|---|---|
| T2V | `prompt` | Hailuo-2.3, Hailuo-02, T2V-01-Director, T2V-01 | `video/t2v.md:64-105` |
| I2V | `first_frame_image` | Hailuo-2.3, 2.3-Fast, Hailuo-02, I2V-01-Director, I2V-01-live, I2V-01 | `video/i2v.md` |
| Frame2V | `first_frame_image` + `last_frame_image` | Hailuo-02 | `video/frame2v.md` |
| Subject Ref | `subject_reference` | S2V-01 | `video/refvideo.md` |

`VideoContentItem` enum(`crates/agent-runtime-aigc-providers/src/types/video.rs:42-60`)已经覆盖全部 4 个变体,**不需要新增 variant**:

```rust
pub enum VideoContentItem {
    Text { text: String },                                  // T2V prompt
    Image { asset: AssetRef, role: VideoImageRole },        // I2V / Frame2V / Subject Ref
    Video { asset: AssetRef },                              // 已有,Minimax 不用
    Audio { asset: AssetRef },                              // 已有,Minimax 不用
    DraftTask { id: String },                               // 已有,Minimax 不用
}
```

`VideoImageRole`(`types/video.rs:62-67`)的 3 个 variant 已对齐 Minimax 用法:
- `FirstFrame` → I2V 的 `first_frame_image` / Frame2V 的 `first_frame_image`
- `LastFrame` → Frame2V 的 `last_frame_image`
- `ReferenceImage` → Subject Ref 的 `subject_reference`

换句话说,`VideoProvider` 实现只负责把 `Vec<VideoContentItem>` 按 `role` 解构,挑出对应字段填到 Minimax 请求体里,**types 层零改动**。

**Subject Reference 的细节**(`video/refvideo.md`): Minimax 的 `subject_reference` 字段除了图片还可能带 `mask` / 标签等附加元数据,需要确认 `AssetRef` 是否够用。若不够,优先扩 `AssetRef` 而不是给 `VideoContentItem` 新加 variant。

### 5.3 通用参数(`video/t2v.md:64-105`)

| 字段 | 类型 | 锚点 | 现有 `VideoGenerationConfig` (`types/video.rs:69-96`) 是否覆盖 |
|---|---|---|---|
| `model` | enum | `video/t2v.md:64` | ✅ `VideoGenerationRequest.model` |
| `prompt` | string ≤2000 | `video/t2v.md:74` | ✅ `VideoContentItem::Text { text }` |
| `duration` | 6 或 10 秒 | `video/t2v.md` | ✅ `duration_secs: Option<i32>` |
| `resolution` | 512P/720P/768P/1080P | `video/t2v.md` | ✅ `resolution: Option<String>` |
| `aigc_watermark` | bool | `video/t2v.md` | ✅ `watermark: bool` |
| `prompt_optimizer` | bool,默认 true | `video/t2v.md:77` | ❌ Minimax 专属,放 `provider_options` JSON(避免污染共享类型) |
| `fast_pretreatment` | bool,仅 Hailuo-2.3/2.3-Fast/02 | `video/t2v.md:80` | ❌ 同上,`provider_options` |
| `callback_url` | string | `video/t2v.md:107` | ❌ webhook 路径,见 §5.5(v0.10 不实现) |

**图片输入约束**(`video/t2v.md` 同段 / `video/i2v.md`): JPG/JPEG/PNG/WebP,≤20MB,短边 >300px,长宽比 2:5 ~ 5:2,传 URL 或 `data:image/jpeg;base64,...`。`AssetRef`(`types/common.rs:13-21`)的 `Url` 与 `DataUrl` variant 已覆盖,**无需扩展**。

**运镜指令**(15 种): `[左移] [右移] [推进] [拉远] [上升] [下降] [上摇] [下摇] [左摇] [右摇] [变焦推近] [变焦拉远] [晃动] [跟随] [固定]`,组合 ≤3 个,直接嵌入 `prompt` 字符串(`video/t2v.md` prompt 字段段)。

### 5.4 状态查询(`video/status.md`)

**端点**: `GET /v1/query/video_generation?task_id=xxx`(`video/status.md:13,26,33`)

**响应**(`video/status.md:51-68`):

| 字段 | 类型 | 终态 |
|---|---|---|
| `task_id` | string | — |
| `status` | `Preparing\|Queueing\|Processing\|Success\|Fail` | `Success` / `Fail` |
| `file_id` | string | 仅 `Success` 时返回 |
| `video_width` / `video_height` | int | 仅 `Success` |

映射到 `ProviderGenerationStatus`(`types/common.rs:110-119`,5 个 variant: `Queued` / `Running` / `Completed` / `Failed` / `TimedOut`):
- `Preparing` / `Queueing` → `Queued`
- `Processing` → `Running`
- `Success` → `Completed`(with `file_id` → 下一步用 `/v1/files/retrieve` 拉 `download_url`)
- `Fail` → `Failed`
- (Minimax 没有显式 timeout 状态;`TimedOut` 由 gateway 的 `wait_for_completion` 在轮询超过预算时本地产生)

### 5.5 callback_url webhook(可选,v0.10 暂不实现)

`video/t2v.md:107-109`(以及 i2v/frame2v/refvideo 同段):

1. **验证阶段**: Minimax POST `{ challenge: "..." }` 到 `callback_url`,服务端必须 3 秒内原样回 challenge
2. **推送阶段**: 任务状态变更(processing / success / failed)时 POST 最新状态,结构与 `GET /v1/query/video_generation` 一致

**决策**: v0.10 只实现轮询。Webhook 不是技术问题(SDK 调用方完全可能在自己服务里嵌 SDK 并暴露回调端点),而是优先级问题——轮询已覆盖 99% 用例,callback 仅在"成本极敏感、不想 5s 一次轮询"的场景下需要。v0.11+ 看需求再补。

### 5.6 验证清单

- [ ] T2V: `VideoGateway::generate(prompt="...", model="MiniMax-Hailuo-02")` → 拿到本地 asset URL
- [ ] I2V: 同上 + `first_frame_image: AssetRef::Url(...)`
- [ ] Frame2V: 同上 + first/last frame
- [ ] Subject Ref: `VideoContentItem::Image { role: VideoImageRole::ReferenceImage, .. }` + S2V-01
- [ ] 轮询: `Preparing → Queueing → Processing → Success` 状态序列正确,失败时 `Fail` 错误透传
- [ ] 下载: gateway 在 download_url 1 小时过期前完成持久化

---

## 六、实现优先级与依赖链

按价值/风险/依赖排序。各 Phase 不共享代码,**完全并行**:

| Phase | 对应章节 | 范围 | 依赖 |
|---|---|---|---|
| 1 | §二 | LLM(Minimax adapter + `ContentBlock::Image/Video/MidConvSystem` + `Role` 扩展 + `service_tier`) | 无 |
| 2 | §五 | Video(`VideoProvider` 实现 + `VideoGenerationConfig` 不动 + `prompt_optimizer/fast_pretreatment` 走 `provider_options`) | 无 |
| 3 | §3.2 + §3.3 | TTS 同步 WSS + 异步(`MinimaxTtsAdapter` + `TtsOperation::Async` variant) | 无 |
| 4 | §3.4 + §3.5 | 文件上传 + Voice Management(`VoiceManager` trait + 3 个请求类型) | 与 Phase 3 共享同一 `minimax` feature flag,但代码模块互不依赖,可并行 |
| 5 | §四 | Music(新 crate `agent-runtime-music-providers` + `MusicProvider` trait) | 无 |

```
Phase 1: LLM
└── crates/agent-runtime-providers/src/providers/minimax/
    + crates/agent-runtime-model/src/types.rs(ContentBlock + Role 扩展)

Phase 2: Video
└── crates/agent-runtime-aigc-providers/src/providers/minimax.rs

Phase 3: TTS sync + async
├── crates/agent-runtime-tts-providers/src/providers/minimax/{mod,protocol,sync,async}.rs
└── crates/agent-runtime-tts-providers/src/types.rs(+ TtsOperation::Async)

Phase 4: Files + Voice Management
├── crates/agent-runtime-tts-providers/src/providers/minimax/{files,voice}.rs
├── crates/agent-runtime-tts-providers/src/traits.rs(+ VoiceManager)
└── crates/agent-runtime-tts-providers/src/types.rs(+ CloneVoiceRequest 等)

Phase 5: Music
└── crates/agent-runtime-music-providers/(新 crate)
```

**风险点**: Phase 1 给 `agent-runtime-model::ContentBlock` 加 variant 会让所有 LLM provider 必须处理新 variant(`unimplemented!()` 或 `OptionAdjustment`)——这是 SDK 级别的破坏性改动,要先在 §七 Q1 决策清楚再动手。

---

## 七、待决策的开放问题

### Q1: `agent-runtime-model::ContentBlock` 扩展是否影响其他 provider?

`ContentBlock::Image` / `Video` / `MidConvSystem` 加进 `agent-runtime-model/src/types.rs:25-41` 后,**所有现有 LLM provider**(`anthropic` / `openai` / `deepseek` / `openrouter` / `volcengine`)都必须处理这些新 variant——Rust enum 没有"忽略未知 variant"的语法,match 必须穷尽。

- **选项 A**(推荐): 其他 provider 在 match arm 里返回 `OptionAdjustment` + 丢弃该 block,与现有 `thinking_budget_tokens` unsupported 路径一致(`crates/agent-runtime-providers/src/providers/openai.rs:243-251`)。**`Image` 例外**: Anthropic 原生支持,顺手实现;OpenAI 也支持 vision,留作 v0.11 follow-up
- **选项 B**: types.rs 加 feature flag,Minimax-only types 用 `#[cfg(feature = "minimax_extensions")]` 隔离 — 增加复杂度,不推荐

### Q2: Voice Management 的 trait 归属

- **选项 A**(推荐): `VoiceManager` 独立 trait,与 `TtsProvider` 同文件 — 职责清晰,只 Minimax 实现
- **选项 B**: 把 3 个方法加进 `TtsProvider` 并给 default impl `Err(Unsupported)` — 污染所有 provider
- **选项 C**: 完全独立 crate — 过度工程,与 TTS 强耦合(voice_id 复用)

### Q3: Music 是否独立 crate

- **选项 A**(推荐): 独立 `agent-runtime-music-providers` — 与 TTS/AIGC 参数体系差异大,未来易加 Suno 等
- **选项 B**: 并入 `agent-runtime-aigc-providers` — 污染 ImageProvider/VideoProvider trait

### Q4: callback_url webhook 是否在 v0.10 实现

**决策**: **不实现**(理由见 §5.5)。v0.11+ 看需求再补。

---

## 八、缺失的供应商文档(需补)

1. **音色列表查询 API**: Minimax TTS/Voice 文档中多次引用 `/api-reference/voice-management-get` 与 `/faq/system-voice-id`(`tts_sync.md:68` 等),但 `docs/external/minimax/` 中没有对应文件。
   - 影响: `MinimaxTtsAdapter::list_voices` 需要这个端点才能返回系统音色清单;§3.5 Voice Clone 试听时 `model` 参数也需要知道可用模型 ID
   - 行动: 从 https://platform.minimaxi.com/docs 补 `voice_list.md`

2. **`voice_manage.md` 是错误文件**: `docs/external/minimax/voice_manage.md` 与 `voice_clone/clone.md` 内容**完全相同**(已用 `diff` 验证),都是 `POST /v1/voice_clone`。文件名 "voice_manage" 误导,它不是音色管理 API。
   - 行动: 删除 `voice_manage.md`,补上真正的 voice list 文档

3. **TTS Async 状态查询**: `tts_async.md` 全文只有 `POST /v1/t2a_async_v2`(`tts_async.md:53-54`),没有独立的 task 状态查询端点。文件就绪通过 `/v1/files/retrieve` 隐式探测。
   - 行动: 向 Minimax 确认是否真的只有这种方式,或者补查询端点文档

4. **计费/限流文档**: 各 API 返回 `usage_characters` / `task_token`(JWT 包含配额信息?)但 Minimax 计费规则文档未收录。
   - 影响: `polaris/observability.md` 的 token 计量需要这些字段映射
   - 行动: 补 `pricing.md` 与限流规则文档,确认 RPM / TPM 上限

---

## 九、待确认:Her(角色扮演 / 陪伴模型)

**现状**: `docs/external/minimax/` **完全没有** Her 模型相关文档。

核查结论(全文 grep):
- 出现的 LLM 模型只有 `MiniMax-M2 / M2.1 / M2.5 / M2.7 / M3`(及 `-highspeed` 变体)
- 没有 `her` / `role.?play` / `角色扮演` / `陪伴` / `companion` / `persona` 任何关键词
- `llm.md` 只覆盖 `/anthropic/v1/messages` 一个端点

**可能的解释**(本地不可证):
1. Her 走 Minimax 自有 chat 协议(非 Anthropic 兼容路径),如 `/v1/text/chatcompletion_v2` 系列,文档未拉取
2. Her 在 Minimax 海外站(MiniMax Audio / Talkie)产品里,不出现在 `platform.minimaxi.com/docs` 的 API 索引中
3. `llm.md:2` 写明 "Fetch the complete documentation index at https://platform.minimaxi.com/docs/llms.txt",**本地是子集**,Her 可能在未拉取的页面里

**对接入设计的影响**:
- 若 Her 走 Anthropic 兼容路径,**§二 的 `MinimaxAdapter` 已覆盖**,只需在 `catalog.rs` 加模型条目
- 若 Her 走自有 chat 协议,需要在 §二 新增第二个 adapter(`MinimaxLegacyAdapter`),与现有 `MinimaxAdapter`(Anthropic 兼容)并存
- 若 Her 有**专属字段**(如 `character_setting` / `persona_id` / 多模态人物形象),需评估是否扩 `RequestOptions` / `Message` / `ContentBlock`

**行动**:
1. 从 `https://platform.minimaxi.com/docs/llms.txt` 抓完整索引,确认 Her 是否独立 API
2. 若是,补 `docs/external/minimax/her.md`(或 `chat_legacy.md`),然后回头扩 §二
3. 在确认之前,**§六 Phase 1 范围不包含 Her** — 不要在没看到 schema 的情况下凭印象设计字段
