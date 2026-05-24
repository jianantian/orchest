# 014 · 端到端验收

## 背景

Issues 001-013 各自有单元级验收标准，但需要一个明确的集成验收节点，确保整条链路（Rust core → FFI → Python/TS SDK → Anthropic API → tool → event stream）在真实环境下端到端跑通。

## 目标

跑通全部 4 个 demo，并人工确认所有 `RuntimeEvent` 变体均能观测到。

## 验收标准

**Python：**
- [ ] `examples/python_basic.py` 成功运行，agent 完成任务，流式 token 输出正常
- [ ] `examples/python_async_tool.py` 成功运行，`async_tool_progress` 事件出现在输出中

**TypeScript：**
- [ ] `examples/ts_basic.ts` 成功运行，tool 调用事件正常打印
- [ ] `examples/ts_streaming.ts` 成功运行，逐 token 流式输出正常

**RuntimeEvent 覆盖检查：**
- [ ] Python 和 TypeScript demos 观测到的 `RuntimeEvent.type` 均使用 canonical snake_case wire format，不出现 camelCase 事件 discriminant
- [ ] `./scripts/check-ts-event-wire-naming.sh` 通过，防止 TS-facing contract/example/doc 文件重新引入 camelCase event discriminant
- [ ] Validation 输出或说明引用 [v0.1 E2E Validation Notes](../e2e-validation.md)，并明确标记 `read_file` path boundary 是 v0.1 known limitation
- [ ] 以下事件在至少一个 demo 中均可观测到：
  - `run_started`
  - `model_call_started` / `model_call_completed`
  - `model_stream_chunk`（text delta）
  - `model_stream_chunk`（thinking boundary ordering：`ThinkingStart` → `Thinking` → `ThinkingEnd`）
  - `tool_call_started` / `tool_call_completed`
  - `async_tool_started` / `async_tool_progress` / `async_tool_completed`
  - `skill_content_read`（需要配置 skill 目录）
  - `approval_requested` / `approval_granted`
  - `approval_requested` / `approval_denied`，并验证 denied 后不执行对应 tool、最终 run 仍可 `run_completed`
  - `budget_warning`（需要配置很小的 budget 触发）
  - `run_completed`

**RunState 序列化：**
- [ ] 在 run 中途（tool call 完成后）将 `RunState` 序列化为 JSON，验证格式有效
- [ ] 从 JSON 重新加载 `RunState`，字段无损失（`JobHandle.poll` 除外）

**Known-risk visibility：**
- [ ] `read_file` 读取已注册 skill `SKILL.md` 时可观测到 `SkillContentRead`
- [ ] `read_file` 读取非 skill 文件在 v0.1 被允许，但不会产生 `SkillContentRead`；该行为在 validation notes 中标为 accepted known risk
- [ ] 该 known risk cross-link 到 Polaris non-goals / 无沙箱运行建议

## 依赖

全部 001-013 issues 完成后执行。
