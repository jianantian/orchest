# 005 · Sub-Agent 事件流嵌套与透传

## 背景

父 agent 的消费者（用户代码）需要能观测到 sub-agent 内部发生的所有事件，包括 sub-agent 的 token streaming、tool 调用等，同时通过 `run_id` 区分事件来源，构建完整的执行树视图。

## 目标

Sub-agent 产生的所有 `RuntimeEvent` 透传到父 agent 的事件流，附加嵌套层级信息，不丢失任何事件。

## 验收标准

**事件透传：**
- [ ] 所有 `RuntimeEvent` 新增可选字段 `child_run_id: Option<RunId>`（仅 sub-agent 事件携带）
- [ ] Sub-agent 的每个事件在透传时保留原始 `run_id`（sub-agent 的 run ID）
- [ ] 父 agent 事件流中，sub-agent 事件和父 agent 自身事件按实际发生顺序交织出现

**层级标记：**
- [ ] `RuntimeEvent` 新增 `run_depth: u32` 字段（root = 0，每层 sub-agent +1）
- [ ] 消费者可通过 `run_depth` 和 `run_id` 构建完整执行树

**Sub-Agent 生命周期事件：**
- [ ] `RuntimeEvent` 新增 `SubAgentStarted { parent_run_id, child_run_id, config_summary }`
- [ ] `RuntimeEvent` 新增 `SubAgentCompleted { child_run_id, output, budget_used }`
- [ ] `RuntimeEvent` 新增 `SubAgentFailed { child_run_id, error }`

**SDK 层支持：**
- [ ] Python SDK：`async for event in agent.run(...)` 中自然包含 sub-agent 事件
- [ ] TS SDK：`for await (const event of agent.run(...))` 中自然包含 sub-agent 事件
- [ ] 事件的 `runDepth` 和 `childRunId` 字段在类型定义中可选（root agent 事件这两个字段为 null/undefined）

**不丢失事件：**
- [ ] Sub-agent 内部的 `ModelStreamChunk` 事件透传（token streaming 不中断）
- [ ] Sub-agent 的 approval 请求透传到父 agent 事件流，消费者可以统一处理审批

## 说明

Sub-agent 的 approval 响应通过 `run_handle.respond_approval(child_run_id, approved)` 触发，run_handle 能正确路由到对应的 sub-agent 等待 channel。
