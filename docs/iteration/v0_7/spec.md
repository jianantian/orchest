# v0.7 Spec：扩展性地基

## 背景

v0.1–v0.6 + 两轮 hotfix 完成了核心 runtime、多 provider、MCP、sub-agent、budget 等功能，并清偿了结构债务。但 runtime 的 run loop 仍然是"硬编码单体"——所有横切关注点（approval、budget、compaction、error mapping）直接写在循环里，用户无法注入自定义行为。

三份竞品研究一致指出：**Hook / 中间件框架是 Orchest 与竞品之间最根本的架构差异**。OpenAI Agents SDK 有 4 层扩展（AgentHooks + RunHooks + Guardrail + ToolGuardrail），Claude Agent SDK 有 10 种 Hook 事件，DeerFlow 有 14 种 Middleware。Orchest 有零。

同时，sub-agent 的 `__sub_agent_request` + `AgentDelegate` 双路径问题在 v0.6 被标记为不在范围，需要在本轮解决。

## 目标

1. runtime 提供明确的生命周期 hook 点，用户可注入自定义行为
2. sub-agent 统一为 Agent-as-Tool + Handoff 两层语义，消除 `__sub_agent_request`
3. 模型调用失败时支持可配置的重试策略
4. 检测并阻止 agent 循环调用同一工具

## 范围

### Hook 框架

定义生命周期 hook 点：

```
on_run_start / on_run_end / on_run_error
before_model / after_model
before_tool / after_tool
on_handoff
before_compact
```

`Hook` trait，所有方法有默认空实现，用户只覆写关心的 hook 点。`AgentConfig` 接受 `Vec<Arc<dyn Hook>>`，按注册顺序链式调用。

Core 只提供框架和调用链，不内置任何具体 middleware 实现（极简 Core 原则）。

设计参考：
- OpenAI `RunHooks`（7 回调）— 简洁、事件驱动
- DeerFlow `AgentMiddleware`（6 hook 点）— wrap 模式，可拦截可修改
- Claude SDK `HookMatcher`（10 事件）— 统一回调签名

### Handoff 重构

消除 `__sub_agent_request` 魔法字段，统一为两层语义：

- **Agent-as-Tool**：委托子任务，父 agent 拿到结果继续。模型看到的是普通 tool
- **Handoff**：路由会话到另一个 agent，当前 agent 退出。模型通过特殊 tool 选择目标

设计文档：[sub-agent-handoff-vs-agent-as-tool.md](../../research/sub-agent-handoff-vs-agent-as-tool.md)

### LLM Retry

`RetryPolicy` 配置（max_retries、backoff strategy）。可重试错误分类：

- `rate_limit`（429）→ 指数退避
- `server_error`（5xx）→ 固定间隔重试
- `timeout` → 重试
- 其他错误 → 不重试

可以作为 hook 实现（`after_model` 拦截错误并重试），也可以在 loop 层实现。视实现复杂度决定。

### Loop Detection

检测 agent 循环调用同一工具模式，两级防御：

- **warn**：相同工具调用模式出现 N 次后注入警告消息（提示模型换策略）
- **hard stop**：超过上限后强制终止

作为 Hook trait 的具体实现提供。参考 DeerFlow `LoopDetectionMiddleware` 的滑动窗口 + 去重策略。

### 补充 Examples

v0.6 deferred。Hook、Handoff、Retry 的 API 稳定后补齐使用示例。

## 不在范围内

- Session 持久化 → v0.8
- 内置 Guardrail 实现 → v0.8（hook 框架提供基础）
- 权限模型扩展 → v0.8
- 新 Provider → v0.9
- Mid-run Steering → v0.9
- crates.io 发包 → v0.9

## 依赖

- hotfix 2026-05-26 全部完成（Error 类型稳定、crate 结构干净、ProviderFactory 就位）
- Handoff 重构依赖 Hook 框架先落地（handoff 事件需要通过 `on_handoff` hook 通知消费者）

## 验收标准

- [ ] `Hook` trait 定义完整，所有 hook 点可注册自定义实现
- [ ] `AgentConfig` 支持 `hooks: Vec<Arc<dyn Hook>>`
- [ ] `crates/` 中无 `__sub_agent_request` 字符串
- [ ] Agent-as-Tool 和 Handoff 各有独立的 API 和测试
- [ ] `RetryPolicy` 可配置，rate_limit 错误触发重试
- [ ] Loop detection hook 可检测重复工具调用并注入警告
- [ ] `examples/` 目录包含 hook、handoff、retry 的使用示例
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
