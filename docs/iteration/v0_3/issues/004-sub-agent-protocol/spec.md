# 004 · Sub-Agent 协议与 Budget 继承

## 背景

复杂任务中，skill 可能需要启动一个专注于子任务的 agent（例如：research skill 启动一个专门负责网页摘要的 sub-agent）。Sub-agent 的资源消耗必须从父 agent 的预算中扣除，避免无限递归和预算失控。

## 目标

定义 sub-agent 的启动协议，实现 budget 从父 agent 到 sub-agent 的继承与实时累计。

## 验收标准

**启动接口（供 skill script 调用）：**
- [ ] `orchest-sdk` Python/TS 包提供 `create_sub_agent(parent_run_id, config, input)` 函数
- [ ] `orchest-sdk` 是 runtime **自动注入**的内置包，无需 skill 在 `dependencies` 里手动声明：Python 侧 runtime 在创建 skill venv 时预装，Node 侧放入 `NODE_PATH` 的内置目录（与 issue 001/002 的用户依赖机制分开管理）
- [ ] skill script 通过环境变量 `ORCHEST_PARENT_RUN_ID` 获取父 run ID
- [ ] sub-agent 的 `BudgetConfig` 由两部分约束：传入的显式配置 + 父 agent 剩余预算的上限

**Budget 继承规则：**
- [ ] `sub_budget.max_tokens = min(requested_max_tokens, parent_remaining_tokens)`
- [ ] `sub_budget.max_duration = min(requested_max_duration, parent_remaining_duration)`
- [ ] 其余 budget 维度同理
- [ ] 若父 agent 剩余预算为 0，sub-agent 创建立即失败，返回 `SubAgentFailed { reason: "parent_budget_exhausted" }`

**实时累计：**
- [ ] Sub-agent 每完成一次 model call 或 tool call，立即更新父 agent 的 `budget_used`
- [ ] 父 agent 的 budget guard 在下一次检查时能看到 sub-agent 的消耗
- [ ] Sub-agent 超出自己的预算时终止，父 agent 收到结果后继续运行

**嵌套深度限制：**
- [ ] Runtime 维护 `run_depth: u32`（root agent = 0，每层 sub-agent +1）
- [ ] `run_depth >= 3` 时，拒绝创建 sub-agent，返回错误
- [ ] `run_depth` 通过环境变量 `ORCHEST_RUN_DEPTH` 传递给 skill subprocess

## 说明

Sub-agent 的 `allowed_tools` 和 `allowed_skills` 默认继承父 agent 的配置，skill script 可以进一步收窄（不能扩展）。
