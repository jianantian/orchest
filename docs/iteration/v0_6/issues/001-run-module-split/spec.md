# 001 · run.rs 模块化拆分

## 背景

`crates/agent-runtime-core/src/run.rs` 当前 4040 行，混合了以下职责：

- 公共类型（`RunId`, `AgentConfig`, `RunState`, `RunStatus`）
- `RunHandle` + approval slot 逻辑
- `AgentRun::start` 入口
- `run_loop` 主循环（~500 行）
- 单次 tool call 执行分发（~400 行）
- Sub-agent 代理（`execute_sub_agent_request`, `execute_agent_delegate`，~300 行）
- Skill 注册（`register_skills`，~100 行）
- Webhook runtime（`WebhookRuntime`, `start_webhook_server`，~200 行）
- Context compaction（`maybe_compact_context`，~100 行）
- 工具相关 helpers（`truncate_output`, `append_searched_tool_defs`, `parse_budget_config` 等）
- 异步 job 轮询（`poll_async_job`，~150 行）
- MCP 连接（`connect_mcp_servers`，~80 行）

这是纯重构 issue：**公共 API 不变，测试不变，只改文件组织**。

## 目标

把 `run.rs` 拆分为 `run/` 目录，每个文件职责单一，最大文件不超过 700 行。

## 目标文件结构

```
crates/agent-runtime-core/src/run/
├── mod.rs          ← pub re-exports + AgentRun::start 入口（<120 行）
├── config.rs       ← RunId, AgentConfig, RunState, RunStatus（<120 行）
├── handle.rs       ← RunHandle, ApprovalSlot, take_and_send（<80 行）
├── loop_.rs        ← run_loop 主循环骨架（<350 行）
├── tool_exec.rs    ← 单次 tool call 分发、approval gate、timeout、poll_async_job（<400 行）
├── sub_agent.rs    ← SubAgentRuntime, execute_sub_agent_request, execute_agent_delegate（<300 行）
├── skills.rs       ← register_skills（<150 行）
├── webhook.rs      ← WebhookRuntime, start_webhook_server, write_http_response（<250 行）
├── compaction.rs   ← maybe_compact_context（<150 行）
└── helpers.rs      ← truncate_output, truncate_str_utf8_safe, narrow_permission_list,
                       min_option, min_option_f64, parse_budget_config,
                       append_searched_tool_defs, connect_mcp_servers（<400 行）
```

## 可见性规则

- `mod.rs` 中 `pub use` 的符号集合与当前 `run.rs` 的 `pub` 导出完全一致
- 子模块间通过 `pub(super)` 或 `pub(crate)` 共享内部符号，不暴露到 crate 外
- `run_loop`, `emit`, `register_skills` 等当前是 crate-internal 的函数保持 `pub(crate)` 或 `pub(super)`

## 验收标准

### 结构

- [ ] `crates/agent-runtime-core/src/run.rs` 文件不再存在
- [ ] `crates/agent-runtime-core/src/run/mod.rs` 存在，包含 `pub use` 重新导出
- [ ] `run/` 下每个文件行数不超过 700 行（`wc -l` 可验证）
- [ ] 每个文件有清晰的单一职责，头部注释说明职责（一行即可）

### 正确性

- [ ] `cargo build -p agent-runtime-core` 通过，无编译错误
- [ ] `cargo test --workspace` 全部通过，无 regression
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

### API 兼容性

- [ ] `crate::run::RunId` 路径可用
- [ ] `crate::run::AgentConfig` 路径可用
- [ ] `crate::run::RunHandle` 路径可用
- [ ] `crate::run::AgentRun` 路径可用
- [ ] `crate::run::RunStatus` 路径可用
- [ ] `crate::run::RunState` 路径可用
- [ ] Python / Node FFI binding crate 中的 import path 无变化（`use agent_runtime_core::run::*`）

## 注意事项

- 这是纯移动/重组，不修改任何业务逻辑。遇到拆分歧义，优先按现有函数调用关系分组，不强行解耦
- `loop_.rs` 调用 `tool_exec.rs` 中的函数时，使用 `super::tool_exec::*` 或 `use crate::run::tool_exec::*`
- 跨子模块循环引用是拆分失败信号，应重新归组
- 如果某个 helper 函数被多个子模块使用，放在 `helpers.rs`，而不是复制
