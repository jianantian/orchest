# 008 实现路线

## 步骤

1. **创建 `exceptions.py`**
   - 新建 `python/agent_runtime/exceptions.py`
   - 定义 `AgentError`（基类）和五个子类：`BudgetExceededError`, `ApprovalDeniedError`, `ModelError`, `ToolError`, `SkillError`
   - 实现 `from_code(message, code) -> AgentError` 工厂函数，按 code 字符串映射到对应子类（见 spec 中的 mapping 表）
   - 运行 `python -c "from agent_runtime.exceptions import from_code, BudgetExceededError; print(from_code('test', 'budget_exceeded'))"` 确认可用

2. **在 `__init__.py` 添加 `run_sync` 和 exception 导出**
   - 找到 `Agent` 类定义，添加两个方法：
     - `run_sync(self, prompt: str) -> list` — 用 `asyncio.run()` 包 `_collect_events`
     - `_collect_events(self, prompt: str) -> list` — async 方法，用 `async for` 收集所有事件
   - 在文件顶部添加 `from .exceptions import AgentError, BudgetExceededError, ...` import
   - 在 `__all__` 中添加 exception 类名
   - 运行 `python -c "from agent_runtime import Agent; print(hasattr(Agent, 'run_sync'))"` 确认

3. **创建测试文件**
   - 新建 `python/tests/test_run_sync.py`
   - 写入 spec 中要求的四个测试（`test_from_code_budget`, `test_from_code_unknown`, `test_agent_error_repr`, `test_agent_error_is_exception`）
   - 运行 `python -m pytest python/tests/test_run_sync.py -v` — 四个测试全过

4. **更新 PyO3 绑定层传递 error code（可选）**
   - 这步取决于 `crates/agent-runtime-py/src/lib.rs` 现有的错误处理方式
   - 先检查：当 agent run 失败时，Python 层收到的是什么异常，包含 code 字段吗
   - 如果当前 PyO3 层只传 message 字符串（`PyException::new_err(e.to_string())`），改为传 `(message, code)` 元组并在 Python 层用 `from_code` 处理
   - 如果改动涉及重新编译 Rust，需要 `maturin develop` 后再测试
   - **如果 PyO3 层改动复杂**，本 issue 可以只完成步骤 1-3，PyO3 层的结构化错误作为 follow-up issue

5. **验收**
   - `python -c "from agent_runtime import Agent, BudgetExceededError, AgentError; print('ok')"` — 输出 `ok`
   - `python -m pytest python/tests/test_run_sync.py -v` — 全绿
   - `python -c "from agent_runtime import Agent; print(hasattr(Agent, 'run_sync'))"` — `True`

## 要读的现有代码

- `python/agent_runtime/__init__.py` — 完整文件，了解 `Agent` 类的现有结构，找到 `run()` 方法位置
- `crates/agent-runtime-py/src/lib.rs` — 了解 PyO3 层当前如何处理错误，确认是否需要修改 Rust 代码

## 关键决策

- **`asyncio.run()` 的嵌套问题**：如果用户在已有 event loop 的环境中调用 `run_sync()`（比如 Jupyter），`asyncio.run()` 会报错（不能在已有 loop 的线程中创建新 loop）。可以加一个检测：`try: loop = asyncio.get_running_loop(); except RuntimeError: loop = None`，如果有运行中的 loop 就用 `loop.run_until_complete()`。但这在 Jupyter 中也不直接可用（Jupyter 的 event loop 是在后台运行的）。**建议**：`run_sync` 的文档注释明确标注"不能在 async 上下文中调用"，复杂的嵌套场景留给用户用 `asyncio.run()` 手动处理
- **`run_sync` 的返回值**：返回 `list[RuntimeEvent]`，不是 generator。这对于完整收集 events 的场景是合理的；如果用户只想要最终输出，可以在 list 里找 `RunCompleted` event 提取。不需要专门提供 `get_output()` 方法，保持 API 简洁
- **PyO3 层的错误改动是否必须**：spec 中这部分是 P2（可选），只改 Python 层的 `exceptions.py` 和 `__init__.py` 就能满足大部分需求。PyO3 Rust 层的修改是锦上添花，如果时间有限可以跳过
