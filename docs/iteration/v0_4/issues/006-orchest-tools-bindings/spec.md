# 006 · `orchest-tools` 三语言 binding

## 背景

`orchest-tools` Rust crate 在 issue 005 中完成后，需要通过 PyO3 与 napi-rs 暴露到 Python / Node，作为"基础扩展统一包"的完整发布形态。

这是仓内第一条 **非 core crate → 三语言 binding** 的参考路径，后续追加新基础扩展时复用相同模式。

## 现状与 API 缺口（必读）

当前 binding 层的实际形态（核对 `crates/agent-runtime-py/src/lib.rs`、`crates/agent-runtime-node/src/lib.rs`、`python/agent_runtime/__init__.py`、`js/index.ts`）：

- **Python 公开包**：`agent_runtime`（在 `python/agent_runtime/__init__.py` 中重导出 `agent_runtime_py.Agent`）。用户写 `from agent_runtime import Agent`，**不**是 `from agent_runtime_py import Agent`
- **Python tool 注册 API**：当前只有 `@agent.tool` 装饰器（包装 Python 函数）。**没有** `agent.register_tool(<class instance>)` 方法用于注册 Rust 来源的 tool
- **Node tool 注册 API**：`agent.registerTool({ name, description, inputSchema, requiresApproval, sideEffect })` —— 当前签名接受一个 options dict，**不**接受类实例
- **包名占用**：`agent-runtime-py` PyPI 名已用，wrapper `agent_runtime` 是其上层 Python 包；npm 侧尚未有 `package.json`

因此 issue 006 的工作不止"暴露 tool"，**还要在 `agent-runtime-py` / `agent-runtime-node` 这两个 core binding 中新增"原生 tool 注册"入口**，让 `orchest-tools` 这种 Rust 来源的 tool 能被注册。这是 binding 层的纯接口增加，不动核心业务逻辑，仍符合 AGENTS.md "binding crates only do type conversion and FFI glue"。

## 目标

- 新建 `crates/orchest-tools-py/`（PyO3 binding，PyPI 包名 `orchest-tools`，Python 模块名 `orchest_tools`）
- 新建 `crates/orchest-tools-node/`（napi-rs binding，npm 包名 `@orchest/tools`）
- 在 `agent-runtime-py` / `agent-runtime-node` 中各新增一个公共 API（如 `Agent.register_native_tool(handle)` / `Agent.registerNativeTool(handle)`），接受由 `orchest-tools` binding 构造的 tool handle
- 让 Python / Node 用户能用统一方式注册 `orchest-tools` 中的 tool

## 验收标准

### 核 binding 接口增强（先决条件）

- [ ] `agent-runtime-py` 中新增公开方法：`Agent.register_native_tool(handle: PyAny) -> None`，handle 内部携带一个已注册到 `ToolRegistry` 的 `Arc<dyn Tool>`（通过 PyCapsule 或 PyO3 共享对象传递）
- [ ] `agent-runtime-node` 中新增对应 `Agent.registerNativeTool(handle: NativeToolHandle): void`
- [ ] 上述 API 在 `agent-runtime-py` / `agent-runtime-node` 各自的单元测试中至少有一条 happy-path 测试（注册一个 dummy Rust tool 并被 mock model 调用）
- [ ] **不在** core binding 中暴露 `orchest-tools` 的具体类型；只暴露通用 handle 入口

### Python binding (`crates/orchest-tools-py/`)

- [ ] `Cargo.toml`：workspace member，依赖 `pyo3`（`extension-module`）、`orchest-tools`、`agent-runtime-core`
- [ ] `pyproject.toml`：使用 `maturin` 构建，PyPI 包名 `orchest-tools`，Python 模块名 `orchest_tools`（pyproject 中用 `module-name = "orchest_tools"`）
- [ ] Python API 最小可用形态（必须**字面**对应 `examples/python/with_orchest_tools.py` 中可运行的代码）：
  ```python
  from agent_runtime import Agent
  from orchest_tools import WebFetchTool, WebSearchTool, register_all

  agent = Agent(model="anthropic/claude-sonnet-4-20250514")
  agent.register_native_tool(WebFetchTool().handle())
  # 或一键注册所有：
  register_all(agent)
  ```
- [ ] `WebFetchTool` / `WebSearchTool` 暴露为 Python 类，构造时接受配置（`WebFetchTool(timeout_ms=10000, max_body_bytes=524288)` / `WebSearchTool(max_results=5)`）
- [ ] `tool.handle()` 返回一个不透明对象，可被 `Agent.register_native_tool()` 接受
- [ ] `register_all(agent)` 一次注册全部 `orchest-tools` 工具
- [ ] 错误以 Python 异常形式抛出（不是 Rust panic 透传），异常基类 `OrchestToolError`，子类至少 `OrchestNetworkError`、`OrchestTimeoutError`
- [ ] `examples/python/with_orchest_tools.py` 存在，且在 issue 002 创建的 CI workflow 中以 `python examples/python/with_orchest_tools.py` step 跑通（mock provider，exit code 0）

### Node binding (`crates/orchest-tools-node/`)

- [ ] `Cargo.toml`：workspace member，依赖 `napi`、`napi-derive`、`orchest-tools`
- [ ] `package.json`：npm 包名 `@orchest/tools`，napi-rs 标准构建配置（`napi build` 输出 `.node`）
- [ ] TypeScript API 最小可用形态（**字面**对应 `examples/typescript/with_orchest_tools.ts`）：
  ```ts
  import { Agent } from "@orchest/runtime";
  import { WebFetchTool, WebSearchTool, registerAll } from "@orchest/tools";

  const agent = new Agent({ model: "anthropic/claude-sonnet-4-20250514", systemPrompt: "..." });
  agent.registerNativeTool(new WebFetchTool({ timeoutMs: 10000 }).handle());
  // 或一键注册：
  registerAll(agent);
  ```
- [ ] `.d.ts` 由 napi-rs 自动生成，包含 `WebFetchTool`、`WebSearchTool`、`registerAll`、tool 构造配置类型、错误类型
- [ ] 错误以 JS Error 子类抛出（`OrchestToolError`、`OrchestNetworkError`、`OrchestTimeoutError`），名称与 Python 端 1:1 对应
- [ ] `examples/typescript/with_orchest_tools.ts` 存在，且在 CI workflow 中以 `tsx examples/typescript/with_orchest_tools.ts`（或等价命令）跑通

### 一致性约束

- [ ] 配置字段名规则：Rust crate 中以 snake_case 命名（`timeout_ms`、`max_results`），Python binding 沿用 snake_case，Node binding 转 camelCase（`timeoutMs`、`maxResults`）。三语言之间存在严格 1:1 映射，由 binding 层做名称转换
- [ ] 同一 input 在三语言下行为一致：编写一组共享 fixture（如 `crates/orchest-tools/tests/fixtures/`），三语言各自跑一遍，断言输出 JSON 一致（除了语言原生序列化差异）
- [ ] 三语言 binding 的 tool input schema **来自 `orchest-tools` crate 中 tool 定义的同一份 `JsonSchema`**；在 binding 中**不再重写 schema**。验证：Python `tool.input_schema()` 与 Node `tool.inputSchema()` 与 Rust `tool.input_schema()` 三者反序列化后 `serde_json::Value` 完全相等

### 构建与发包策略

- [ ] CI 在 issue 002 创建的 workflow 中追加 step：
  - `maturin build --release -m crates/orchest-tools-py/Cargo.toml`，验证 `.whl` 产物存在
  - `cd crates/orchest-tools-node && npm install && npm run build`，验证 `.node` 产物存在
- [ ] **v0.4 不做正式发包**。明确写在 `crates/orchest-tools-py/README.md` 与 `crates/orchest-tools-node/README.md` 中："本包尚未发布到 PyPI / npm；当前从源码构建。正式发包计划在 v0.5"
- [ ] 仍然在 CI 中跑一次"模拟安装"：`pip install crates/orchest-tools-py/target/wheels/*.whl` 与 `npm pack crates/orchest-tools-node && npm install ./orchest-tools-*.tgz` —— 验证产物可以装

### 文档

- [ ] `docs/sdk/authoring-tools.md`（由 issue 004 创建）新增一节"使用基础扩展包"，写出 `pip install` / `npm install` + import + 注册的完整代码片段（字面对应 examples/ 中文件）
- [ ] `crates/orchest-tools-py/README.md` 与 `crates/orchest-tools-node/README.md` 各自有 ≤ 50 行的 quickstart
- [ ] examples/ 所有权约定：本 issue 负责 `examples/python/with_orchest_tools.py` 与 `examples/typescript/with_orchest_tools.ts`（专门演示基础扩展用法）；issue 004 负责 `examples/python/basic.py` 等通用 getting-started 示例的核对，两者引用边界写入各自 README

## 注意

- **不要**在 binding 层增加 Rust 没有的能力。binding 是 1:1 暴露 + 类型转换，没有业务决策（参见 AGENTS.md "Rule: All business logic lives in agent-runtime-core"）
- "在 core binding 中新增 register_native_tool 入口"不属于业务决策，是接口扩张；改动局限于 PyO3 / napi 类型映射代码
- 不要在 binding 里塞额外的 tool；新 tool 增加在 `orchest-tools` Rust crate 中即可，binding 不需要改动（除非有新类型需要转换）
- napi-rs 与 PyO3 都不支持把 `async fn` 直接暴露——需要在 binding 里用 `tokio::runtime` 或对应桥接；参考 `agent-runtime-py` / `agent-runtime-node` 已有写法
- PyPI 名 `orchest-tools`、npm 名 `@orchest/tools` —— 在正式 publish 前确认未被占用；如被占用，开新 issue 讨论命名
