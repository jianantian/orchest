# 012 · Python SDK（PyO3 Binding）

## 背景

通过 PyO3 把 Rust core 暴露给 Python，提供 decorator 风格的 tool 注册和 async generator 事件流，让 Python 用户能以自然的 Python 方式使用 runtime。

## 目标

实现 `agent_runtime` Python 包，完成 `examples/python/python_basic.py` 和 `examples/python/python_async_tool.py` 两个 demo。

## 验收标准

**核心 API：**
- [ ] `Agent(model, system_prompt, skills_dir=None, budget=None)` 构造
- [ ] `@agent.tool` decorator 注册同步/异步 tool（自动推断 input schema from type hints）
- [ ] `@agent.tool(requires_approval=True, side_effect=True)` 带元数据注册
- [ ] `agent.run(input: str)` 返回 async generator，每次 yield 一个事件 dict
- [ ] `agent.respond_approval(run_id, approved: bool)` 响应 approval 请求

**事件格式（Python dict）：**
- [ ] 每个事件包含 `type` 字段（snake_case，如 `"model_stream_chunk"`）
- [ ] 事件字段与 `RuntimeEvent` 变体对应，并透传 v0.1 canonical snake_case wire format

**Async Job 支持：**
- [ ] tool handler 可以返回 dict `{"async_job": {"job_id": ..., "poll_interval_ms": ..., "poll": <async callable>}}`
- [ ] runtime 识别该返回值，构造 `JobHandle`，poll 闭包调用 Python async callable

**打包：**
- [ ] `maturin` 构建，生成 wheel
- [ ] `python/agent_runtime/__init__.py` 提供顶层导出

**Demo 验证：**
- [ ] `python_basic.py`：注册两个 tool，运行 agent，打印流式输出和 tool 调用事件
- [ ] `python_async_tool.py`：模拟视频生成（`asyncio.sleep` 替代真实 API），打印进度事件

## 类型推断说明

v0.1 使用简单规则从 type hints 生成 JSON schema：
- `str` → `{"type": "string"}`
- `int` → `{"type": "integer"}`
- `float` → `{"type": "number"}`
- `bool` → `{"type": "boolean"}`
- 不支持的类型 → 使用 `{}` 作为 schema（接受任意值）

不引入 pydantic，避免依赖复杂度。
