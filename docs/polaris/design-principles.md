# 设计原则

## 核心定位

给真正想做 agent 应用的开发者一个**轻量、透明、AI 时代友好、对齐开放标准的底层 runtime**。

这个 runtime 的价值不在于功能多，而在于几个核心约束：
- 严格区分 tool / MCP / skill，不混淆抽象层次
- 对齐 Anthropic Agent Skills 开放标准，与社区生态互通
- 让 agent 的每一步推理可见（包括 token 级流式输出）
- 让用户的 Python/TS 代码能直接调用，不需要管理额外进程
- 原生处理长时异步 tool，不把等待成本转嫁给用户代码
- 让 Rust 的严格性帮 AI 写出更可靠的实现

## 渐进式披露

借鉴 Anthropic Agent Skills 的三阶段设计，扩展到整个 runtime。

**Skill 的三层渐进披露：**

1. **Discovery（启动时）**：所有已注册 skill 的 `name` + `description` 进入 system prompt，每个 skill 约 80 token footprint，可同时注册数百个 skill 而不爆 context
2. **Activation（按需）**：当 agent 判断某个 skill 相关时，通过 file read tool 读取完整 SKILL.md（typical 500-2000 tokens）
3. **Execution（深入）**：如果任务需要更详细的信息，agent 进一步读取 `references/` 文件，或调用 `scripts/` 里的脚本

**Tool 的渐进披露（v0.2 考虑）：** Anthropic 已推出 Tool Search Tool，把渐进披露应用到 tool 本身。v0.1 不实现，但 tool registry 设计要为后续扩展留口。

## 极简 Core

Runtime 只做"循环 + 状态管理 + 事件流"，所有能力外移到 tool 和 skill。不在 core 里内置复杂工作流引擎，不提供预制 agent 模板。

## 可观测优先

每一步发出事件，包括 token 级流式输出。事件流是一等公民，不是调试附加。用户消费事件做日志、UI、审计——不应该有"黑盒"步骤。

## AI 时代友好

设计假设是 AI 写大部分代码。因此偏好：
- 显式约束而非隐式惯例
- 强类型而非 stringly-typed 接口
- 清晰的反馈回路（类型错误 > 运行时错误 > 文档说明）
- 不做过度抽象，三行类似代码比一个仓促抽象更好

## Anthropic 开放标准对齐

SKILL.md 格式与 Anthropic Agent Skills 开放标准（agentskills.io）兼容：
- 用户写的 skill 可以同时在本 runtime 和 Claude Code、Claude.ai 中使用
- Anthropic 官方开源 skill 可以直接 drop-in 使用
- skill 作者只需要学一套规范

不兼容的部分（需明确告知）：
- Anthropic 某些 skill 假设有 code execution 环境（Python REPL），本 runtime 通过 spawn 子进程模拟
- bundled scripts 的 schema 声明需要扩展到 SKILL.md frontmatter（Anthropic 的脚本是约定式的，没有 schema 声明）

## 决策时的参考问题

遇到设计分叉时，用以下问题校验方向是否跑偏：

1. 这个功能是"循环 + 状态管理 + 事件流"的一部分，还是应该在 tool/skill 层解决？
2. 这个抽象让 agent 的行为更可见，还是更隐藏？
3. 这个设计偏离了 Anthropic Agent Skills 开放标准吗？如果是，代价是什么？
4. 一个不熟悉这个 runtime 的 AI 能否通过强类型和错误信息自己推断出正确用法？
