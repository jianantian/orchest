# Hotfix 2026-08-06: Atomic Downstream APIs

## Background

Orchest 的 Rust provider traits 与 registry 已经能直接表达 Chat、ASR、TTS、Gen 等原子能力，
但 Python 与 TypeScript SDK 目前只公开 `Agent`。下游若只需一次 JSON completion、文件转写或
实时 ASR 中继，必须构造完整 agent loop，或绕过 Orchest 直接调用厂商 API。

Murmur 是首个暴露该缺口的真实下游：它需要 DeepSeek 单轮 JSON completion、移动端 m4a 短音频
同步转写，以及后续 WebSocket 音频中继。最新阿里云文档位于 `docs/external/aliyun/asr/`；当前实现
既没有 DashScope 同步 HTTP ASR，也没有 Python/TypeScript ASR 入口，流式阿里云方言还会把
`context` 错放进 `parameters`，同时固定发送空 `input`。

## Product Boundary

SDK 公开面分成两层，两者都属于 Orchest 的正式 SDK 能力：

1. **Agent layer**：`Agent` 负责 loop、tools、skills、budget、approval 与 runtime events。
2. **Atomic capability layer**：直接调用 provider capability，不启动 agent loop。本 hotfix 公开
   Chat completion、one-shot ASR 与 realtime ASR；TTS、Gen、Omni Realtime 暂不绑定，但后续沿用
   相同模式扩展。

Tool、MCP 与 Skill 的边界不变：这些原子 provider API 不是 Tool，MCP 也不参与其调用路径。

## Goals

1. 提供 provider-agnostic 的一次性 completion 路径，支持类型化 JSON object 输出与既有推荐重试。
2. 在 `orchest-provider-http` 实现阿里云 `qwen-audio-3.0-asr-flash` 同步 HTTP 方言，支持字节直传、
   m4a/aac、language hints、热词与初始上下文。
3. 修复阿里云 realtime ASR 的 request mapping，使 context 位于 `payload.input.context`，参数仍位于
   `payload.parameters`。
4. Python 与 TypeScript 同批、对称公开 `complete`、`transcribe`、`start_asr_stream` 与流 session。
5. 文档化 Python/Node 下游在 v1.0 发布前的本地构建与可重复安装流程。

## Locked API Shape

### Python

```python
from orchest import complete, transcribe, start_asr_stream

text = complete(
    model="deepseek/deepseek-chat",
    system="...",
    user="...",
    api_key_env="DEEPSEEK_API_KEY",
    json_mode=True,
    retry=True,
)

text = transcribe(
    audio_bytes,
    format="m4a",
    language="zh",
    provider="aliyun/qwen-audio-3.0-asr-flash",
    api_key_env="DASHSCOPE_API_KEY",
)

stream = start_asr_stream(
    format="aac",
    sample_rate=48_000,
    language="zh",
    provider="aliyun/qwen-audio-3.0-asr-flash-streaming",
    api_key_env="DASHSCOPE_API_KEY",
    context=[{"role": "user", "text": "Emile，Orchest，Murmur"}],
    on_event=handle_event,
)
stream.send_audio(chunk)
stream.finish()
stream.wait()
```

### TypeScript

```typescript
import { complete, transcribe, startAsrStream } from "@orchest/sdk";

const text = await complete({
  model: "deepseek/deepseek-chat",
  system: "...",
  user: "...",
  apiKeyEnv: "DEEPSEEK_API_KEY",
  jsonMode: true,
  retry: true,
});

const transcript = await transcribe(audioBytes, {
  format: "m4a",
  language: "zh",
  provider: "aliyun/qwen-audio-3.0-asr-flash",
  apiKeyEnv: "DASHSCOPE_API_KEY",
});

const stream = await startAsrStream(
  {
    format: "aac",
    sampleRate: 48_000,
    language: "zh",
    provider: "aliyun/qwen-audio-3.0-asr-flash-streaming",
    apiKeyEnv: "DASHSCOPE_API_KEY",
    context: [{ role: "user", text: "Emile，Orchest，Murmur" }],
  },
  handleEvent,
);
stream.sendAudio(chunk);
stream.finish();
await stream.wait();
```

Binding 命名遵循各语言惯例（Python snake_case、TypeScript camelCase），但默认值、状态转换、错误
条件与事件 wire shape 必须一致。

ASR context 在两种绑定都使用同一简化结构：
`{"role":"user"|"assistant","text":"..."}`。provider adapter 负责将 user 映射为
`input_text`、assistant 映射为 `text`；每种 role 最多 5 条，消息按轮次排序，单轮 user + assistant
文本合计不超过 400 字符。one-shot ASR 通过 `options.context` 接收该数组，realtime ASR 通过
`context` 参数接收。

## Architecture

```text
Python / TypeScript bindings
  ├─ complete ────────────────> orchest atomic completion helper
  │                                └─ ChatModel via orchest-provider wall
  ├─ transcribe ──────────────> Asr::transcribe
  │                                └─ orchest-provider-http / aliyun
  └─ start_asr_stream ────────> Asr::start_stream
                                   └─ orchest-provider-stream / aliyun
```

- `orchest-protocol` owns request/response enums and cross-capability contracts.
- `orchest` owns provider-neutral completion orchestration and reuses its retry policy.
- HTTP/WS wire details remain in their weight-tier provider crates.
- Bindings only resolve target-language arguments, instantiate through the wall, and bridge callbacks/handles.
- `orchest-py` 与 `orchest-node` enable `orchest-provider` 的 `asr` feature；调用方仍不命名 impl crate。

## Completion Semantics

- 每次调用构造至多一条 system message 与一条 user message，tools 为空，调用一次
  `ChatModel::complete()`；不创建 `AgentConfig`、run、budget 或 runtime event stream。
- 返回所有 `ContentBlock::Text` 按响应顺序拼接的字符串；Thinking 与其他 block 不混入返回值。
- `json_mode=true` 映射为类型化 `ResponseFormat::JsonObject`。Chat-compatible provider 发送
  `response_format:{"type":"json_object"}`；不支持该选项的方言必须在请求前报错，不静默降级。
- `retry=true` 使用 `RetryPolicy::recommended()`，仅重试 429、5xx、timeout 与 stream interruption；
  默认不重试。重试不得启动 agent loop。
- `MaxTokens`、content filter、refusal 等 stop reason 保留为错误而不是把不完整文本冒充成功。

## One-shot Aliyun ASR Semantics

- 默认模型：`aliyun/qwen-audio-3.0-asr-flash`；同 wire 的
  `aliyun/fun-asr-flash-2026-06-15` 可被显式选择，但不作为默认。
- 默认端点为仍可用的公共 DashScope
  `https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation`；
  `api_url` 可覆盖为业务空间专属北京/新加坡端点。
- 请求头固定 `Authorization: Bearer ...`、`Content-Type: application/json`、
  `X-DashScope-SSE: disable`。
- 音频字节编码为 Data URL。`m4a` 使用 `audio/mp4`，`aac` 使用 `audio/aac`；协议层新增
  `AudioFormat::M4a` 与 `AudioFormat::Aac`。
- `language` 映射为单元素 `language_hints`；`options` 中的 `sample_rate`、`vocabulary_id`、
  `vocabulary` 进入 `parameters`，`context` 进入音频消息之前的 `input.messages`。
- 解析 `output.text` 为 `TranscribeResult.text`；request id、usage、sentence detail 保留在
  `diagnostic_metadata`。缺失或非字符串 `output.text` 是协议错误。
- SDK 不自行转码、重采样或上传 OSS；格式、时长、大小等 provider 拒绝通过结构化错误上抛。

## Realtime ASR Session Contract

Session 状态为 `Open -> Finishing -> Closed`：

- `send_audio(bytes)` 仅在 Open 接受非空 chunk；底层 channel 已满时显式返回 backpressure 错误，
  不阻塞调用线程，也不静默丢 chunk。
- `finish()` 从 Open 转为 Finishing，关闭输入 channel，使阿里云方言发送一次 `finish-task`；重复
  `finish()` 幂等。Finishing/Closed 后 `send_audio` 报错。
- provider event 按接收顺序交给 `on_event`，使用 `StreamEvent` 的现有 serde wire shape；
  Provisional/Committed 与 `SegmentRef` 语义保持不变。
- `wait()` 等待 event stream 关闭并转为 Closed；重复调用安全。致命 provider error 同时作为
  `StreamEvent::Error` 交付，且让 `wait()` 返回错误。
- session drop 等价于 best-effort `finish()`，不得泄漏凭证、后台 task 或 WebSocket。
- 初始 context 位于 `run-task.payload.input.context`；`format`、`sample_rate`、`language_hints`、
  `vocabulary_id`、`vocabulary` 位于 `payload.parameters`。运行中 `continue-task` 动态 context 更新
  不在本 hotfix 范围。

## Error Handling

- provider/协议错误保留稳定 code/status/context，经 Python `ModelError`/通用 SDK 异常和 Node
  `Error` 边界转换；错误消息不得包含 API key 或完整 Base64 音频。
- API key 优先级为显式 `api_key`/`apiKey` > 指定 env > provider 默认 env；指定 env 缺失时不回退。
- 未知 model/provider、错误 format、空音频、非法 context、send-after-finish 均在本地明确失败。
- callback 自身抛错会终止 event pump；错误由 `wait()` 返回，不继续吞事件。

## Distribution and Documentation

- Python guide 增加 `uv` path/editable 接入、目标 Python 版本下的 `maturin develop`/wheel 重建，
  并解释原生扩展不能跨 CPython ABI 直接复用。
- TypeScript guide 增加本地 native build、`npm pack` 后从下游安装 tarball 的可重复流程。
- 本 hotfix 不发布 PyPI/npm，不新增发布凭证或 release automation。

## Non-goals

- TTS、GenTask、Omni Realtime 的 Python/TypeScript 绑定。
- realtime ASR 的动态 `continue-task` context 更新。
- 音频转码、采样率检测、时长探测、OSS 上传或异步长音频 filetrans。
- JSON schema structured output；本次只提供 `text` 与 `json_object`。
- live provider 调用。无凭证环境以逐字节 request fixture、response fixture 与 fake session 验证。

## Issue Breakdown

| Issue | Title | Depends on |
|---|---|---|
| 001 | Provider-neutral atomic completion + JSON mode | None |
| 002 | Aliyun HTTP ASR + realtime request/session repair | 001 only for shared protocol build order |
| 003 | Python/TypeScript atomic bindings + downstream guides | 001, 002 |

实施分支为 `hotfix/2026_08_06`。GitHub issues 在 spec review 后、写代码前创建；每个 issue 一个
`closes #N` commit。PRD/spec 文档先行独立 `docs:` commit。

## Success Metrics

- Python 与 TypeScript 的三组原子 API 均无需构造 `Agent`。
- m4a bytes 可形成正确的 DashScope HTTP request fixture 并解析 transcript fixture。
- realtime context/parameters 的 wire 位置由 fixture test 固定，session 无 silent audio/event drop。
- 两种绑定的 API、默认值、错误条件与类型声明一致。
- `cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check`、
  `bash scripts/lint-check.sh` 全部通过；Python package 与 Node addon 按项目约定完成构建验证。
