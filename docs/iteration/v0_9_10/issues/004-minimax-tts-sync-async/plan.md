# 004 · Minimax TTS 同步 + 异步 + 文件上传 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:executing-plans` /
> `superpowers:subagent-driven-development`。步骤用 checkbox 跟踪。

**Goal:** 在 tts crate 加 `minimax` feature 下的 `MinimaxTtsAdapter`,实现同步 WSS + 异步 HTTP
两条 TTS 路径 + 内部文件上传 helper。

**Architecture:** WSS 帧协议参考 volcengine,异步 HTTP 参考 aliyun;错误码集中在 `protocol.rs`。

**Tech Stack:** Rust, tokio-tungstenite, reqwest(json+multipart), serde_json, hex, base64。

---

## 要读的现有代码

- `crates/agent-runtime-tts-providers/src/traits.rs`(`TtsProvider` 7 方法)
- `crates/agent-runtime-tts-providers/src/types.rs`(`TtsOperation:329` / `SynthesizeRequest:246` / `SynthesizeResult:337` / `AudioData:292`)
- `crates/agent-runtime-tts-providers/src/error.rs`(`TtsErrorCode` 16 variant + `upstream_code`)
- `crates/agent-runtime-tts-providers/src/streaming.rs`(`TtsOutputStream` / `TtsDuplexStream` / `TtsStreamEvent`)
- `crates/agent-runtime-tts-providers/src/providers/volcengine/`(WSS 模板)、`providers/aliyun/`(HTTP 模板)
- `crates/agent-runtime-tts-providers/src/providers/mod.rs`、`Cargo.toml`
- `docs/external/minimax/{tts_sync,tts_async}.md`、`voice_clone/{voice-upload,example-upload}.md`

## 文件改动

- Modify: `crates/agent-runtime-tts-providers/Cargo.toml`(feature `minimax` + optional reqwest/hex)
- Modify: `crates/agent-runtime-tts-providers/src/providers/mod.rs`(feature-gated 声明)
- Modify: `crates/agent-runtime-tts-providers/src/types.rs`(`TtsOperation::Async`)
- Add: `crates/agent-runtime-tts-providers/src/providers/minimax/{mod,protocol,sync,async,files}.rs`

## 步骤

### 1. Cargo + 模块骨架

- [ ] `Cargo.toml` 加 `minimax` feature 和 optional `reqwest`(json+multipart）/`hex`(spec 4a)。
- [ ] `providers/mod.rs` 加 `#[cfg(feature = "minimax")] pub mod minimax;`。
- [ ] 建 `minimax/mod.rs`(`MinimaxTtsAdapter` + `impl TtsProvider`)及空 `protocol/sync/async/files.rs`。

### 2. protocol.rs

- [ ] 定义 `task_start`/`task_continue`/`task_finish` 请求帧与 `task_started`/`task_continued`/
      `task_finished` 响应帧的 serde struct。
- [ ] `decode_audio(hex_str) -> Vec<u8>`;处理 `data == null`。
- [ ] `map_base_resp(status_code, msg) -> Result<(), TtsError>` 按 spec 4c 表;1002/1039/2205 用
      `ProviderHttpError` + `status=429` + `upstream_code=原码`。

### 3. sync.rs(WSS)

- [ ] WSS 握手带 `Authorization: Bearer`,强制 `wss://`(参考 ASR 安全修复:拒绝 `ws://`)。
- [ ] 实现 start → continue → finish 帧驱动;聚合 `data.audio`。
- [ ] `TtsProvider::synthesize` / `stream_synthesize` / `start_duplex_stream` 接到 sync 流程
      (duplex:输入 channel → `task_continue` 序列)。

### 4. async.rs

- [ ] `TtsOperation::Async` variant。
- [ ] `synthesize` 按 `operation` 分派;async 路径 POST `/v1/t2a_async_v2`,解析
      `task_id`/`task_token`/`file_id` → `AudioData::Url`。
- [ ] `text_file_id` 路径调 `files::upload_file(.., FilePurpose::T2aAsyncInput, ..)`。

### 5. files.rs

- [ ] `FilePurpose { VoiceClone, PromptAudio, T2aAsyncInput }` + `as_str`。
- [ ] `upload_file(...)` multipart POST `/v1/files/upload`,解析 `{file:{file_id}}` → `u64`。
- [ ] `retrieve_file(file_id) -> download_url` 薄 helper(GET `/v1/files/retrieve`)。

### 6. 测试

- [ ] base_resp 16 码映射单元测试。
- [ ] hex 解码 + `data=null` 判空测试。
- [ ] sync `synthesize` 用帧夹具聚合出非空 bytes;stream 事件序列测试。
- [ ] async 返回 `AudioData::Url` 测试。
- [ ] `upload_file` multipart 构造测试。

### 7. 验证

```bash
cargo test -p agent-runtime-tts-providers --features minimax
cargo clippy -p agent-runtime-tts-providers --features minimax -- -D warnings
cargo build -p agent-runtime-tts-providers --no-default-features --features minimax
cargo fmt --check
```

Live(手动,记录验证报告):sync `synthesize("你好")`、async URL 路径(设计文档 §3.6)。
