# 005 · Agent Run Loop

## 背景

Run loop 是 runtime 的核心：驱动模型调用 → tool 执行 → 结果回填的循环，并通过 event channel 把每一步暴露给外部。

## 目标

实现完整的 agent run loop，顺序执行 tool call，正确发出所有生命周期事件。

## 验收标准

- [ ] `AgentRun::start(config, input, tools) -> (RunHandle, EventStream)` 启动一次 run，返回 handle 和事件流
- [ ] `EventStream` 是 `tokio_stream::Stream<Item = RuntimeEvent>`（或等价 async iterator）
- [ ] loop 开始时发出 `RunStarted`
- [ ] 每步模型调用前发出 `ModelCallStarted { step }`
- [ ] 调用 `model.stream()`，每个 chunk 转发为 `RuntimeEvent::ModelStreamChunk`
- [ ] 模型调用结束后发出 `ModelCallCompleted { tokens }`
- [ ] 解析 response：`EndTurn` → 发出 `RunCompleted`，`ToolUse` → 进入 tool 执行
- [ ] tool 执行前发出 `ToolCallStarted { tool, source, input }`
- [ ] `ToolOutput::Immediate` → 发出 `ToolCallCompleted`，追加 tool result message，继续 loop
- [ ] `ToolOutput::AsyncJob` → 转交给异步 job 轮询（issue 008），等待结果后继续 loop
- [ ] tool 执行异常 → 发出 `ToolCallFailed`，tool result message 包含错误信息
- [ ] 多个 tool call 顺序执行（不并行）
- [ ] `step >= max_steps` 时发出 `RunFailed { error: "max_steps_reached" }`
- [ ] run loop 在独立 tokio task 中运行，不阻塞调用方

## System Prompt 构造

在第一次模型调用前，将 skill 列表拼入 system prompt：

```
{user_system_prompt}

## Available Skills
The following skills are available. To use a skill, read its SKILL.md file
using the read_file tool.

- {skill_name} ({skill_path}/SKILL.md): {skill_description}
...
```

没有 skill 时不追加该部分。

## 依赖

- Issue 002：核心类型
- Issue 003：ToolRegistry
- Issue 004：ModelAdapter
- Issue 006：Budget guard（集成）
- Issue 007：Approval gate（集成）
- Issue 008：Async job 轮询（集成）
