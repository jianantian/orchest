# v0.4 PRD：验收、文档与扩展骨架

## 前置依赖：Hotfix 2026-05

main 上已有 `docs/hotfix/2026_05/` 一份 contract repair 计划，覆盖 `allowed_tools` / `allowed_skills` 枢纽、`ToolMetadata.timeout` / `max_output_tokens` enforcement、SDK 入口接通 skill loading、OpenAI tool-call 历史映射、sub-agent approval 路由、MCP HTTP 重试安全性等。

v0.4 验收场景会直接撞上其中多处问题：

- issue 002 / 003 中的 budget、approval、timeout 路径依赖 Hotfix 002
- issue 003 v0.2 scenario 的 OpenAI 切换依赖 Hotfix 005
- issue 003 v0.2 scenario 的 MCP 集成依赖 Hotfix 007
- issue 003 v0.3 scenario 的 sub-agent / skill 入口依赖 Hotfix 003 / 006
- issue 005 / 006 中 `WebFetchTool` 的 timeout 行为依赖 Hotfix 002
- issue 007 的 skill 加载依赖 Hotfix 003

**因此 v0.4 issue 002 起的实施工作建议在 Hotfix 2026-05 完成后启动**；v0.4 issue 001（能力清单 + 晋升标准 + 建 `docs/sdk/`）可与 Hotfix 并行，因为它仅做文档盘点不依赖运行时行为。

## 目标

v0.1–v0.3 的 runtime 规划能力已全部实现。v0.4 不引入新的 runtime 特性，聚焦把已有能力沉淀为可对外发布的形态：

1. **验收**：让 v0.1 / v0.2 / v0.3 的能力被一条真实代码路径串起来跑通，给后续改动留下回归基线
2. **文档**：写出面向 SDK 使用者的开发者文档（不是设计文档），让外部开发者可以入门
3. **扩展骨架**：把"核心 / 扩展"边界落到物理结构上，搭出三语言扩展的第一条参考路径

v0.4 结束时，开发者应该能够：
1. 通过一个 Rust CLI playground 二进制，分别跑通 v0.1、v0.2、v0.3 的端到端 scenario
2. 在 `docs/sdk/` 阅读到三语言的 getting started、tool / skill 作者指南、MCP / code-exec / sub-agent 使用说明
3. 通过本地构建产物安装 `orchest-tools`（Python wheel / npm tarball），获得统一的基础扩展工具包（含 `WebFetchTool` 等）。**v0.4 不做正式 PyPI / npm 发包**；包的可安装性在 CI 中通过本地 install 验证
4. 在 `skills/` 看到至少 1 个文件型 skill 参考实现，并能在 playground 中被加载执行
5. 阅读 `docs/sdk/extension-promotion-criteria.md` 了解什么算 core、什么留在 extension、文件型 skill 与语言原生 tool 的边界

## 成功指标

- Playground 三个 scenario 在 CI 中可执行，不依赖外部 API key（mock provider 或 fixture）
- `docs/sdk/getting-started.md` 能让一个新开发者在 30 分钟内跑通"注册自定义 tool + 与模型对话"的 hello-world
- `orchest-tools` 三语言入口能在示例工程中通过 `register_native_tool(WebFetchTool().handle())`（Python）/ `registerNativeTool(new WebFetchTool().handle())`（TS）一行注册成功（详见 issue 006）
- 至少 1 个文件型 skill 在 playground 的 v0.3 scenario 中被发现、加载、执行
- `docs/iteration/v0_4/inventory.md` 完整列出 v0.1–v0.3 已实现能力并标注 core / extension 归属

## 范围

### 能力清单与扩展晋升标准

- 编写 `docs/iteration/v0_4/inventory.md`：枚举 v0.1–v0.3 实际落地的 tool、model adapter、skill 基础设施、事件类型，标注每项的归属（core / extension / out-of-scope）
- 编写 `docs/sdk/extension-promotion-criteria.md`：明确以下三类的选择标准
  - **Core**：跨语言基础设施、runtime 自检需要、有性能或安全考量；随 `agent-runtime-core` 分发
  - **基础扩展（Rust + 三语言 binding）**：跨语言需要、非应用一次性、值得 binding 投入；统一打包在 `orchest-tools` 一个 crate 中
  - **文件型 skill**：领域知识、procedural 流程，纯 markdown + 脚本，runtime 直接读，三语言天然可用
  - **语言原生 tool**：应用一次性逻辑，用户自己写，SDK 提供 cookbook 范式

### Playground CLI

- 新建 `playground/` crate，workspace 成员，binary 名称 `orchest-playground`
- 子命令：`scenario <name>`、`repl`
- 三个 scenario：
  - `v0_1_basic_loop`：基础 run loop、event 流、tool 调用、approval gate、budget 上限触发
  - `v0_2_mcp_and_compaction`：MCP server 集成 + Tool Search Tool + context compaction + OpenAI adapter（可切换）
  - `v0_3_subagent_and_codeexec`：sub-agent 启动与 budget 继承 + code execution MCP + skill capabilities 校验
- 不依赖外部 API key：scenario 默认使用 mock provider（复用 `examples/python/mock_anthropic_provider.py` 的能力，或在 Rust 侧重写一个最小版）
- `repl` 模式让人手动验收：能交互式注册 tool、加载 skill、发消息

### SDK 文档

新建 `docs/sdk/` 目录，面向使用者（不是面向设计者）：

- `getting-started.md`：三语言 hello world（Rust / Python / TS 各一份）
- `authoring-tools.md`：in-process tool 怎么写（注册、schema、async、approval、error）
- `authoring-skills.md`：文件型 skill 的 SKILL.md 字段、capabilities 声明、bundled script 协议
- `mcp-integration.md`：接 stdio / streamable HTTP MCP server
- `code-execution.md`：code execution MCP 用法、超时控制、已知限制
- `sub-agent.md`：skill 内启动 sub-agent、budget 继承、事件嵌套
- `api-reference.md`：核心类型 / trait / 方法索引，链回源码
- `extension-promotion-criteria.md`：见上一节

文档要写真实可跑的代码片段，禁止伪代码。每段示例可在 `playground/` 或 `examples/` 找到对应可运行版本。

### 基础扩展包 `orchest-tools`

统一一个 Rust crate，配一对三语言 binding。后续新的基础扩展（shell、其他）都追加到这个 crate 内，不再每 tool 一个 crate。

- `crates/orchest-tools/`：Rust crate，v0.4 内含：
  - `WebFetchTool`：HTTP GET / POST，返回文本或 JSON
  - `WebSearchTool`：默认 DuckDuckGo HTML scrape；预留搜索后端 trait，便于换 SerpAPI / Brave
  - 预留模块边界给后续 `ShellTool`（v0.5 实现，本迭代不做）
- `crates/orchest-tools-py/`：PyO3 binding，PyPI 包名 `orchest-tools`
- `crates/orchest-tools-node/`：napi-rs binding，npm 包名 `@orchest/tools`
- 三种入口暴露同一份能力，作为后续基础扩展的参考实现

### 文件型 skill 参考实现

- `skills/code-review/`：完整 skill，含 `SKILL.md`（capabilities 声明 + procedural 内容） + `scripts/`（可选 bundled tool）
- 在 playground 的 v0.3 scenario 中被加载，验证 skill discovery、capability 校验、bundled tool 协议

## 不在范围内

- 任何 runtime 新特性（修改 run loop / event 类型 / Tool trait 的需求一律拒绝，留到 v0.5+）
- Web UI playground（只做 Rust CLI）
- 第二个基础扩展 tool（`ShellTool` 等留给 v0.5）
- 第二个文件型 skill（先把 1 个跑通，再扩量）
- skill 沙箱实际隔离（沿用 v0.3 决定）
- 多 agent 编排
- skill 版本与依赖锁定
- 文档站点构建（mdBook / docusaurus 等）；v0.4 文档以 markdown 文件为最终交付
- 正式 PyPI / npm 发包（仅做本地构建 + 安装验证；正式发包列入 v0.5）

## Issues 拆解

| Issue | 标题 |
|-------|------|
| [001](./issues/001-capability-inventory/spec.md) | 能力清单与扩展晋升标准 |
| [002](./issues/002-playground-skeleton/spec.md) | Playground crate 骨架 + v0.1 scenario |
| [003](./issues/003-playground-v0_2-v0_3-scenarios/spec.md) | Playground v0.2 / v0.3 scenarios |
| [004](./issues/004-sdk-docs/spec.md) | SDK 文档骨架与 getting-started |
| [005](./issues/005-orchest-tools-rust/spec.md) | `orchest-tools` Rust crate（含 WebFetch / WebSearch） |
| [006](./issues/006-orchest-tools-bindings/spec.md) | `orchest-tools` 三语言 binding |
| [007](./issues/007-file-skill-code-review/spec.md) | 文件型 skill：code-review 参考实现 |

## 推荐执行节奏

1. **001 先做完**：能力清单 + 扩展晋升标准 + 建立 `docs/sdk/` 目录；后续所有动作的锚点
2. **002 与 005 并行**：playground 骨架（含 CI workflow yaml、mock provider 契约）与 `orchest-tools` Rust crate 改动面不冲突，可在 worktree 中分头推进
3. **004 依赖 001**（不是仅依赖 002）：004 的 `extension-promotion-criteria.md` 引用来自 001；004 的 getting-started 代码需要 002 的 playground 已就位
4. **006 紧跟 005**：Rust 侧稳定后再封 binding；**006 还会顺带为 `agent-runtime-py` / `agent-runtime-node` 增加 `register_native_tool` 入口**（参见 issue 006"现状与 API 缺口"）
5. **003（v0.2 / v0.3 scenarios）依赖 002 + 005**：v0.3 scenario 直接消费 issue 007 的 code-review skill；但 005 的 Rust tool 已可在 Rust 层直接被 playground 使用，不必等 006
6. **007 收尾**：code-review skill 消费 005 的 `web_fetch`、被 003 的 v0.3 scenario 加载、被 004 的文档引用，是整迭代的 vertical slice

### 依赖图（DAG）

```
001 ──┬──> 002 ──┬──> 003 (v0.2 部分)
      │         └──> 004 (getting-started)
      └──> 004 (extension-promotion-criteria 引用)
      └──> 005 ──┬──> 006
                 └──> 007 ──> 003 (v0.3 部分)
```

完成标准以各 issue 的 Acceptance Criteria 为准；`spec.md` 仅作历史参考，冲突时以 `docs/iteration/` 为准。

## CI 与发包策略

- 单一 workflow 入口：`.github/workflows/v0_4-playground.yml`，由 issue 002 创建，后续 issue 在同一个文件中追加 step（避免碎片化）
- v0.4 **不**做正式 PyPI / npm 发包；构建产物的"可装"验证在 CI 中完成（`pip install <wheel>` / `npm install <tgz>`）
- 正式发包计划列入 v0.5 PRD，与"第二个基础扩展 tool"一同决策
