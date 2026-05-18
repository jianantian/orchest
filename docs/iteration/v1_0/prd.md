# v1.0 PRD：生产级完整度

## 目标

解决 v0.x 阶段遗留的三个开放问题，使 runtime 达到可以长期演化、对外稳定发布的完整度。

v1.0 结束时，开发者应该能够：
1. 在 skill 的 `SKILL.md` 中声明 Python/Node 依赖，runtime 自动准备隔离的依赖环境，无需用户手动全局安装
2. 让 agent 通过编写代码调用 tool（Code Execution as MCP），大幅降低复杂任务的 token 消耗
3. 在 skill 内部启动 sub-agent，sub-agent 的 budget 由父 agent 统一管控，事件流嵌套透传给外层消费者

## 成功指标

- Skill 依赖管理：包含 Python 依赖声明的 skill 在全新机器上首次运行，自动完成依赖安装并成功执行脚本
- Code Execution as MCP：agent 通过生成并执行代码完成同等任务时，平均 tool call 次数减少 ≥ 30%
- Sub-agent：父 agent 运行包含 sub-agent 的 skill，`budget_used` 正确累计子 agent 的消耗，父 agent 的事件流包含子 agent 的所有事件（带 `run_id` 区分）

## 范围

### Skill 依赖管理

- `SKILL.md` frontmatter 新增 `dependencies` 字段声明依赖
- Runtime 在首次执行 skill script 前自动准备隔离环境
- Python：使用 `venv` + `pip`；Node：使用 `npm`（单 skill 独立的 `node_modules`）
- 环境缓存：依赖 hash 不变时复用已有环境，不重复安装
- 安装失败时发出 `SkillDependencyError` 事件，该 skill 的 bundled tool 不可用

### Code Execution as MCP

- 内置 Code Execution MCP server，提供 `execute_python` 和 `execute_javascript` tool
- 每次 `agent.run()` 创建独立的 sandbox session（基于 `deno` 或受限的 subprocess）
- Session 在 run 结束时销毁，不同 run 间不共享状态
- 支持 stdout 流式输出（作为 `ToolCallUpdate` 事件）
- 严格的超时控制（默认 30s，可通过 tool input 覆盖，上限 5min）
- v1.0 不做完整沙箱（文件系统隔离、网络限制），记录为已知限制

### Skill Sub-Agent

- Skill 的 bundled script 可以通过 `orchest-sdk`（Python/TS 包）启动 sub-agent
- Sub-agent 的 `BudgetConfig` 从父 agent 的剩余预算中分配（不超过父 agent 剩余量）
- Sub-agent 的所有 `RuntimeEvent` 透传到父 agent 的事件流，附加 `parent_run_id` 和 `child_run_id` 字段
- 父 agent 的 `budget_used` 实时累计 sub-agent 的消耗
- Sub-agent 超出分配预算时，sub-agent 终止，父 agent 收到 `SubAgentFailed` 事件，继续运行
- 嵌套深度限制：最多 3 层（防止无限递归）

## 不在范围内

- Skill 沙箱（文件系统隔离、网络隔离）
- Multi-agent 并行编排（多个 agent 协作完成同一任务）
- 可视化调试工具
- Skill 版本管理与依赖锁定（`skill.lock` 文件）

## Issues 拆解

| Issue | 标题 |
|-------|------|
| [001](./issues/001-skill-deps-python.md) | Skill 依赖管理：Python venv 隔离 |
| [002](./issues/002-skill-deps-node.md) | Skill 依赖管理：Node npm 隔离 |
| [003](./issues/003-code-execution-mcp.md) | Code Execution as MCP（内置 code exec server） |
| [004](./issues/004-sub-agent-protocol.md) | Sub-Agent 协议与 budget 继承 |
| [005](./issues/005-sub-agent-event-nesting.md) | Sub-Agent 事件流嵌套与透传 |
