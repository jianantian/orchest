# v0.1 PRD：最小可用 Agent Runtime

## 目标

交付一个**可以真实运行 agent 任务**的最小 runtime，覆盖核心 loop 和双语言 SDK。

v0.1 结束时，开发者应该能够：
1. 用 Python 或 TypeScript 注册 tool、配置 skill 目录、调用 `agent.run(input)`
2. 实时消费 token 级流式输出
3. 注册长时异步 tool（提交任务 + 返回 job handle），runtime 自动轮询并发出进度事件
4. 通过 budget 配置限制 token、tool call 次数、运行时长
5. 对标记了 `requires_approval` 的 tool 接收审批事件并动态决策

## 成功指标

- Python SDK 和 TypeScript SDK 各有一个可运行的 demo（basic + async tool）
- 所有 RuntimeEvent 变体在 demo 中均能观测到
- 单次 agent run 的端到端延迟开销（除去模型 API 耗时）< 50ms
- RunState 可序列化为 JSON，并能从 JSON 重新加载（不含 JobHandle 闭包）

## 范围

### Rust Core

| 模块 | 说明 |
|------|------|
| Agent run loop | 基于 tokio task + mpsc channel，顺序执行 tool call |
| Model streaming | `ModelAdapter::stream()`，逐 chunk 发出 `ModelStreamChunk` 事件 |
| RunState 管理与序列化 | 含 `schema_version` 字段，支持序列化到 JSON |
| Skill 目录扫描 | 扫描 SKILL.md，加载 `SkillManifest`，全部 bundled tool 注册到 registry |
| Tool registry | 支持 InProcess 和 Skill bundled 两种来源 |
| 异步 job 轮询 | `ToolOutput::AsyncJob`，background task 轮询，发出 progress 事件 |
| Anthropic Claude adapter | 支持 streaming，发出 `ModelStreamChunk` |
| Builtin `read_file` tool | 用于 agent 按需读取 skill 内容 |
| Skill bundled script 执行 | spawn 子进程，解析 stdout 为 tool result 或 async job 协议 |
| Budget guard | 检查 max_tokens / max_tool_calls / max_duration / max_cost_usd |
| Approval gate | oneshot channel 暂停 loop，等待 `run_handle.respond_approval()` |
| 完整 event 流 | 覆盖所有 `RuntimeEvent` 变体 |

### Python SDK

- PyO3 binding
- async generator 事件流（`async for event in agent.run(...)`）
- `@agent.tool` decorator 风格注册，支持 `requires_approval`、`side_effect`
- 支持 tool handler 返回 async job dict（`{"async_job": {"job_id": ..., "poll": ...}}`）

### TypeScript SDK

- napi-rs binding
- `AsyncIterator` 事件流（`for await (const event of agent.run(...))`）
- `agent.tool({name, description, input, handler})` 函数式注册
- 支持 tool handler 返回 `{ asyncJob: { jobId, pollIntervalMs, poll } }` 对象

## 不在范围内

见 [non-goals.md](../../polaris/non-goals.md) 的"v0.1 明确不做"部分。

## 交付物

- `crates/agent-runtime-core/`：Rust 核心 crate
- `crates/agent-runtime-py/`：Python binding + `agent_runtime` Python 包
- `crates/agent-runtime-node/`：TypeScript binding + `@orchest/agent-runtime` npm 包
- `examples/python/python_basic.py`：Python 基础 demo
- `examples/python/python_async_tool.py`：Python 长时异步 tool demo（视频生成场景）
- `examples/typescript/ts_basic.ts`：TypeScript 基础 demo
- `examples/typescript/ts_streaming.ts`：TypeScript 流式输出 demo

## Issues 拆解

| Issue | 标题 |
|-------|------|
| [001](./issues/001-project-setup/spec.md) | 项目结构与 Cargo workspace 初始化 |
| [002](./issues/002-core-types/spec.md) | 核心类型定义（Tool trait、RunState、RuntimeEvent 等） |
| [003](./issues/003-tool-registry/spec.md) | Tool registry 与 InProcess tool |
| [004](./issues/004-model-adapter/spec.md) | ModelAdapter trait 与 Anthropic streaming adapter |
| [005](./issues/005-run-loop/spec.md) | Agent run loop（顺序执行、事件流） |
| [006](./issues/006-budget-guard/spec.md) | Budget guard |
| [007](./issues/007-approval-gate/spec.md) | Approval gate（oneshot channel 暂停/恢复） |
| [008](./issues/008-async-job/spec.md) | 异步 job 轮询（ToolOutput::AsyncJob） |
| [009](./issues/009-skill/spec.md) | Skill 目录扫描与 manifest 加载 |
| [010](./issues/010-skill-bundled-tool/spec.md) | Skill bundled script 执行与 async job 协议解析 |
| [011](./issues/011-builtin-read-file/spec.md) | Builtin read_file tool |
| [012](./issues/012-python-sdk/spec.md) | Python SDK（PyO3 binding） |
| [013](./issues/013-typescript-sdk/spec.md) | TypeScript SDK（napi-rs binding） |
| [014](./issues/014-e2e-validation/spec.md) | 端到端验收（4 个 demo 全部跑通） |
