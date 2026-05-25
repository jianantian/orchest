# 004 · SDK 文档骨架与 getting-started

## 背景

仓内现有的 `docs/` 是设计文档（PRD、polaris、研究）。外部使用者需要的是"怎么用"的文档。这次新增 `docs/sdk/`，面向 SDK 使用者，与设计文档物理隔离。

## 目标

建立 `docs/sdk/` 目录骨架，写齐所有计划文档的 **大纲 + 最小可运行 hello world**。本 issue 不要求每篇都写满，但 `getting-started.md` 必须完整可跑通。

## 验收标准

### 目录骨架

- [ ] `docs/sdk/` 目录**由 issue 001 创建**（issue 001 把 `extension-promotion-criteria.md` 直接写入该目录）；本 issue 在此目录下新增其余文档
- [ ] 创建以下文件并写入至少 2 级标题大纲：
  - `getting-started.md`
  - `authoring-tools.md`
  - `authoring-skills.md`
  - `mcp-integration.md`
  - `code-execution.md`
  - `sub-agent.md`
  - `api-reference.md`
- [ ] `extension-promotion-criteria.md` 已由 issue 001 产出，本 issue 仅在 `docs/sdk/README.md`（或 index）中引用
- [ ] 在 `docs/sdk/README.md` 中列出目录树（包含 001 产出的 `extension-promotion-criteria.md`）和每篇文档目的

### getting-started.md 完整内容

- [ ] 三语言各一节（Rust / Python / TS），每节包含：
  - 安装命令（实际可执行的 `cargo add` / `pip install` / `npm install`）
  - 创建 agent
  - 注册一个 in-process tool（注意：Python 当前 API 是 `@agent.tool` 装饰器；TS 当前 API 是 `agent.registerTool({ name, description, inputSchema, ... })`）
  - 发送一条 user message
  - 处理 event 流并打印结果
  - 完整可运行代码片段
- [ ] 每节代码块**逐字**对应 `examples/` 下一个真实文件（如 Python 节对应 `examples/python/python_basic.py`，TS 节对应 `examples/typescript/ts_basic.ts`，Rust 节对应 `examples/rust/rust_provider_runtime_deepseek.rs` 或类似）；用脚注 / 链接指明对应文件路径
- [ ] CI 在 issue 002 创建的 workflow 中追加 step：执行每节对应的 `examples/` 文件，exit code 0 视为通过
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

### 禁伪代码验收

- [ ] `docs/sdk/` 下所有 markdown 文件的代码块（rust / python / ts / bash）通过 grep 检查：不包含 `// 略`、`# 略`、`# TODO 实现`、`<placeholder>`、`<your-...>`、独立成行的 `...` 这类省略符
- [ ] 添加一个 `scripts/check-sdk-docs-pseudocode.sh` 脚本执行上述检查（grep 命令组合），并在 issue 002 创建的 workflow 中追加 step 调用该脚本

## 注意

- 文档示例必须真实可跑，禁止伪代码（`// 略`、`...`、`TODO 实现这里` 等用于"省略"的占位禁止出现在示例代码块中）
- 若写文档过程中发现现有 API 不易使用（比如签名繁琐、命名歧义），不要在文档里绕过——开一个新 issue 记录 API 改进，本 issue 只暴露问题不修问题
- 任何 `getting-started` 示例引用的代码段都应有对应的 `examples/` 或 `playground/` 文件可点击进入
