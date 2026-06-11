# Hotfix 2026-06-12 PRD：Code Review 问题清偿

## 背景

v0.9.1（ASR Provider Gateway）和 v0.9.2（文档）合入后，对全仓库进行系统 code review，覆盖三个维度：

1. **ASR providers crate**（Aliyun/Volcengine adapter、streaming、routing、observability）
2. **Core runtime + 语言绑定**（agent-runtime-core、Python/Node bindings、TS/Python type stubs）
3. **CI/config/scripts**（workflow、Cargo workspace、lint 脚本、.gitignore）

综合内部 3-agent code review 和外部架构评审，共识别 6 个 Critical、13 个 Important、5 个 Medium、2 个 Low 级别问题。Critical 和 Important 级别的问题涉及安全（API key 明文传输）、正确性（async 死锁、tracing span 跨 await、budget 竞态）和 API 一致性（type stub 缺失事件），不适合推到后续功能迭代，需要在 v0.9.3 之前修复。

## 目标

修复 code review 识别的 Critical 和 Important 级别问题，使 runtime 在安全性、正确性和 API 一致性维度回到可信状态。Medium 级别中的 CI 和 config 问题一并修复（成本低、无风险）。

Hotfix 完成后：

1. WebSocket 连接必须强制 `wss://`，API key 不会通过明文传输
2. 库代码中不存在 `unwrap()` / `todo!()` 可触达的 panic 路径
3. Python 绑定在已有 event loop 中可用（async tool handler 不崩溃）
4. tracing span 正确 instrument 异步代码，不跨 `.await` 使用 `.enter()`
5. MCP 客户端单行 parse 错误不导致全部 pending request 丢失
6. `max_tool_calls` budget 在单步多 tool 场景下准确执行
7. TypeScript 和 Python type stub 与 Rust runtime 的事件枚举完整对齐
8. 慢 event subscriber 不阻塞 run loop，event 丢弃有通知
9. Model call 前 context window 超限时返回明确错误，不依赖 provider 侧报错
10. MCP 子进程 Drop 时不泄漏，Node binding event dropping 有通知
11. 未使用的 forward declaration 和 dead contract 已清除

## 成功指标

- `ws://` URL 传入 ASR provider 时返回 `AsrError::InvalidRequest`，不建立连接
- `build_run_task` / `build_finish_task` 返回 `Result`，无 `unwrap()`
- `create_asr_provider_from_config` 中 `todo!()` 替换为 `Err(AsrError)`
- Python async tool handler 在 `asyncio.run()` + `uvicorn` 下均可执行
- `which::which("deno")` 只调用一次，结果复用
- 两个 ASR adapter 的 `span.enter()` 替换为 `.instrument()`
- Volcengine 的 `UtteranceDeduplicator` 使用完整文本作为 dedup key
- MCP stdio reader 对单行 parse 错误 `continue` 而非 `break`
- `max_tool_calls` 在 budget check 通过后立即 `record_tool_call()`
- `register_persistence_hook` 不重复注册
- `js/index.d.ts` 和 `python/__init__.pyi` 包含 `run_aborted` 和 `events_dropped` 事件
- Python stdout pre-sentinel loop 有行数上限（10,000）
- `.gitignore` 中 `.DS_STORE` 修正为 `.DS_Store`
- `check-ts-event-wire-naming.sh` 中 scan target 路径与实际目录一致
- Primary event subscriber 超时后 run loop 继续执行，不阻塞
- model call 前 token count 超阈值时返回明确 runtime 错误（非 provider 500）
- `McpStdioClient::Drop` 不泄漏子进程
- Node binding event dropping 时发送 `EventsDropped` 通知
- `AgentRef` / `AgentError` 已删除，`on_update` 字段已删除
- 标准验证通过：`cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check`、`./scripts/lint-check.sh`

## 范围

### Issue 1：ASR Provider 安全与正确性修复

**Severity: Critical + Important**

| # | 问题 | 严重性 |
|---|------|--------|
| 1a | `build_run_task` / `build_finish_task` 使用 `unwrap()`，违反 AGENTS.md 禁令 | Critical |
| 1b | Aliyun/Volcengine adapter 不校验 `wss://`，API key 可能明文传输 | Critical |
| 1c | 两个 adapter 的 `span.enter()` 跨 `.await`，tracing span 在多线程 executor 上失效 | Important |
| 1d | Volcengine `UtteranceDeduplicator` 用 hash 而非原文做 dedup key，hash 碰撞会丢 utterance | Important |
| 1e | Volcengine `segment_finalized` 在 non-end flush 后未重置，multi-segment 场景有 latent bug | Important |
| 1f | `serde_bytes_vec` 序列化只保留长度、反序列化为全零，无注释说明有损行为 | Important |
| 1g | `select_for_streaming` / `select_for_transcribe` 完全相同，维护风险 | Important |
| 1h | `create_asr_provider_from_config` 中 `todo!()` 会 panic | Medium |

**修复方案：**

- 1a：`build_run_task` / `build_finish_task` 返回 `Result<String, AsrError>`，调用方 `?` 传播
- 1b：`start_stream` 中校验 URL 前缀必须为 `wss://`，否则返回 `AsrError::InvalidRequest`
- 1c：删除 `span.enter()` + `_guard`，改为 `tokio_tungstenite::connect_async(ws_request).instrument(span).await`
- 1d：`seen: HashSet<(i32, i32, String)>`，key 含完整文本，删除 `DefaultHasher` 引用
- 1e：non-end `is_last` 路径末尾加 `segment_finalized = false`
- 1f：在 `serde_bytes_vec` 模块顶部加注释说明有损行为是故意的（用于日志/debug 序列化，不用于数据传输）
- 1g：提取 `select_provider` 私有方法，两个公共方法变为 thin wrapper
- 1h：`todo!()` 替换为 `Err(AsrError::new(AsrErrorCode::UnsupportedOperation, ...))`

### Issue 2：Core Runtime 正确性修复

**Severity: Critical + Important**

| # | 问题 | 严重性 |
|---|------|--------|
| 2a | Python code exec 的 pre-sentinel stdout loop 无行数上限，可无限自旋 | Critical |
| 2b | JS tool `which::which("deno")` 调用两次，TOCTOU 竞态（概率极低，后果为 stdin 写入被忽略，不崩溃） | Medium |
| 2c | Python 绑定 `asyncio.run()` 在已有 event loop 中崩溃 | Critical |
| 2d | MCP stdio reader 一行 parse 失败就清空所有 pending request 并退出 | Important |
| 2e | `max_tool_calls` budget check 在 tool 执行后才 record，单步多 tool 可超限 | Important |
| 2f | `register_persistence_hook` 可重复注册 | Important |

**修复方案：**

- 2a：loop 中加计数器，超过 `MAX_PRE_SENTINEL_LINES`（10,000）后 kill 子进程并返回错误；非 sentinel 行通过 `emit_update` 发出
- 2b：`let use_deno = which::which("deno").is_ok();` 在分支前缓存，复用
- 2c：检测是否有正在运行的 event loop（`asyncio.get_running_loop()`）。无 loop 时用 `asyncio.run(coro)`；有 loop 时用 `asyncio.ensure_future(coro)` 提交到已有 loop，通过 `concurrent.futures.Future` + `threading.Event` 等待结果（`run_until_complete` 在已运行的 loop 中同样会抛 `RuntimeError`）。两处均需修改（tool handler 和 async job poll）
- 2d：`Err(_) => continue` 替代 `break`，仅在 EOF（`next_line` 返回 `None`）时 break 清空
- 2e：budget check 通过后立即调用 `state.budget.record_tool_call()`，删除 line 1092 的延迟 record
- 2f：push 前检查 hooks 中是否已存在相同 `session_id` 的 `SessionPersistenceHook`

### Issue 3：Type Stub 对齐

**Severity: Important**

`js/index.d.ts` 和 `python/agent_runtime/__init__.pyi` 的 `RuntimeEvent` union 缺少 `run_aborted` 和 `events_dropped` 两个变体。

**修复方案：**

TypeScript 添加：
```typescript
| { type: "run_aborted"; reason: string | null; run_depth: number }
| { type: "events_dropped"; subscriber_id: number; count: number; run_depth: number }
```

Python 添加等价的 `TypedDict`。

### Issue 4：CI 和 Config 修复

**Severity: Medium**

| # | 问题 |
|---|------|
| 4a | `.gitignore` 中 `.DS_STORE` 大小写错误，Linux 上不匹配 |
| 4b | `check-ts-event-wire-naming.sh` 扫描不存在的路径 `docs/iteration/v0_1/issues` |
| 4c | `agent-runtime-core/Cargo.toml` 使用 `tokio = { features = ["full"] }`，库 crate 不应全量引入 |

**修复方案：**

- 4a：`.DS_STORE` → `.DS_Store`
- 4b：路径更正为 `docs/archive/iteration`；加入启动时路径存在性校验
- 4c：`features = ["full"]` 替换为实际需要的 feature 列表（`rt`, `sync`, `time`, `macros`, `io-util`, `process`）

### Issue 5：Node 绑定 unsafe 审计

**Severity: Important**

`unsafe impl Send for JsTool` / `unsafe impl Sync for JsTool` 的 soundness 依赖 napi-rs 版本中 `ThreadsafeFunction` 的实际 trait bound。

**修复方案：**

添加编译期静态断言验证 `ThreadsafeFunction` 本身实现了 `Send + Sync`。若编译失败，则改用 `Arc<Mutex<...>>` 包装。

### Issue 6：Event Backpressure 与 Context Window 防御

**Severity: Critical + Important**

| # | 问题 | 严重性 |
|---|------|--------|
| 6a | Primary event subscriber 的 `.send().await` 阻塞 run loop——慢消费者直接卡死 agent | Critical |
| 6b | Secondary subscriber 用 `try_send` 静默丢事件；`EventsDropped` 通知本身也用 `try_send`，可被丢弃 | Critical |
| 6c | Model call 前不检查 token count，context overflow 时 provider 返回不可预期的错误（如 HTTP 500）而非明确的 runtime 错误 | Important |

**修复方案：**

- 6a/6b：primary subscriber 改为 `send_timeout`（如 500ms），超时发 `EventsDropped` 并继续。长期考虑迁移到 `tokio::sync::broadcast`，但 broadcast 的 lagging receiver 语义需要评估对 watcher 的影响，hotfix 阶段先用 timeout 兜底
- 6c：在 `run_one_step` 的 model call 前（约 line 480），估算 `messages + tool_defs` 的 token count，若超过 context window 阈值则返回明确的 `RuntimeEvent::RunFailed` 并附上 token count 信息，而非让 provider 返回不可预期的错误。Proactive compaction 触发作为后续功能迭代处理

### Issue 7：MCP 子进程泄漏与 Node Event Dropping

**Severity: Important**

| # | 问题 | 严重性 |
|---|------|--------|
| 7a | `McpStdioClient::Drop` 中 `try_lock` 失败时子进程泄漏为僵尸 | Important |
| 7b | Node binding event forwarding 使用 `NonBlocking` 模式，backpressure 下 JS callback 被静默跳过，无 `EventsDropped` 通知 | Important |

**修复方案：**

- 7a：在 `Drop` 之前，先 abort reader task（解除其对 Mutex 的持有），再 `try_lock` kill 子进程。顺序改为 `self.reader_abort.abort()` → `self.child.try_lock()` → `start_kill()`。`std::sync::Mutex` 没有 `try_lock_for`，不引入新依赖
- 7b：`NonBlocking` 的 `try_send` 失败时，通过 primary event channel 发送 `EventsDropped`，确保至少一个渠道能通知到消费者

### Issue 8：Dead Code 与 Dead Contract 清理

**Severity: Low**

| # | 问题 |
|---|------|
| 8a | `AgentRef` / `AgentError`（agent_ref.rs）标注 "v0.8 forward declaration" 但未被任何代码使用 |
| 8b | `ToolContext.on_update` 字段在 actor.rs:860 始终设为 `None`，是 dead contract |

**修复方案：**

- 8a：删除 `AgentRef` struct 和 `AgentError` enum 及其 `impl` 块
- 8b：删除 `on_update` 字段。`ExecutePythonTool` 已通过 `ctx.event_tx` 直接发送 `ToolCallUpdate`，`on_update` 是冗余路径

## 不在范围内

- ASR / Core 的新功能或 API 扩展
- `routing.rs` `select_for_streaming` / `select_for_transcribe` 的公共 API 签名变更（Issue 1g 仅提取内部共享实现为私有方法，不改公共接口）
- `SqliteSessionStore` 并发优化（performance concern，非正确性 bug）
- `to_snake_case` 去重到 common crate（重构，非 bug）
- `lint-check.sh` file-length check 的启用（待 A4 splits 完成）
- `run_one_step` 拆分（753 行，需专项设计阶段划分，已录入 todo backlog）
- Handoff 状态原地突变重设计（需快照/回滚或 terminate-and-restart，已录入 todo backlog）
- 消息历史零拷贝（`Arc<[Message]>` + CoW，需改所有权模型，已录入 todo backlog）
- 废弃 API 移除（`as_tool_legacy` 等，等 v1.0 breaking change 窗口，已录入 todo backlog）
- Handoff / Compaction / Crash-recovery 测试补齐（已录入 todo backlog）
- 代码执行沙箱注入点（功能扩展，已录入 todo backlog）
- 热路径静态 Value 优化（4 处 `json!`，低收益，已录入 todo backlog）
- v0.9.3 TTS Provider Gateway 的任何工作

## Issues 拆解

| Issue | 标题 | Severity |
|-------|------|----------|
| [001](./issues/001-asr-safety-correctness/spec.md) | ASR Provider 安全与正确性修复 | critical / important |
| [002](./issues/002-core-runtime-correctness/spec.md) | Core Runtime 正确性修复 | critical / important |
| [003](./issues/003-type-stub-alignment/spec.md) | TypeScript / Python Type Stub 对齐 | important |
| [004](./issues/004-ci-config-fixes/spec.md) | CI 和 Config 修复 | medium |
| [005](./issues/005-node-unsafe-audit/spec.md) | Node 绑定 unsafe 审计 | important |
| [006](./issues/006-event-backpressure-context/spec.md) | Event Backpressure 与 Context Window 防御 | critical |
| [007](./issues/007-mcp-leak-node-dropping/spec.md) | MCP 子进程泄漏与 Node Event Dropping | important |
| [008](./issues/008-dead-code-cleanup/spec.md) | Dead Code 与 Dead Contract 清理 | low |

## 建议执行顺序

1. **001 + 002 + 006**（可并行）：ASR、Core 正确性、event backpressure 各自独立，均为 Critical
2. **003 + 004 + 005 + 007 + 008**（可并行）：type stub、CI config、unsafe 审计、MCP 泄漏、dead code 互不依赖
3. 全量验证：`cargo test --workspace` + `cargo clippy` + `lint-check.sh` + `check-ts-event-wire-naming.sh`
