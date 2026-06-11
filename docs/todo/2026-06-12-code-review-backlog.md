---
name: project-code-review-backlog
description: 2026-06-12 全仓库 code review 识别的非 hotfix 项——重构、性能、测试、功能扩展，按优先级分档
metadata:
  node_type: memory
  type: project
  originSessionId: orchest-full-code-review-2026-06-12
---

2026-06-12 对 orchest 全仓库做了系统 code review（内部 3-agent 并行 + 外部架构评审），识别出一批不适合进 hotfix 但需要在 v1.0 前解决的问题。Critical/Important 级别已进入 [hotfix 06-12](../hotfix/2026_06_12/prd.md)，以下是剩余项。

---

## v1.0 前重构（高优先级）

### R1. `run_one_step` 拆分

**位置**: `crates/agent-runtime-core/src/run/actor.rs:425–1178`（753 行）

单函数混合五个阶段：limit/budget check → before_model hook + model call → after_model hook → tool execution loop（含 approval/timeout/dispatch/async job/handoff）→ step counter。`#[allow(clippy::too_many_lines)]` 注释承认了问题。

**建议**: 提取 `check_limits()`, `call_model()`, `execute_tool()`, `process_handoff()` 等阶段函数。每个只接收所需参数，不传 `&mut AgentRunState`。

**风险**: 控制流跨阶段有隐式依赖（如 handoff 中断当前步），需要先画出状态转移图再拆。

### R2. Handoff 状态原地突变

**位置**: `actor.rs:1101–1174`

5 个字段依次替换（messages, registry, tool_defs, budget, config），无快照/回滚。中间有一个 `await`（`apply_filter`），panic 会导致半新半旧的状态。

**建议**: Handoff 应产出一个 closing event 并终止当前 run。RunHandle 或 supervisor 观察到 handoff 后用新 config 重启。或者做 snapshot-then-swap：先构建完整的新 state，一次性赋值。

### R3. 消息历史零拷贝

**位置**: `actor.rs:478`（每步克隆）、`actor.rs:485`（retry 内再克隆）

`state.messages.clone()` 代价正比对话长度。长对话 + 多 retry 会产生大量临时分配。

**建议**: `Arc<[Message]>` + `Arc::make_mut` 实现 copy-on-write。需要改 Message 的所有权模型，影响面较大。

### R4. Handoff / Compaction / Crash-recovery 测试补齐

**位置**: `crates/agent-runtime-core/tests/`

现有 e2e 覆盖了完整 loop、approval、budget、async tool、watcher、guardrail。但缺三条路径：
- Handoff 状态突变的端到端验证（无 mock model 返回 `ToolOutput::Handoff`）
- Compaction 触发和 summary injection
- Supervisor crash-and-restart 的状态重放

---

## v1.0 Breaking Change 窗口

### B1. 废弃 API 移除

三处 deprecated 0.9.0 的 API 仍然存在且被绑定 crate 引用：
- `as_tool_legacy`（config.rs:106）——7 参数版本
- `ApprovalMode::SideEffectOnly`（config.rs:178）
- `AgentAsTool::new`（agent_as_tool.rs:71）

PRD 原文："正式移除留到下一个 breaking change 窗口"。v1.0 是该窗口。

---

## 待规划（中低优先级）

### P1. 代码执行沙箱注入点

**位置**: `crates/agent-runtime-core/src/tool/code_exec.rs`

`ExecutePythonTool` / `ExecuteJavaScriptTool` 直接 `Command::new` 裸进程。`ScriptExecutor` trait 只服务于 skill bundled scripts，未接入 code execution tools。doc comment 诚实承认了这一点。

**建议**: 让 code exec tools 接受 `Option<Arc<dyn ScriptExecutor>>`，默认 None 时行为不变，有值时通过 executor 运行代码。

### P2. 热路径静态 Value

**位置**: `actor.rs` 7 处 `json!` 调用

其中 4 处包裹纯静态字符串（"tool call skipped by hook", "tool call denied by user", "tool call budget exceeded", "tool execution timed out"），可提为 `const`/`static` 避免每次分配。但全部在 error/skip 路径上，不在每次成功 tool call 的热路径，实际收益有限。

### P3. 绑定 crate 代码去重

**位置**: `agent-runtime-py/src/lib.rs`（904 行）、`agent-runtime-node/src/lib.rs`（767 行）

两个 binding 共享大量相同逻辑：approval 解析、tool 注册、event 转发、budget config 转换、`to_snake_case`。divergence 会导致 Python 和 Node 行为不一致。

**建议**: 提取 FFI 无关的共享逻辑到 `agent-runtime-core` 的 `ffi_helpers` 模块，或新建 `agent-runtime-bindings-common` 内部 crate。

### P4. Python GIL 竞争文档

**位置**: `agent-runtime-py/src/lib.rs:91-94`

`PyTool::execute` 对每个 tool call 获取 GIL。如果 Python tool 执行 5 秒，GIL 被持有 5 秒，阻塞所有其他 Python 线程。

**建议**: 至少在文档中说明 GIL 约束。长期考虑 `pyo3-async-runtimes` 在 I/O 等待期间释放 GIL。

### P5. Observability 缺口

**位置**: `crates/agent-runtime-core/src/telemetry.rs`

当前只有 2 个 metric（`tool.call.duration`, `tool.call.count`），全部 tool 维度。缺：
- Model call duration / token usage histogram
- Budget utilization gauge
- Approval gate latency
- Compaction frequency / token savings
- Event channel backpressure（drops per subscriber）

### P6. `Arc<Mutex<Option<ActorRef>>>` 语义澄清

**位置**: `handle.rs:53`

外部评审认为 Mutex 不必要（"set once"），但实际 supervisor restart 时会写入新 ActorRef（supervisor.rs:171），Mutex 是必需的。代码本身没有注释说明为什么需要 Mutex。

**建议**: 加一行注释说明 "re-written on each supervisor restart, Mutex is required for concurrent watcher access"。

---

**来源**: 2026-06-12 内部 3-agent code review + 外部架构评审（Orchest Code Review — Full System Assessment）
