# 006 · `orchest-tools` 三语言 binding

## 背景

`orchest-tools` Rust crate 在 issue 005 中完成后，需要通过 PyO3 与 napi-rs 暴露到 Python / Node，作为"基础扩展统一包"的完整发布形态。

这是仓内第一条 **非 core crate → 三语言 binding** 的参考路径，后续追加新基础扩展时复用相同模式。

## 目标

- 新建 `crates/orchest-tools-py/`（PyO3 binding，PyPI 包名 `orchest-tools`）
- 新建 `crates/orchest-tools-node/`（napi-rs binding，npm 包名 `@orchest/tools`）
- 让 Python / Node 用户在已经安装了 `agent-runtime-py` / `@orchest/runtime` 的项目中，通过简单的 import + 注册调用 `WebFetchTool` 与 `WebSearchTool`

## 验收标准

### Python binding

- [ ] `crates/orchest-tools-py/Cargo.toml`：workspace member，依赖 `pyo3`、`orchest-tools`、`agent-runtime-core`
- [ ] `crates/orchest-tools-py/pyproject.toml`：使用 `maturin` 构建，PyPI 包名 `orchest-tools`
- [ ] Python API（最小可用形态）：
  ```python
  from orchest_tools import WebFetchTool, WebSearchTool, register_all
  from agent_runtime_py import Agent

  agent = Agent(model="...")
  # 单个 tool 注册：
  agent.register_tool(WebFetchTool())
  # 或一键注册全部：
  register_all(agent)
  ```
- [ ] `WebFetchTool` / `WebSearchTool` 暴露为 Python 类，构造时接受可选配置（timeout、max_results 等）
- [ ] 错误以 Python 异常形式抛出（不是 Rust panic 透传），异常类型至少包含：`OrchestToolError`、`OrchestNetworkError`、`OrchestTimeoutError`
- [ ] `python examples/python_basic.py` 模式的最小示例存在于 `examples/python_with_orchest_tools.py`，能跑通

### Node binding

- [ ] `crates/orchest-tools-node/Cargo.toml`：workspace member，依赖 `napi`、`napi-derive`、`orchest-tools`
- [ ] `crates/orchest-tools-node/package.json`：npm 包名 `@orchest/tools`，napi-rs 标准构建配置
- [ ] TypeScript API（最小可用形态）：
  ```ts
  import { Agent } from "@orchest/runtime";
  import { WebFetchTool, WebSearchTool, registerAll } from "@orchest/tools";

  const agent = new Agent({ model: "..." });
  agent.registerTool(new WebFetchTool());
  // 或：
  registerAll(agent);
  ```
- [ ] 类型定义（`.d.ts`）由 napi-rs 自动生成；至少覆盖 tool 类、配置类型、错误类型
- [ ] 错误以 JS Error 子类抛出，名称与 Python 异常对齐
- [ ] `examples/ts_with_orchest_tools.ts` 最小示例存在且能跑通

### 一致性约束

- [ ] Python 类名 / 方法名 / 配置字段名 → 三语言对齐（snake_case 与 camelCase 各自符合语言约定，但语义对应清晰）
- [ ] 同一 input 在三语言下行为一致；同一错误在三语言下错误类型可一一映射
- [ ] 三语言 binding 共享一份 input schema（来自 `orchest-tools` crate 中 tool 的 schema 定义）

### 构建与 CI

- [ ] CI 增加 step：`cd crates/orchest-tools-py && maturin build --release` 成功
- [ ] CI 增加 step：`cd crates/orchest-tools-node && npm run build` 成功（napi-rs 标准构建）
- [ ] CI 不要求实际发包；只验证构建产物存在

### 文档

- [ ] `docs/sdk/authoring-tools.md` 增加一节"使用基础扩展包"，引用 Python / Node 安装与使用方法
- [ ] `crates/orchest-tools-py/README.md` 和 `crates/orchest-tools-node/README.md` 各自有最小 quickstart

## 注意

- **不要**在 binding 层增加 Rust 没有的能力。binding 是 1:1 暴露 + 类型转换，没有业务决策（参见 AGENTS.md "Rule: All business logic lives in agent-runtime-core"）
- 不要在 binding 里塞额外的 tool；新 tool 增加在 `orchest-tools` Rust crate 中即可，binding 不需要改动（除非有新类型需要转换）
- napi-rs 与 PyO3 都不支持把 `async fn` 直接暴露——需要在 binding 里用 `tokio::runtime` 或对应桥接；这部分参考 `agent-runtime-py` / `agent-runtime-node` 已有写法
- PyPI 包名 `orchest-tools` 与 Rust crate 同名；npm 包名 `@orchest/tools`。如果 PyPI / npm 名字已被占用，先与维护者沟通命名调整
