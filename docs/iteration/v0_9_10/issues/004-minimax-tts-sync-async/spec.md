# 004 · Minimax TTS 同步 + 异步 + 文件上传

## 背景

Minimax TTS 两条路径:同步 WSS(`wss://api.minimaxi.com/ws/v1/t2a_v2`)与异步 HTTP
(`POST /v1/t2a_async_v2`)。落到现有 `TtsProvider` trait
(`crates/agent-runtime-tts-providers/src/traits.rs`,7 方法已核对)。文件上传
(`POST /v1/files/upload`)是异步 TTS 长文本 + Voice Clone(005)的内部前置 helper,不暴露给 trait。

依赖确认:
- `TtsErrorCode`(`error.rs`)16 variant 已核对,含 `InvalidRequest`,**无** `RateLimited`/`InvalidInput`。
- `TtsError.upstream_code` 字段存在,用于保留 Minimax 原始状态码。
- tts crate `Cargo.toml` **当前无 `reqwest`** —— 本 issue 需新增(异步 + 上传走 HTTP)。

设计来源:[`minimax-api-analysis.md`](../../../../research/minimax-api-analysis.md) §3.1-3.4。
模板:同步 WSS → `providers/volcengine/{bidirectional,unidirectional}`;单文件 HTTP → `providers/aliyun`。

## 4a. Cargo feature + 依赖

```toml
[features]
minimax = ["dep:futures-util", "dep:tokio-tungstenite", "dep:base64", "dep:hex", "dep:reqwest"]

[dependencies]
reqwest = { version = "0.12", features = ["json", "multipart"], optional = true }
hex = { version = "0.4", optional = true }
```

`tokio-tungstenite` / `futures-util` / `base64` 已是其他 provider 的 optional dep,feature 复用。

## 4b. 模块布局

```
crates/agent-runtime-tts-providers/src/providers/minimax/
├── mod.rs        # MinimaxTtsAdapter + impl TtsProvider(005 再加 impl VoiceManager)
├── protocol.rs   # task_start/continue/finish JSON 帧 + hex 音频解码 + base_resp 错误码映射
├── sync.rs       # WSS 客户端
├── async.rs      # POST /v1/t2a_async_v2
└── files.rs      # POST /v1/files/upload(internal helper)+ /v1/files/retrieve 薄拷贝
```

`providers/mod.rs` 加 `#[cfg(feature = "minimax")] pub mod minimax;`。

## 4c. 同步 WSS(§3.2)

帧序列:`task_start` → `task_started` → `task_continue` → `task_continued`(可多次)→
`task_finish` → `task_finished`。关键事实:全程 JSON 文本帧;音频在 `data.audio` 以 **hex 字符串**
编码;`data` 可能为 `null` 要判空;`is_final` 标志最终块。鉴权:WSS 握手 `Authorization: Bearer`。

trait 方法映射:
- `synthesize`:start → 单 continue → finish,聚合所有 `data.audio` hex 解码 → `AudioData::Bytes`。
- `stream_synthesize`:每帧立刻 push `TtsStreamEvent::Audio`。
- `start_duplex_stream`:输入 channel 映射到 `task_continue` 序列。

**base_resp 错误码映射(§3.2 表 → 已核对的 `TtsErrorCode`)**:

| Minimax | → TtsErrorCode |
|---|---|
| 0 | 正常 |
| 1001 / 2201 | `Timeout` |
| 1002 / 1039 / 2205(限流) | `ProviderHttpError`(`status=429`,`upstream_code` 留原码) |
| 1004 | `InvalidApiKey` |
| 1042 / 2203 / 2204 / 2013 | `InvalidRequest` |
| 2202 | `ProviderStreamError` |
| 1000 / 其他 | `ProviderTaskFailed` |

> `TtsErrorCode` 不区分限流与一般 HTTP 错误,1002/1039/2205 复用 `ProviderHttpError` 但
> `upstream_code` 保留原码;未来需 retry 策略再补 `RateLimited`(设计文档 §3.2)。

## 4d. 异步(§3.3)

- `SynthesizeRequest` 的 `TtsOperation`(`types.rs:329`)加 `Async` variant。
- `MinimaxTtsAdapter::synthesize` 按 `request.operation` 分派 sync WSS 或 async HTTP。
- async POST `/v1/t2a_async_v2` 返回 `task_id`/`task_token`/`file_id` → `AudioData::Url`(是否预下载留给上层)。
- `text_file_id` 长文本路径调 4e 上传 helper。

## 4e. 文件上传(§3.4)

`files.rs`:`upload_file(client, api_key, purpose, bytes, filename, mime) -> Result<u64, TtsError>`
(multipart),`FilePurpose { VoiceClone, PromptAudio, T2aAsyncInput }`。`/v1/files/retrieve`
薄拷贝(GET + JSON + `download_url` 提取,设计文档 §3.3 决策 A,tts crate 内复制)。
错误用 `TtsErrorCode::InvalidRequest`(非 `InvalidInput`)。

## 验收标准

- [ ] Cargo `minimax` feature + optional `reqwest`(json+multipart)/`hex` 加好,`--no-default-features --features minimax` 可编译
- [ ] `providers/minimax/{mod,protocol,sync,async,files}.rs` 存在,`providers/mod.rs` feature-gated 声明
- [ ] `TtsOperation::Async` variant 存在
- [ ] protocol.rs 16 个 base_resp 状态码按 4c 表映射,单元测试覆盖(尤其 1002→429+upstream_code、1004→InvalidApiKey)
- [ ] hex 音频解码:`data.audio` hex → bytes,`data=null` 判空不 panic(单元测试)
- [ ] `synthesize`(sync 路径)聚合帧返回非空 `AudioData::Bytes`(用 fake WSS / 帧夹具测试)
- [ ] `stream_synthesize` 至少产生 1 个 `task_continued` + 1 个 `task_finished` 对应事件
- [ ] `synthesize` with `TtsOperation::Async` 返回 `AudioData::Url`
- [ ] `upload_file` 构造 multipart(`purpose` + `file`),`FilePurpose` 3 值映射正确
- [ ] `cargo test -p agent-runtime-tts-providers --features minimax` 全绿;`clippy --features minimax -- -D warnings` 无 warning

> Live(手动,记录验证报告):sync `synthesize("你好")` 长度匹配 `extra_info.audio_size`;
> async 返回 URL(设计文档 §3.6)。
