# 001 · ASR Provider 安全与正确性修复 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Remove panic, insecure transport, tracing, deduplication, and routing correctness risks from `agent-runtime-asr-providers`.

**Architecture:** Keep public ASR provider APIs stable. Make small internal changes in Aliyun, Volcengine, routing, config, and serialization helpers, with focused unit tests where behavior is externally observable.

**Tech Stack:** Rust, Tokio, tokio-tungstenite, tracing, serde_json.

---

## 要读的现有代码

- `crates/agent-runtime-asr-providers/src/providers/aliyun/mod.rs`
- `crates/agent-runtime-asr-providers/src/providers/volcengine/mod.rs`
- `crates/agent-runtime-asr-providers/src/routing.rs`
- `crates/agent-runtime-asr-providers/src/config.rs`
- `crates/agent-runtime-asr-providers/src/types.rs`
- `crates/agent-runtime-asr-providers/tests/router.rs`

## 文件改动

- Modify: `crates/agent-runtime-asr-providers/src/providers/aliyun/mod.rs`
- Modify: `crates/agent-runtime-asr-providers/src/providers/volcengine/mod.rs`
- Modify: `crates/agent-runtime-asr-providers/src/routing.rs`
- Modify: `crates/agent-runtime-asr-providers/src/config.rs`
- Modify: `crates/agent-runtime-asr-providers/src/types.rs`
- Test: add or extend tests in provider modules and `crates/agent-runtime-asr-providers/tests/router.rs`

## 步骤

### 1. Aliyun JSON task builders 改为 fallible

- [ ] 修改 `build_run_task` 和 `build_finish_task` 返回 `Result<String, AsrError>`。
- [ ] 将 `serde_json::to_string(&msg).unwrap()` 改为：

```rust
serde_json::to_string(&msg).map_err(|e| {
    AsrError::new(
        AsrErrorCode::InvalidRequest,
        format!("failed to serialize aliyun task message: {e}"),
    )
})
```

- [ ] 将 `TaskParams::build_run_task` 改为 `Result<String, AsrError>`。
- [ ] 在 `start_stream` 初始化路径用 `?` 传播 `build_run_task(task_id, model, sample_rate, format, vocabulary_id, &options)`。
- [ ] 在 spawned adapter task 内，对 `build_finish_task(&_current_task_id)` 和 `task_params.build_run_task(&_current_task_id)` 使用 `match`；失败时发送 `AsrStreamEvent::Error(err)` 并 `return`，不要在 spawned task 内 panic。
- [ ] 更新 Aliyun module 内现有 builder 单元测试，使用 `.unwrap()` 只允许在 `#[cfg(test)]` 测试中。

### 2. 两个 adapter 强制 `wss://`

- [ ] 在 `AliyunAsrAdapter::start_stream` 构建 request 前加入：

```rust
if !self.config.ws_url.starts_with("wss://") {
    return Err(AsrError::new(
        AsrErrorCode::InvalidRequest,
        "WebSocket URL must use wss:// for secure credential transport",
    ));
}
```

- [ ] 在 `VolcengineAsrAdapter::start_stream` 加同样校验。
- [ ] 添加两个单元测试：`aliyun_rejects_insecure_ws_url`、`volcengine_rejects_insecure_ws_url`。构造 `ws://example.invalid` config，调用 `start_stream`，断言返回 `AsrErrorCode::InvalidRequest`。

### 3. tracing span 改为 `.instrument(span)`

- [ ] 在 Aliyun 和 Volcengine adapter 文件中引入 `use tracing::Instrument;`。
- [ ] 删除 `let _guard = span.enter();`。
- [ ] 将 `tokio_tungstenite::connect_async(ws_request).await` 改为：

```rust
tokio_tungstenite::connect_async(ws_request)
    .instrument(span)
    .await
```

- [ ] 全局确认无 ASR provider `span.enter()` 跨 `.await`：

```bash
rg "span\\.enter|_guard" crates/agent-runtime-asr-providers/src
```

### 4. Volcengine dedup key 使用完整文本

- [ ] 删除 `DefaultHasher` 和 `std::hash::{Hash, Hasher}` imports。
- [ ] 将 `UtteranceDeduplicator.seen` 改为 `HashSet<(i32, i32, String)>`。
- [ ] 将 key 构造改为 `(u.start_time, u.end_time, u.text.clone())`。
- [ ] 添加或更新单元测试：两个 utterance 有相同 start/end 但不同 text 时都应 `is_new() == true`。

### 5. Volcengine non-end flush 后重置 segment 状态

- [ ] 在 `is_last && !segment_finalized` 且不是 end 的路径末尾加入：

```rust
segment_finalized = false;
segment_idx += 1;
segment_words.clear();
```

- [ ] 确认 timeout 路径和 non-end flush 路径状态更新一致。
- [ ] 添加 fake/adapter 单元测试：连续两个 flush segment 都能产生 final segment event，不被第一段的 `segment_finalized = true` 卡住。

### 6. 标注文档化有损 serializer

- [ ] 在 `types.rs` 的 `serde_bytes_vec` 模块前加入 doc comment，明确只用于 debug/log 序列化，不用于传输或持久化。

### 7. Router 选择逻辑去重

- [ ] 在 `AsrProviderRouter` impl 内提取：

```rust
fn select_provider(
    &self,
    model: Option<&str>,
    language: Option<&Language>,
) -> Result<Arc<dyn AsrProvider>, AsrError>
```

- [ ] `select_for_streaming` 从 `StreamingAsrRequest` 取 `model` / `language` 后调用私有方法。
- [ ] `select_for_transcribe` 从 transcribe request 取 `model` / `language` 后调用私有方法。
- [ ] 保持 public method signatures 不变。
- [ ] 运行 router 测试确认行为不变。

### 8. Config factory 删除 `todo!()`

- [ ] 在 `create_asr_provider_from_config` 中将 feature-gated `todo!()` 分支改为 `Err(AsrError::new(AsrErrorCode::UnsupportedOperation, "volcengine adapter creation from config is not yet implemented"))`；Aliyun 分支使用同样错误码和 `"aliyun adapter creation from config is not yet implemented"`。
- [ ] 添加测试覆盖 `volcengine` / `aliyun` provider config 返回 `UnsupportedOperation` 而不是 panic。

### 9. 验证

```bash
cargo test -p agent-runtime-asr-providers
cargo clippy -p agent-runtime-asr-providers -- -D warnings
cargo fmt --check
rg "todo!\\(|DefaultHasher|span\\.enter|serde_json::to_string\\([^\\n]+\\)\\.unwrap\\(" crates/agent-runtime-asr-providers/src
```

`rg` 最后一条应无生产代码命中；测试代码中的 `.unwrap()` 可接受。
