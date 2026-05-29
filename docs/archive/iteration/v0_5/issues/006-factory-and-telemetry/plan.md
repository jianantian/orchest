# 006 实现路线

## 步骤

1. **实现 factory 函数**
   - 在 `lib.rs` 中添加 `create_adapter()` — 逻辑简单：`split_once('/')` → match provider → 构造对应 Config + `from_config()`
   - 两个 error case：无 `/` → `"invalid_model"`，未知 provider → `"unknown_provider"`
   - OpenRouter 特殊处理：`openrouter/` 后面全部内容作为 model name
   - 各 adapter 的 `max_tokens` 默认 4096

2. **实现便利函数**
   - `stream_chat()` 和 `chat()` 都在 `lib.rs` 中
   - `stream_chat()`：创建 `mpsc::channel(64)`，调用 `adapter.complete(messages, tools, options, Some(tx))`，返回 `(future, rx)`
   - `chat()`：直接 `adapter.complete(messages, tools, options, None).await`
   - 注意 `stream_chat` 返回的是 `impl Future + '_`，不是 async fn——因为要同时返回 receiver

3. **实现 telemetry.rs**
   - 创建 `crates/agent-runtime-providers/src/telemetry.rs`，`lib.rs` 添加 `pub mod telemetry;`
   - `model_complete_span(provider, model, streaming)` → `tracing::info_span!("model.complete", provider, model, streaming)`
   - 6 个 metric name 常量：`pub const METRIC_REQUEST_DURATION: &str = "model.request.duration";` 等
   - 纯定义，不安装 subscriber

4. **创建共享测试工具**
   - `src/test_util.rs`（`#[cfg(test)]`）
   - `serve_sse_once` helper：bind `127.0.0.1:0` → accept 一个连接 → 返回 SSE 响应 → 返回 addr
   - 从 issue 002/003 中已有的测试 helper 统一抽取（如果各 adapter 已各自实现，此时 refactor 为共享）

5. **完善 lib.rs 导出**
   - re-export 完整列表：所有 types + 4 个 adapter + 4 个 config + factory + helpers + telemetry
   - `sse` 模块保持 `pub(crate)`

6. **写测试**
   - Factory 测试（4 个）：路由正确、unknown provider、no slash、openrouter full model
   - Helper 测试（2 个）：stream_chat 返回 pair、chat 返回 response
   - Runtime contract 测试（3 个）：语义等价、底层 complete 调用、backpressure — 需要 MockAdapter 实现 ModelAdapter trait
   - 跨 adapter 集成测试（7 个）：capabilities / options / message_serialization / stop_reason
   - Telemetry 测试（4 个）：用 `tracing-subscriber` 的 in-memory layer 和 `metrics-util` 的 `DebuggingRecorder`

## 要读的现有代码

- 各 adapter 的 `from_config()` 签名——factory 要能构造它们
- 现有测试中的 `serve_sse_once` 实现——统一到 `test_util.rs`

## 关键决策

- MockAdapter 放在 `test_util.rs` 中，实现最简 `ModelAdapter`（complete 直接返回固定 response），用于 helper 和 runtime_contract 测试
- 跨 adapter 集成测试需要各 adapter 已就位，可以用 `create_adapter()` + 环境变量（或传 api_key）构造——但集成测试不应该真正调 provider API。用 MockAdapter 或 `serve_sse_once` 模拟
