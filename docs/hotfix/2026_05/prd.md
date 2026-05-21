# Hotfix 2026-05 PRD：Runtime Contract Repair

## 背景

当前实现已经覆盖 v0.1-v0.3 的主要模块，但深入 review 发现几个已进入公共 API 或安全语义的 contract 偏差：

- `allowed_tools` / `allowed_skills` 暴露为 permission boundary，但 root run 没有执行该限制
- `ToolMetadata.timeout` / `max_output_tokens` 是公开 contract，但 run loop 未统一 enforce
- `skills_dir`、skill bundled tools、`SkillMissingCapabilities` 没有接到 SDK / runtime 入口
- Python / TypeScript SDK 与文档承诺的事件流、tool handler、approval API 不一致
- OpenAI tool-call 历史映射不符合 Chat Completions 协议
- Sub-agent approval 路由、事件层级、budget 实时累计没有完成
- MCP HTTP 对 side-effectful tool call 做无条件重试，可能重复执行

这些问题不适合作为新功能继续向后推，因为它们会让后续阶段建立在错误的权限、SDK 和 provider contract 上。

## 目标

在进入下一阶段前，完成一次 hotfix iteration，使 runtime 对外 contract 回到可信状态。

Hotfix 完成后：

1. 公开配置字段必须有实际行为，尤其是权限边界和 tool metadata
2. SDK demo 与文档验收标准必须验证真实 handler 执行，而不是只验证事件打印
3. Anthropic / OpenAI adapter 均能支撑至少一轮 tool call 后继续对话
4. Skill loading 从 SDK 入口可用，capability warning 可观测
5. Sub-agent lifecycle 不会在 approval 场景死锁，budget 和事件来源可追踪
6. E2E 覆盖必须防止这些 contract 再次静默回退

## 成功指标

- `allowed_tools` 限制下，未允许的 tool 不出现在 model-visible schemas 中，即使模型猜到名称也不能执行
- 配置 `ToolMetadata.timeout` 的 in-process tool 超时后发出 `ToolCallFailed`，run 继续把错误作为 tool result 回传模型
- 配置 `ToolMetadata.max_output_tokens` 的 tool 输出被截断，并在输出中带有明确截断标记
- Python 和 TypeScript demo 中至少一个 tool handler 的真实返回值出现在 `tool_call_completed.output`
- `skills_dir` 中声明的 bundled tool 可通过 SDK run 调用；含 `scripts/` 且无 `capabilities` 的 skill 发出 `SkillMissingCapabilities`
- OpenAI adapter 的集成测试覆盖：assistant tool call -> tool result -> final answer
- Sub-agent 内部 approval 可通过 `respond_approval(child_run_id, approved)` 唤醒
- MCP HTTP side-effectful `tools/call` 不会在请求失败后自动重复执行
- 标准验证通过：`cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check`、`./scripts/check-ts-event-wire-naming.sh`

## 范围

### Contract Repair

- Enforce root-level `AgentConfig.allowed_tools`
- Define and enforce `allowed_skills` behavior for skill scanning / bundled tool registration
- Enforce `ToolMetadata.timeout` and `max_output_tokens` in the shared tool dispatch path
- Preserve provider-neutral run loop: provider-specific message mapping stays inside adapters

### SDK Repair

- Python SDK provides an event-streaming API consistent with docs
- TypeScript SDK supports registering executable handlers, not only schemas
- Approval responses are tied to active run handles and routed by `run_id`
- Existing sync demo helpers may remain only as compatibility wrappers if they do not hide contract failures

### Skill Entry Repair

- SDK/runtime entrypoints scan `skills_dir`
- Register skill bundled tools with dependencies and capabilities
- Register `read_file` skill paths so `SkillContentRead` is emitted for known `SKILL.md` reads
- Emit `SkillMissingCapabilities` for script skills without capability declarations

### Provider Repair

- OpenAI adapter serializes assistant `tool_calls` and `tool` result messages correctly
- Anthropic adapter fails clearly on malformed provider SSE JSON instead of silently producing an empty response

### Sub-Agent Repair

- Sub-agent events include enough identity metadata for consumers to build the run tree
- Approval requests inside sub-agents are routable
- Parent budget usage updates as child events arrive, not only after child run completes

### Reliability Repair

- MCP HTTP retry behavior is safe for side-effectful tool calls
- Timed-out subprocess/code-exec children are terminated
- E2E tests cover negative and boundary cases, not only happy path events

## 不在范围内

- Full process sandboxing
- New provider support beyond repairing Anthropic/OpenAI behavior
- Parallel tool execution
- Full durable run resume implementation beyond preserving existing `RunState` serialization behavior
- UI / web dashboard work

## Issues 拆解

| Issue | 标题 |
|-------|------|
| [001](./issues/001-permission-boundary.md) | Enforce permission boundaries |
| [002](./issues/002-tool-execution-contract.md) | Enforce tool execution metadata |
| [003](./issues/003-skill-entrypoint.md) | Wire skill loading into SDK/runtime entrypoints |
| [004](./issues/004-sdk-contracts.md) | Repair Python and TypeScript SDK contracts |
| [005](./issues/005-provider-tool-protocol.md) | Repair provider tool protocol mappings |
| [006](./issues/006-sub-agent-routing.md) | Repair sub-agent routing, events, and budget |
| [007](./issues/007-mcp-and-process-reliability.md) | Fix MCP retry and child process timeout behavior |
| [008](./issues/008-hotfix-e2e-validation.md) | Add hotfix regression validation |

## 建议执行顺序

1. 001 + 002：先修权限和 tool contract，减少后续测试误判
2. 003 + 004：接通真实 SDK/skill 入口
3. 005：修 provider adapter，使 SDK smoke tests 能覆盖 OpenAI
4. 006：修 sub-agent 的跨 run 行为
5. 007：收敛可靠性风险
6. 008：补齐防回归验证，并将 hotfix 标记完成
