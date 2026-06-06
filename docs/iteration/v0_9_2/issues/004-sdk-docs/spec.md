# 004 · SDK 文档 — Spec

## 背景

Orchest 的核心卖点之一是 Python / TypeScript 双语言 SDK（PyO3 / napi-rs 绑定）。README 有最短片段，`examples/python/` 和 `examples/typescript/` 有可运行示例，但缺面向 SDK 用户的连贯指南。本 issue 补 `docs/guide/sdk-python.md` 和 `docs/guide/sdk-typescript.md`。

## 目标

新增两份 SDK 指南，覆盖安装、基础用法、tool 注册、事件消费，代码与现有 `examples/python/` / `examples/typescript/` 一致。

## 范围

### `docs/guide/sdk-python.md`

1. **安装**：`pip install maturin` → `maturin develop`（开发模式）；附注 PyPI 发布计划于 v1.0
2. **基础用法**：`Agent(model=..., system_prompt=..., api_key_env="ANTHROPIC_API_KEY")` 构造；`run(input)` / `run_sync(input)` 返回 `list[RuntimeEvent]`
3. **Tool 注册**：`@agent.tool` 装饰器（推荐）和 `agent.register_tool(func, ...)`；docstring 作为 description；`approval` 参数（`"never"`/`"when_risky"`/`"always"`）
4. **事件消费**：遍历返回的 event list，`event["type"]` 分派；给出 `model_stream_chunk` / `tool_call_started` / `tool_call_completed` / `run_completed` / `run_failed` 的处理示例
5. **event type 速查表**：列出全部 event type 字符串
6. **异常**：`AgentError` 及子类（`BudgetExceededError` / `ApprovalDeniedError` / `ModelError` / `ToolError` / `SkillError`）
7. **类型提示**：指向 `python/agent_runtime/__init__.pyi`

### `docs/guide/sdk-typescript.md`

1. **安装**：`npm install && npm run build:native`（`cargo build -p agent-runtime-node` + copy addon）；包名 `@orchest/agent-runtime`；Node ≥ 18；附注 npm 发布计划于 v1.0
2. **基础用法**：`new Agent({ model, systemPrompt, apiKeyEnv })`；`runSync(input)` 返回 `RuntimeEvent[]`
3. **Tool 注册**：`registerTool({ name, description, inputSchema })`（schema-only）和 `registerToolWithHandler(name, desc, schema, handler, opts)`（带执行函数）；`approval`（`"never"`/`"whenRisky"`/`"always"`）
4. **事件消费**：`for (const event of events)` + `switch (event.type)`
5. **event type 速查表**
6. **类型定义**：指向 `js/index.d.ts` / `js/native.d.ts`

### README 链接

在 `README.md` 的 Documentation 表中新增两行，分别链接到 `docs/guide/sdk-python.md` 和 `docs/guide/sdk-typescript.md`。

## 约束

- 代码片段与 `examples/python/basic.py`、`examples/typescript/basic.ts` 一致
- 与 README 的最短片段不冲突：README 给"一眼能跑"的片段，SDK 文档给完整可运行 + 逐项说明
- 不重复 Rust quickstart（issue 003）的概念讲解，必要处链接过去
- 事件 type 字符串以 `__init__.pyi` / `index.d.ts` 中的列表为准（两者略有差异，分别按各自 SDK 实际导出列）

## 验收标准

- [ ] `docs/guide/sdk-python.md` 存在，覆盖 7 个小节
- [ ] `docs/guide/sdk-typescript.md` 存在，覆盖 6 个小节
- [ ] Python 代码片段与 `examples/python/basic.py` 一致（装饰器注册 + 事件遍历）
- [ ] TS 代码片段与 `examples/typescript/basic.ts` 一致（schema 注册 + switch 事件）
- [ ] 两份文档的 event type 速查表分别与 `__init__.pyi` / `index.d.ts` 的导出一致
- [ ] 安装小节注明 PyPI / npm 正式发布计划于 v1.0
- [ ] `README.md` Documentation 表新增 sdk-python / sdk-typescript 两行链接
- [ ] 无失效内部链接（指向的 .pyi / .d.ts / examples 路径存在）

## Notes

- Python tool 装饰器把函数 docstring 当 description，参数名/类型推断 input schema —— 文档要点明这一隐式契约。
- 异步 tool（Python `{"async_job": {...}}` 返回形状）属于进阶，本指南一句话带过并链接 `examples/python/async_tool.py`，不展开。
- TS 侧 `run`（async）在 `native.d.ts` 未必导出，当前以 `runSync` 为准；若实际有 async `run` 再补。实现时以 `js/native.d.ts` 真实签名为准。
- Python 的 `run` 与 `run_sync` 当前都返回 `list[RuntimeEvent]`（非流式迭代器）——文档不要写成 generator，按实际返回类型描述。
