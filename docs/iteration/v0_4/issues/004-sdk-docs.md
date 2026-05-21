# 004 · SDK 文档骨架与 getting-started

## 背景

仓内现有的 `docs/` 是设计文档（PRD、polaris、研究）。外部使用者需要的是"怎么用"的文档。这次新增 `docs/sdk/`，面向 SDK 使用者，与设计文档物理隔离。

## 目标

建立 `docs/sdk/` 目录骨架，写齐所有计划文档的 **大纲 + 最小可运行 hello world**。本 issue 不要求每篇都写满，但 `getting-started.md` 必须完整可跑通。

## 验收标准

### 目录骨架

- [ ] 新建 `docs/sdk/` 目录
- [ ] 创建以下文件并写入至少 2 级标题大纲：
  - `getting-started.md`
  - `authoring-tools.md`
  - `authoring-skills.md`
  - `mcp-integration.md`
  - `code-execution.md`
  - `sub-agent.md`
  - `api-reference.md`
  - `extension-promotion-criteria.md`（由 issue 001 产出，本 issue 仅占位）
- [ ] 在 `docs/sdk/README.md` 或 index 中列出目录树和每篇文档目的

### getting-started.md 完整内容

- [ ] 三语言各一节（Rust / Python / TS），每节包含：
  - 安装命令（cargo add / pip install / npm install）
  - 创建 agent
  - 注册一个 in-process tool
  - 发送一条 user message
  - 处理 event 流并打印结果
  - 完整可运行代码（最多 50 行）
- [ ] 每节代码与 `playground/` 或 `examples/` 中实际可跑的版本对应；脚注链接对应文件
- [ ] 末尾"Next Steps"链接到其它 `docs/sdk/*.md`

### 其它文档大纲

每篇 `*.md` 至少包含以下小节占位（内容可留 `TODO`，由后续 issue 或独立 PR 填充）：

- `authoring-tools.md`：tool trait、参数 schema、async vs sync、approval、错误处理、跨语言差异
- `authoring-skills.md`：SKILL.md frontmatter、capabilities、bundled tools、dependencies、目录约定
- `mcp-integration.md`：stdio transport、HTTP transport、tool discovery、与 in-process tool 的差异
- `code-execution.md`：内置 code exec MCP、超时、安全性已知限制
- `sub-agent.md`：sub-agent 启动、budget 继承、事件嵌套、最大深度
- `api-reference.md`：核心 trait / 类型表（指向源码与 issue 001 inventory）

### 不引入文档站点构建

- [ ] **不**引入 mdBook、docusaurus、mkdocs 等；本迭代以 markdown 文件为终态
- [ ] 文档链接用相对路径，能在 GitHub 上渲染正确即可

## 注意

- 文档示例必须真实可跑，禁止伪代码（`// 略`、`...`、`TODO 实现这里` 等用于"省略"的占位禁止出现在示例代码块中）
- 若写文档过程中发现现有 API 不易使用（比如签名繁琐、命名歧义），不要在文档里绕过——开一个新 issue 记录 API 改进，本 issue 只暴露问题不修问题
- 任何 `getting-started` 示例引用的代码段都应有对应的 `examples/` 或 `playground/` 文件可点击进入
