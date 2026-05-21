# 001 · 能力清单与扩展晋升标准

## 背景

v0.1–v0.3 把 runtime 规划能力全部落地，但代码层只记在各 crate 的源码里，没有面向使用者的"目录"。v0.4 的第一步是把已实现能力做一次清点，并把"什么算 core / 什么算扩展"的判断标准写下来，作为后续 issue 拆解和文档结构的锚点。

## 目标

产出两份文档：

1. `docs/iteration/v0_4/inventory.md`：v0.1–v0.3 已实现能力清单（tool / model adapter / skill 基础设施 / 事件类型 / 配置项），每项标注 core / extension / out-of-scope 归属
2. `docs/sdk/extension-promotion-criteria.md`：四类（core / 基础扩展 / 文件型 skill / 语言原生 tool）的选择标准。**本 issue 同时建立 `docs/sdk/` 目录**（issue 004 之后会在该目录下新增其余 SDK 文档）

## 验收标准

### inventory.md

- [ ] 按 v0.1 / v0.2 / v0.3 三个段落组织，每段列出该迭代新增的能力项
- [ ] 每个能力项至少给出：名称、所在 crate / 模块路径、对外公开符号、归属（core / extension / out-of-scope）
- [ ] 覆盖以下范畴（按 `crates/agent-runtime-core/src/` 现状逐项核对，确认每一项的实际公开符号 / 模块路径）：
  - 内置 tool：`read_file`（`tool/builtin.rs`）、Tool Search Tool（`tool/search.rs`）、Code Execution MCP（`tool/code_exec.rs`）
  - 协议层：MCP stdio / Streamable HTTP transport（`tool/mcp.rs`）
  - 异步 tool：polling 与 webhook 两种模式（`tool/async_job.rs`）
  - Model adapter：Anthropic（`model/anthropic.rs`）、OpenAI（`model/openai.rs`）
  - Run loop：streaming、approval gate、budget、context compaction（`run.rs`、`budget.rs`）
  - 事件类型：`RuntimeEvent` 全部 variant（按 `events.rs` 逐项列出，**禁止臆造未存在的 variant 名**，例如 `ToolCallDenied`、`BudgetExceeded`、`ToolRegistered` 在当前代码中不存在）
  - 嵌入式 SDK：v0.3 在 `skill/mod.rs` 中自动安装的 Python `orchest_sdk` 与 Node `orchest-sdk` 包（提供 `create_sub_agent` 入口），属于 core，但归类时单列"嵌入式 SDK"段落，避免与"基础扩展包 orchest-tools"混淆
  - Skill 基础设施（**注意区分**）：
    - Skill **发现与加载**（`SkillScanner`、`SkillManifest` 解析、`SkillEnvManager` 依赖准备）—— core，agent 初始化路径
    - Skill 内 **bundled 脚本执行**（`ScriptExecutor`、`BareSubprocessExecutor`、bundled tool 协议）—— core，运行时路径
    - `CapabilityValidator` 与 `ExecutionContext.env` 构造 —— core
    - **"Skill 本身"是数据**（markdown + 脚本），不是 core 也不是 extension 的 runtime 代码；其归属取决于 skill 内容来源（仓内示例 / 用户自有 / 扩展包附带）
  - Sub-agent：协议、budget 继承、事件嵌套
  - 事件类型：`RuntimeEvent` 全部 variant
- [ ] 对每项 core / extension 判断给出一句话理由（不能只是标签）
- [ ] 末尾附一节"已知缺口"，列出文档化时发现的未实现 / 半实现项。**本 issue 仅做记录,不在 v0.4 内修复**；明显 bug 单独开 issue 跟进，其余作为 v0.5 候选

### extension-promotion-criteria.md

- [ ] 四类边界各一节：Core / 基础扩展（`orchest-tools`）/ 文件型 skill / 语言原生 tool
- [ ] 每类至少包含：定义、判断标准（≥3 条 yes/no 问题）、典型例子、反例
- [ ] 给出一张"决策树"流程图（ASCII / mermaid 任一）：开发者面对一个新能力想法时，如何决定它归哪一类
- [ ] 明确说明：基础扩展统一打包在 `orchest-tools` 一个 crate 内，不再每个 tool 一个 crate
- [ ] 明确以下不变量：
  - **Skill 发现 / 加载**（`SkillScanner`、SKILL.md 解析、capability 校验、依赖准备）—— 永远属于 core
  - **Skill 内 bundled 脚本执行**（`ScriptExecutor` 抽象与默认实现）—— 永远属于 core
  - **Skill 自身**是数据：仓内示例 skill 放 `skills/`；用户自定义 skill 由用户管理；扩展包可以附带 skill 但 skill 本身不通过扩展 crate 的 binding 跨语言传递（runtime 直接读文件）

## 注意

- inventory 的最小信息单位是"对使用者可见的能力"，不是"源码文件"。同一个能力涉及多个文件时合并成一项
- 这份 inventory 也是 SDK 文档（issue 004）的目录索引，需要让后续 SDK 文档作者可以直接据此分章节
- 不要在本 issue 里讨论"未来要不要把 X 重新归类"——本 issue 只做现状盘点 + 标准制定，后续重分类是独立动作
