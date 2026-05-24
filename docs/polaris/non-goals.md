# 不做什么（Non-Goals）

明确不做什么，与明确要做什么同等重要。以下是本 runtime 的硬性边界，遇到需求冲动时应回来核对。

## 永久 Non-Goals

**"什么都能做"的 agent 框架**
- 不内置复杂工作流引擎（DAG、条件分支、循环编排）
- 不提供预制 agent 模板和角色系统
- 不提供 skill marketplace 或内置 skill 库
- 不试图替代 LangChain / LangGraph 的全部功能

**绑定特定厂商**
- 不绑定 Anthropic 作为唯一模型 provider（v0.1 只实现 Anthropic 是务实选择，不是设计约束）
- 不假设用户使用特定云平台或基础设施

**偏离 Anthropic Agent Skills 开放标准**
- SKILL.md 格式不得与官方标准不兼容
- 不创造与标准并行的私有 skill 格式

**内置观测平台**
- 不内置日志后端、metrics exporter、OpenTelemetry collector、Prometheus server 或 Web dashboard
- SDK 只通过 `RuntimeEvent`、`tracing`、`metrics` 暴露统一观测信号；采集、存储、告警、展示由宿主应用负责
- 不为了 dashboard 便利而把高基数字段塞进默认 metric label

## v0.1 明确不做（留给 v0.2+）

| 功能 | 原因 |
|------|------|
| MCP server 集成 | 独立功能模块，v0.1 先验证核心架构 |
| Tool Search Tool（渐进式 tool 加载） | tool 数量不是 v0.1 的实际瓶颈 |
| Context compaction | 超长 session 是 v0.2 才面对的问题 |
| 多 model adapter（OpenAI 等） | 一个 adapter 足以验证架构 |
| Multi-agent 协作 | sub-agent 的 budget 继承和事件嵌套是独立复杂度 |
| Skill 沙箱（firejail/bubblewrap） | v0.3 完成 ScriptExecutor 抽象和 capability 声明；实际进程隔离留后续 |
| 并行 tool call | 顺序执行保持审批门简单，并行是 v0.2 优化项 |
| Webhook 模式异步 tool | polling 模式先验证，webhook 是补充 |
| Persistent script mode | 只在冷启动成为实测瓶颈后才值得做 |

## v0.3 解决的生产化问题

- **Skill 依赖管理**：skill 的 Python/Node 脚本通过 SKILL.md frontmatter 声明依赖，runtime 准备按 skill 隔离的缓存环境
- **Code Execution as MCP**：启用 `AgentConfig.code_execution_enabled` 后，runtime 提供内置 `execute_python` / `execute_javascript` tool
- **Skill sub-agent**：runtime 提供 sub-agent budget 继承、深度限制和生命周期事件；多 agent 协作编排仍不属于 SDK core

## 无沙箱环境的最低运营建议

v0.1–v0.3 不做进程级沙箱，在此期间建议遵守以下约束以降低误用风险：

- **只加载来自受信目录的 skill**：通过 `AgentConfig.skills_dir` 指向受你控制的路径；不自动加载来自网络或未知来源的 skill
- **含 `scripts/` 的 skill 须手动审核**：bundled script 会以当前进程权限执行，审核方式与审查第三方 shell 脚本相同
- **`side_effect: true` 的 tool 默认开启 `requires_approval`**：这是 v0.1 就支持的控制手段，对 skill bundled tool 同样适用，不应跳过
- **通过 `capabilities.env` 收缩环境变量暴露**：v0.3 起 `ExecutionContext` 只向子进程传递 skill 声明的变量，不继承完整父进程环境；对包含密钥的进程尤为重要
- **审计 `SkillContentRead` 和 `ToolCallStarted` 事件**：所有 skill 文件读取和 tool 调用都应通过 `RuntimeEvent` 可见，并可按 [observability.md](./observability.md) 接入 `tracing` / `metrics` 体系检测异常访问模式
