# Minimax API 接入分析

> 基于 `docs/external/minimax/` 下的 API 文档，分析接入 Orchest 所需的前置条件、缺口和实现策略。
> 日期: 2026-06-14 | 状态: 分析阶段，未动手

---

## 一、API 矩阵总览

### 1.1 全部 16 个 API

| # | 领域 | 端点 | 方法 | Orchest 已有同类? |
|---|---|---|---|---|
| 1 | **LLM Chat** | `/anthropic/v1/messages` | POST + SSE | ✅ Anthropic provider (同协议) |
| 2 | **TTS Sync** | `/ws/v1/t2a_v2` | WSS | ❌ 现有 TTS 只用 HTTP |
| 3 | **TTS Async** | `/v1/t2a_async_v2` | POST | ✅ 类似 Volcengine 模式 |
| 4 | **Voice Clone** | `/v1/voice_clone` | POST | ❌ 全新领域 |
| 5 | **Voice Upload** | `/v1/files/upload` | POST (multipart) | ❌ 文件上传基础设施缺失 |
| 6 | **Voice Design** | `/v1/voice_design` | POST | ❌ 全新领域 (prompt→音色) |
| 7 | **Delete Voice** | `/v1/delete_voice` | POST | ❌ |
| 8 | **Music Generation** | `/v1/music_generation` | POST (+SSE stream) | ❌ 全新领域 |
| 9 | **Lyrics Generation** | `/v1/lyrics_generation` | POST | ❌ 全新领域 |
| 10 | **Music Cover Preprocess** | `/v1/music_cover_preprocess` | POST | ❌ 全新领域 |
| 11 | **Video T2V** | `/v1/video_generation` | POST | ✅ AIGC crate (OpenRouter 等) |
| 12 | **Video I2V** | `/v1/video_generation` | POST | ✅ |
| 13 | **Video Frame2V** | `/v1/video_generation` | POST | ✅ |
| 14 | **Video Subject Ref** | `/v1/video_generation` | POST | ✅ |
| 15 | **Video Query** | `/v1/query/video_generation` | GET | ❌ 任务轮询模式待建 |
| 16 | **File Retrieve** | `/v1/files/retrieve` | GET | ❌ 通用文件下载待建 |


> **⚠️ 缺失文档**: Minimax 的 TTS/Voice Clone/Design 文档中多次引用 `/api-reference/voice-management-get`（查询可用音色列表）和 `/faq/system-voice-id`，但 `docs/external/minimax/` 中没有对应文件。此外 `voice_manage.md` 实际内容是 Voice Clone 的重复（与 `voice_clone/clone.md` 完全相同），并不是音色管理/查询 API。Voice List 是 TTS 和 Voice 工作流的前置能力，需要从 Minimax 平台补充该文档。
---

## 二、前置基础设施缺口

以下是多个 API 共同依赖、但目前 Orchest 完全缺失的能力。

### 2.1 文件上传 (`multipart/form-data`)

**影响范围**: Voice Clone 音频上传、TTS Async 的 `text_file_id`、Video 的 Files API 引用 (`mm_file://`)

**当前状态**: Orchest 无任何 `multipart/form-data` 支持

**需要**:
- HTTP client 层的 `multipart/form-data` 构造能力
- MIME 类型检测
- 文件大小/格式校验（不同 API 有不同约束）

### 2.2 异步任务轮询抽象

**影响范围**: 视频生成 (task 提交 → 轮询 → 完成 → 下载)、TTS Async

**当前状态**: 各 provider 各自实现，无通用 trait

**需要**:
```rust
// 建议的 trait 形态
pub(crate) trait AsyncTask {
    type Request;
    type Status;
    type Output;
    async fn submit(&self, req: Self::Request) -> Result<TaskHandle>;
    async fn poll(&self, task_id: &str) -> Result<Self::Status>;
    async fn wait(&self, task_id: &str, poll_interval: Duration) -> Result<Self::Output>;
}
```

> **注意**: TTS Async 与 Video 的轮询路径不同。Video 有独立的 `GET /v1/query/video_generation`，而 TTS Async 响应直接返回 `file_id`，任务完成后通过 `/v1/files/retrieve` 下载 — 轮询方式待确认。

**视频任务状态机**: `Preparing → Queueing → Processing → Success/Fail`

### 2.3 WebSocket 客户端

**影响范围**: TTS 同步 (WSS `/ws/v1/t2a_v2`)

**当前状态**: 完全无 WebSocket transport

**TTS 同步的 WS 协议**:
- 多阶段协议: `task_start` → 服务端 `task_started` → 逐条 `task_continue` → `task_finish`
- 响应是二进制音频帧经 WS 推送
- 需要 tokio-tungstenite 或类似依赖

**决策点**: 是否值得引入 WebSocket，还是只实现异步 HTTP 版本

### 2.4 通用文件下载

**影响范围**: 视频下载、TTS Async 下载

**当前状态**: 无

**关键约束**:
- Video 的 `download_url` 有效期 1 小时
- TTS Async 的下载 URL 有效期 9 小时
- 视频文件可能很大，需要 streaming download

---

## 三、模块化缺口分析

### 3.1 LLM — 相对简单

Minimax LLM 使用 **Anthropic Messages API 兼容格式**。

**可直接复用**:
- `CreateMessageReq` / `CreateMessageResp` 类型结构几乎一致
- SSE streaming 格式 (`message_start` / `content_block_delta` / `message_stop`) 完全相同
- `Tool` / `ToolChoice` 格式一致
- `Usage` 结构兼容

**Minimax 独有差异**:

| 差异点 | 说明 | Anthropic 标准有? |
|---|---|---|
| 多模态 `image` / `video` block | MiniMax-M3 支持图片+视频理解 | ✅ 有 image (但 video 是 Minimax 专属) |
| `thinking` 控制 | `{ type: "adaptive" \| "disabled" }` | ✅ 有 extended thinking (参数形态不同) |
| `mid_conv_system` block | 对话中途插入 system prompt | ❌ |
| `user_system` / `group` / `sample_message_*` role | 非标准角色 | ❌ |
| `service_tier` | `standard` / `priority` | ❌ |
| `MediaSource.detail` | `low` / `default` / `high` | ❌ |
| `MediaSource.fps` | 视频抽帧频率 [0.2, 5] | ❌ |
| `MediaSource.max_long_side_pixel` | 最长边约束 | ❌ |
| Auth 双模式 | `Authorization: Bearer` + `x-api-key` | ✅ Bearer |

**结论**: 大部分工作是在现有 Anthropic provider 上做 Minimax 差异适配，核心挑战是多模态 + thinking 参数映射。

### 3.2 TTS — 中等

#### 同步 TTS (WSS)

```
客户端                             服务端
  │                                  │
  │── { event: "task_start",         │
  │      model, voice_setting,       │
  │      audio_setting, ... } ──────▶│
  │                                  │
  │◀── { event: "task_started" } ────│
  │                                  │
  │── { event: "task_continue",      │
  │      text: "第一句..." } ───────▶│
  │◀── [binary audio chunk] ─────────│
  │                                  │
  │── { event: "task_continue",      │
  │      text: "第二句..." } ───────▶│
  │◀── [binary audio chunk] ─────────│
  │                                  │
  │── { event: "task_finish" } ─────▶│
  │◀── [final audio chunk] ──────────│
```

**音色参数的复杂程度**:

```
voice_setting:
  voice_id:          string          # 音色 ID 或混合音色时为空
  speed:             float [0.5,2]   # 语速
  vol:               float (0,10]    # 音量
  pitch:             int [-12,12]    # 语调
  emotion:           "happy"|"sad"|... # 情绪 (9种)
  english_normalization: bool        # 英文规范化
  latex_read:        bool            # LaTeX 公式朗读

timbre_weights:                     # 混合音色 (最多4种)
  - voice_id: string
    weight: int [1,100]

audio_setting:
  sample_rate:  8000|16000|22050|24000|32000|44100
  bitrate:      32000|64000|128000|256000
  format:       mp3|pcm|flac|wav|pcmu_raw|pcmu_wav|opus
  channel:      1|2

pronunciation_dict:
  tone: ["燕少飞/(yan4)(shao3)(fei1)", "omg/oh my god"]

language_boost: Chinese|English|Japanese|...|auto  (41种语言)

voice_modify:                       # 声音效果器
  pitch:        int [-100,100]      # 音高
  intensity:    int [-100,100]      # 强度
  timbre:       int [-100,100]      # 音色
  sound_effects: string             # 音效
```

#### 异步 TTS

```
POST /v1/t2a_async_v2
  → { task_id, file_id, task_token, usage_characters }
  → 通过 task_id 轮询或用 file_id 下载

输入: text (≤5万字符) 或 text_file_id (txt/zip, ≤100万字符)
```

**与现有 TTS provider 的关系**: 需要确认 `agent-runtime-tts-providers/src/traits.rs` 的 trait 签名能否容纳 Minimax 的参数丰富度。

### 3.3 Voice Management — 高复杂度

4 个 API 构成完整音色生命周期:

```
文件上传 ──▶ 音色复刻 ──▶ 音色可用 ──▶ 删除音色
                │
音色设计 ──────┘
```

#### 3.3.1 Voice Upload

```
POST /v1/files/upload
Content-Type: multipart/form-data

purpose: voice_clone
file:    <binary audio>

约束: mp3/m4a/wav, 10s-5min, ≤20MB

响应: { file: { file_id, bytes, filename, purpose } }
```

#### 3.3.2 Voice Clone

```
POST /v1/voice_clone

请求:
  file_id:              int64   # 上传后获得的文件 ID
  voice_id:             string  # 自定义 (8-256字符, 字母开头)
  clone_prompt:                  # 可选, 提高相似度
    prompt_audio:       int64   # 示例音频 file_id
    prompt_text:        string  # 示例音频对应文本
  text:                 string  # 可选, 试听文本 (≤1000字符)
  model:                string  # 试听合成模型 (text + model 都存在时才合成试听)
  language_boost:       string  # 可选
  need_noise_reduction:  bool   # 降噪, 默认 false
  need_volume_normalization: bool # 音量归一化
  aigc_watermark:       bool

响应:
  input_sensitive: { type: 0-7 }  # 风控结果
  demo_audio:         string      # 试听链接 (如果请求了 text + model)
  extra_info:                      # 试听的元信息和计费
```

**特殊规则**: 音色 7 天未使用会被系统清理。

#### 3.3.3 Voice Design (文本→音色)

```
POST /v1/voice_design

请求:
  prompt:         string  # 音色描述 e.g. "讲述悬疑故事的播音员，声音低沉富有磁性"
  preview_text:   string  # 试听文本 (≤500字符)
  voice_id:       string  # 可选, 不传则自动生成

响应:
  voice_id:    string  # ttv-voice-2025060717322425-xxxxxxxx
  trial_audio: string  # hex 编码音频
```

#### 3.3.4 Delete Voice

```
POST /v1/delete_voice

请求:
  voice_type: "voice_cloning" | "voice_generation"
  voice_id:   string

只删除 clone 和 design 产生的音色，系统音色不能删。
```

**当前 Orchest 缺口**: `tts-providers` 有 `voices.rs` (2.5KB) 但只是音色列表查询，不是 CRUD 管理。Voice Management 是一整套新能力。

### 3.4 Music — 全新领域

3 个 API 逻辑耦合:

```
歌词生成 ──────────────┐
                       ├──▶ 音乐生成 ──▶ 音频
翻唱预处理 ────────────┘
```

#### 3.4.1 Lyrics Generation

```
POST /v1/lyrics_generation

mode:   "write_full_song" | "edit"
prompt: string      # 歌曲主题/风格 (≤2000字符)
lyrics: string      # edit 模式下的已有歌词 (≤3500字符)
title:  string      # 可选, 指定标题

响应:
  song_title: string
  style_tags: "Mandopop, Summer Vibe, Romance, ..."
  lyrics:     string  # 含结构标签 [Intro][Verse][Chorus]...
```

支持 14 种结构标签: `[Intro]`, `[Verse]`, `[Pre-Chorus]`, `[Chorus]`, `[Hook]`, `[Drop]`, `[Bridge]`, `[Solo]`, `[Build-up]`, `[Instrumental]`, `[Breakdown]`, `[Break]`, `[Interlude]`, `[Outro]`

#### 3.4.2 Music Cover Preprocess

```
POST /v1/music_cover_preprocess

model:         "music-cover"
audio_url:     string     # 或 audio_base64, 二选一

约束: 6s-6min, ≤50MB, mp3/wav/flac 等

响应:
  cover_feature_id:  string  # 24 小时有效, 同音频去重
  formatted_lyrics:  string  # ASR 提取的歌词
  structure_result:  JSON    # {"num_segments":4, "segments":[...]}
  audio_duration:    float
```

#### 3.4.3 Music Generation (核心)

```
POST /v1/music_generation

模型: music-2.6 | music-cover | music-2.6-free | music-cover-free

通用参数:
  model:            string
  prompt:           string   # 风格描述
  lyrics:           string   # \n 分隔, 支持结构标签
  stream:           bool     # 流式返回 hex chunk
  output_format:    "url" | "hex"
  audio_setting:    { sample_rate, bitrate, format: mp3|wav|pcm }
  aigc_watermark:   bool

music-2.6 专属:
  lyrics_optimizer: bool     # prompt 自动生成歌词
  is_instrumental:  bool     # 纯音乐

music-cover 专属:
  audio_url:        string   # 参考音频 (6s-6min, ≤50MB)
  audio_base64:     string   # 或 Base64
  cover_feature_id: string   # 两步翻唱 (与 audio_url/base64 互斥)
```

### 3.5 Video — 中等

5 个生成变体共享 `POST /v1/video_generation`:

| 变体 | 区分字段 | 可用模型 |
|---|---|---|
| **T2V** | `prompt` | Hailuo-2.3, Hailuo-02, T2V-01-Director, T2V-01 |
| **I2V** | `first_frame_image` | Hailuo-2.3, 2.3-Fast, Hailuo-02, I2V-01-Director, I2V-01-live, I2V-01 |
| **Frame2V** | `first_frame_image` + `last_frame_image` | Hailuo-02 |
| **Subject Ref** | `subject_reference` | S2V-01 |

#### 通用参数

```
POST /v1/video_generation

model:              枚举见上表
prompt:             string (≤2000字符)  # T2V/I2V/Frame2V 需要, Subject Ref 可选
prompt_optimizer:   bool   # 自动优化 prompt, 默认 true
fast_pretreatment:  bool   # 缩短优化耗时 (仅 Hailuo-2.3/2.3-Fast/02)
duration:           6 | 10 # 秒
resolution:         512P | 720P | 768P | 1080P  # 依模型而定
callback_url:       string # 状态推送回调
aigc_watermark:     bool

响应:
  task_id: string   # 用于后续查询

图片输入 (first_frame_image / last_frame_image):
  格式: JPG, JPEG, PNG, WebP
  大小: ≤20MB
  尺寸: 短边 >300px, 长宽比 2:5 ~ 5:2
  传参: URL 或 data:image/jpeg;base64,...

运镜指令 (15种):
  [左移] [右移] [左摇] [右摇] [推进] [拉远] [上升] [下降]
  [上摇] [下摇] [变焦推近] [变焦拉远] [晃动] [跟随] [固定]
  组合: [左摇,上升] (≤3个)
```

#### Task 状态机

```
Preparing → Queueing → Processing → Success → 下载
                                   → Fail
```

#### 状态查询

```
GET /v1/query/video_generation?task_id=xxx

响应:
  task_id:      string
  status:       "Preparing" | "Queueing" | "Processing" | "Success" | "Fail"
  file_id:      string     # Success 时返回
  video_width:  int
  video_height: int
```

#### 文件下载

```
GET /v1/files/retrieve?file_id=xxx

响应:
  file:
    file_id:      int64
    bytes:        int64
    filename:     "output_aigc.mp4"
    purpose:      "video_generation"
    download_url: string   # 有效期 1 小时
```

#### callback_url 异步通知

Video 生成支持通过 `callback_url` 接收任务状态变更推送，替代轮询：

1. **验证阶段**: Minimax 服务器向 `callback_url` 发送 `POST { challenge: "..." }` → 服务端必须在 3 秒内原样返回 challenge 值
2. **推送阶段**: 验证通过后，每当任务状态变更（`processing` / `success` / `failed`），Minimax 向 `callback_url` POST 最新任务状态（结构与 `GET /v1/query/video_generation` 响应一致）

这意味着 Video provider 既要实现 HTTP client 发起请求，也要能在 service 侧处理 incoming webhook — 或者只实现轮询模式，callback 作为可选增强。

**与现有代码的关系**: `agent-runtime-aigc-providers` 已有 Volcengine/OpenRouter/Renderful 的视频生成，模式类似。可以复用 AIGC gateway 和 types。

---

## 四、实现优先级与依赖链

```
Phase 1: 基础设施 (无依赖, 跨模块)
├── HTTP multipart/form-data 支持
├── 通用文件上传/下载 trait
└── 异步任务轮询 trait (AsyncTask)

Phase 2: LLM (最高价值/最低风险)
└── crates/agent-runtime-providers/src/minimax.rs
    复用 Anthropic Messages API 路径, 只做差异适配

Phase 3: TTS
├── 异步 TTS 的 text_file_id 路径依赖 Phase 1 (文件上传)
├── 同步 WSS 路径与异步 text 输入路径无依赖, 可先行
├── crates/agent-runtime-tts-providers/src/providers/minimax/
│   ├── websocket.rs   (依赖 tokio-tungstenite)
│   └── async.rs

Phase 4: Voice Management (依赖 Phase 1)
├── 新 crate: agent-runtime-voice-providers/ 或并入 TTS crate
│   ├── upload.rs
│   ├── clone.rs
│   ├── design.rs
│   └── delete.rs

Phase 5: Music (全新 crate)
└── crates/agent-runtime-music-providers/
    ├── generation.rs
    ├── lyrics.rs
    └── cover.rs

Phase 6: Video (扩展现有 AIGC crate)
└── crates/agent-runtime-aigc-providers/src/providers/minimax.rs
    ├── t2v.rs
    ├── i2v.rs
    ├── frame2v.rs
    ├── subject_ref.rs
    ├── query.rs
    └── download.rs
```

---

## 五、待决策的架构问题

以下是需要在动手前通过阅读现有代码确定的：

### Q1: 现有 traits 能否容纳 Minimax 的参数丰富度?

需要审查:
- `agent-runtime-tts-providers/src/traits.rs` — TTS trait 的泛型/参数设计
- `agent-runtime-aigc-providers/src/types.rs` — 图片/视频生成的请求类型
- `agent-runtime-providers/src/types.rs` — LLM provider types

### Q2: WebSocket transport 的取舍

TTS 同步的 WSS 是否需要支持？
- **做**: 需要引入 `tokio-tungstenite`，增加依赖和复杂度
- **不做**: 只实现异步 HTTP TTS，功能不完整，但降低首次交付成本

### Q3: Voice Management 的归属

- **方案 A**: 独立 `agent-runtime-voice-providers` crate — 职责清晰，但增加 crate 数量
- **方案 B**: 并入 `agent-runtime-tts-providers` — Voice 是 TTS 的前置依赖，逻辑上紧密

### Q4: Music crate 的层次

- **方案 A**: 并入 `agent-runtime-aigc-providers` — 音乐是 AIGC 的一种
- **方案 B**: 独立 `agent-runtime-music-providers` — 音乐 API 有自己的参数体系 (歌词、翻唱预处理)，与图片/视频差异大

### Q5: 文件上传下载的 infra 层归属

- 是所有 provider crate 共享的基础设施层 (`agent-runtime-core`)?
- 还是各 crate 自己实现 HTTP client 调用?
- 考虑到不同 API 的上传约束各异（Voice: mp3/m4a/wav, 10s-5min, ≤20MB; Video: JPG/PNG/WebP, ≤20MB），共享层的抽象粒度需要仔细设计

### Q6: `voice_manage.md` 是 Voice Clone 的重复文件, 真正的 Voice List API 文档缺失

`docs/external/minimax/voice_manage.md` 与 `voice_clone/clone.md` 内容完全相同 — 都是 `POST /v1/voice_clone`。文件名 "voice_manage" 有误导性, 它并不是音色管理 (list/query) API。

Minimax 的 TTS/Voice 文档中多次引用 `/api-reference/voice-management-get`（查询可用音色列表）, 但 `docs/external/minimax/` 中没有对应文件。Voice List 是 TTS 合成选音色的前置能力, 需要从 Minimax 平台补充该文档。

---

## 六、已有外部 provider doc 参考

Orchest 已有的 provider 实现可以作为代码模板:

| Provider | Crate | 文件 |
|---|---|---|
| Anthropic | agent-runtime-providers | `anthropic.rs` (55.7KB) |
| OpenAI | agent-runtime-providers | `openai.rs` (28.5KB) |
| Volcengine TTS | agent-runtime-tts-providers | `providers/volcengine/` |
| Volcengine AIGC | agent-runtime-aigc-providers | `providers/volcengine.rs` (7.7KB) |
| OpenRouter AIGC | agent-runtime-aigc-providers | `providers/openrouter.rs` (18.4KB) |

Minimax LLM 可以以 Anthropic provider 为起点（同协议），Minimax 视频可以以 OpenRouter AIGC 为起点（已有多模态）。
