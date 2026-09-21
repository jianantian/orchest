# Required Agent Name

## Background

运行开始和 agent 间交接需要稳定、可读的身份标签；`system_prompt` 前 60 个字符不具备唯一性，也不应承担 agent identity 语义。

## Goal / Scope

- `AgentConfig` 增加必填 `name: String`。
- Rust builder、Python `Agent(...)`、Node `AgentOptions` 都要求调用方显式提供名称。
- `RunHookContext.agent_name`、`HandoffHookContext.previous_agent/new_agent` 与 `RuntimeEvent::AgentUpdated` 使用配置名称。
- 不提供默认名称，不从 `system_prompt` fallback。

## Acceptance Criteria

- [x] Rust、Python、Node 构造入口均无法省略 `name`。
- [x] `on_run_start` 收到显式 agent 名称。
- [x] handoff hook 与 `AgentUpdated` 记录显式的源/目标 agent 名称。
- [x] `system_prompt` 仍原样发送给模型，不参与名称生成。
- [x] workspace test、clippy 与 fmt 检查通过。
