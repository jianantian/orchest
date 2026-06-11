# 001 · ASR Provider 安全与正确性修复

## 背景

`agent-runtime-asr-providers` crate 在 v0.9.1 合入后经 code review 发现 2 个 Critical、5 个 Important、1 个 Medium 问题。涉及安全（API key 明文传输）、正确性（unwrap panic、tracing span 失效、dedup 丢数据）和代码质量。

## 1a. `build_run_task` / `build_finish_task` 使用 `unwrap()`（Critical）

**文件**：`crates/agent-runtime-asr-providers/src/providers/aliyun/mod.rs:156,170`

```rust
serde_json::to_string(&msg).unwrap()
```

AGENTS.md 禁令："`unwrap()` and `expect()` are banned in library code except inside `#[cfg(test)]` blocks"。虽然此序列化实践中不会失败（struct 是静态已知的），但违反项目规范。

**修复**：两个函数签名改为 `-> Result<String, AsrError>`：

```rust
serde_json::to_string(&msg).map_err(|e| {
    AsrError::new(AsrErrorCode::InvalidRequest, format!("failed to serialize message: {e}"))
})
```

**调用方影响**：
- `start_stream` 中 `build_run_task(...)` → 加 `?`
- `TaskParams::build_run_task()` 返回值改为 `Result<String, AsrError>`
- `adapter_task` 中 `build_finish_task(...)` 和 `task_params.build_run_task(...)` 在 `tokio::spawn` 内部，无法 `?`——改为 `.unwrap_or_else(|e| ...)` 发送 `AsrStreamEvent::Error` 并 return

## 1b. WebSocket URL 不校验 TLS（Critical）

**文件**：`aliyun/mod.rs:310-316`、`volcengine/mod.rs:193-208`

两个 adapter 的 `start_stream` 将 API key（`Authorization: bearer ...` / `X-Api-Key`）附加到 WebSocket request header 中。`extract_host` 显式处理 `ws://` 前缀，允许明文连接。

**修复**：在 `start_stream` 构建 `ws_request` 之前校验：

```rust
if !self.config.ws_url.starts_with("wss://") {
    return Err(AsrError::new(
        AsrErrorCode::InvalidRequest,
        "WebSocket URL must use wss:// for secure credential transport",
    ));
}
```

两个 adapter 各加一处。`extract_host` 保留 `ws://` 分支不删（测试工具可能用到），安全边界在 `start_stream` 层。

## 1c. `span.enter()` 跨 `.await`（Important）

**文件**：`aliyun/mod.rs:329-330`、`volcengine/mod.rs:217-218`

```rust
let span = observability::provider_stream_span(&trace_id, &model);
let _guard = span.enter();
// ... connect_async(...).await  ← guard 在多线程 executor 上语义错误
```

`span.enter()` 返回同步 `Entered` guard，跨 `.await` 持有会导致 span 记录到错误线程。

**修复**：删除 `_guard`，改用 `tracing::Instrument`：

```rust
use tracing::Instrument;

let span = observability::provider_stream_span(&trace_id, &model);
let (ws_stream, _response) = tokio_tungstenite::connect_async(ws_request)
    .instrument(span)
    .await
    .map_err(|e| { ... })?;
```

两个 adapter 均需添加 `use tracing::Instrument;`。

## 1d. `UtteranceDeduplicator` 使用 hash 做 dedup key（Important）

**文件**：`volcengine/mod.rs:285-302`

```rust
struct UtteranceDeduplicator {
    seen: HashSet<(i32, i32, u64)>,  // u64 是 text 的 hash
}
```

`DefaultHasher` 不保证抗碰撞。两个不同文本 hash 到同一 `u64` 时，后者被静默丢弃。

**修复**：key 直接含完整文本：

```rust
struct UtteranceDeduplicator {
    seen: HashSet<(i32, i32, String)>,
}

fn is_new(&mut self, u: &VolcengineUtterance) -> bool {
    let key = (u.start_time, u.end_time, u.text.clone());
    self.seen.insert(key)
}
```

删除 `use std::collections::hash_map::DefaultHasher` 和 `use std::hash::{Hash, Hasher}`。每个 segment 的 utterance 数量有限，String clone 的内存开销可忽略。

## 1e. `segment_finalized` 未重置（Important）

**文件**：`volcengine/mod.rs:458-515`

当 `is_last && !segment_finalized` 且 `end_requested == false`（flush 而非 end）时，`segment_finalized = true` 后函数继续 loop。下一个 segment 的 flush 检查 `if is_last && !segment_finalized` 永远为 false。

当前 capabilities 声明 `multi_segment_streaming: false`，所以不会有第二个 flush，但这是 latent bug。

**修复**：non-end `is_last` 路径末尾加：

```rust
segment_finalized = false;
segment_idx += 1;
segment_words.clear();
```

与 timeout 路径（约 line 581）保持一致。

## 1f. `serde_bytes_vec` 无注释说明有损行为（Important）

**文件**：`crates/agent-runtime-asr-providers/src/types.rs:72-83`

自定义 serializer 序列化只输出字节长度，反序列化为全零 Vec。行为是故意的（避免日志/debug 输出中包含大量音频二进制），但无注释，后续维护者可能误以为是 bug。

**修复**：加模块级注释：

```rust
/// Lossy serializer for audio byte data: serializes only the byte count,
/// deserializes as a zeroed buffer of that length. Intentional — used for
/// debug/log serialization where preserving audio payload is unnecessary.
/// Do NOT use for data transport or persistence.
mod serde_bytes_vec { ... }
```

## 1g. `select_for_streaming` / `select_for_transcribe` 重复（Important）

**文件**：`crates/agent-runtime-asr-providers/src/routing.rs:66-174`

两个公共方法逐字相同，仅 request 类型不同。

**修复**：提取私有方法：

```rust
fn select_provider(
    &self,
    model: Option<&str>,
    language: Option<&Language>,
) -> Result<Arc<dyn AsrProvider>, AsrError>
```

两个公共方法变为 thin wrapper，各自从 request 中提取 `model` 和 `language` 后调用 `select_provider`。公共 API 签名不变。

## 1h. `create_asr_provider_from_config` 中 `todo!()`（Medium）

**文件**：`crates/agent-runtime-asr-providers/src/config.rs:77-91`

`todo!()` 在 `volcengine` 和 `aliyun` feature 分支中——运行时 panic。

**修复**：

```rust
#[cfg(feature = "volcengine")]
"volcengine" => Err(AsrError::new(
    AsrErrorCode::UnsupportedOperation,
    "volcengine adapter creation from config is not yet implemented",
)),
```

`aliyun` 分支同理。

## 验收标准

- [ ] `build_run_task` / `build_finish_task` 返回 `Result<String, AsrError>`，无 `unwrap()`
- [ ] `ws://` URL 传入 `start_stream` 时两个 adapter 均返回 `AsrError::InvalidRequest`
- [ ] 两个 adapter 的 `span.enter()` + `_guard` 替换为 `.instrument(span)`
- [ ] `UtteranceDeduplicator.seen` 类型为 `HashSet<(i32, i32, String)>`，无 `DefaultHasher`
- [ ] non-end flush 后 `segment_finalized` 重置为 false
- [ ] `serde_bytes_vec` 模块有 doc comment 说明有损行为
- [ ] `select_for_streaming` / `select_for_transcribe` 共享私有 `select_provider` 方法
- [ ] `create_asr_provider_from_config` 中无 `todo!()`
- [ ] `cargo test -p agent-runtime-asr-providers` 全绿
- [ ] `cargo clippy -p agent-runtime-asr-providers -- -D warnings` 无 warning
