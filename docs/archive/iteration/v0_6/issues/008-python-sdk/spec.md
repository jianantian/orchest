# 008 · Python SDK 易用性改进

## 背景

两个已知的 Python SDK 易用性问题：

### 问题 1：缺少同步入口

Python 开发者在 Jupyter notebook、简单脚本、pytest（默认非 async）等环境中需要手动 `asyncio.run()`，无法直接调用 `agent.run()`：

```python
# 现在需要这样
import asyncio
events = asyncio.run(collect(agent.run("hello")))

# 应该直接这样
events = agent.run_sync("hello")
```

### 问题 2：异常类型丢失结构化信息

PyO3 绑定层当前把 Rust 错误直接转成 Python 通用 `Exception`，丢失了 `ToolError.code`、`ModelError.code` 等结构化字段，消费者无法做 `except BudgetExceededError` 之类的细粒度捕获。

## 变更

### 1. `run_sync()` 方法

文件：`python/agent_runtime/__init__.py`

```python
import asyncio
from typing import Iterator

class Agent:
    # 现有 async run() 方法保持不变

    def run_sync(self, prompt: str) -> list["RuntimeEvent"]:
        """Synchronous convenience method. Runs the agent and collects all events.

        Suitable for scripts, Jupyter notebooks, and synchronous test frameworks.
        Raises AgentError (or subclass) if the run fails.
        """
        return asyncio.run(self._collect_events(prompt))

    async def _collect_events(self, prompt: str) -> list["RuntimeEvent"]:
        events = []
        async for event in self.run(prompt):
            events.append(event)
        return events
```

### 2. 结构化异常类型

文件：`python/agent_runtime/exceptions.py`（新文件）

```python
class AgentError(Exception):
    """Base exception for all agent runtime errors."""

    def __init__(self, message: str, code: str | None = None) -> None:
        super().__init__(message)
        self.code = code

    def __repr__(self) -> str:
        return f"{type(self).__name__}(message={str(self)!r}, code={self.code!r})"


class BudgetExceededError(AgentError):
    """Raised when the agent exceeds its configured budget."""


class ApprovalDeniedError(AgentError):
    """Raised when a required tool approval is denied."""


class ModelError(AgentError):
    """Raised when the LLM provider returns an error."""


class ToolError(AgentError):
    """Raised when a tool execution fails."""


class SkillError(AgentError):
    """Raised when skill loading or execution fails."""


def from_code(message: str, code: str | None) -> AgentError:
    """Map an error code string to the appropriate exception subclass."""
    mapping = {
        "budget_exceeded": BudgetExceededError,
        "max_steps_reached": BudgetExceededError,
        "approval_denied": ApprovalDeniedError,
        "model_error": ModelError,
        "tool_error": ToolError,
        "skill_error": SkillError,
    }
    cls = mapping.get(code or "", AgentError)
    return cls(message, code)
```

文件：`python/agent_runtime/__init__.py`（更新导出）

```python
from .exceptions import (
    AgentError,
    BudgetExceededError,
    ApprovalDeniedError,
    ModelError,
    ToolError,
    SkillError,
)

__all__ = [
    "Agent",
    "AgentError",
    "BudgetExceededError",
    "ApprovalDeniedError",
    "ModelError",
    "ToolError",
    "SkillError",
]
```

### 3. PyO3 绑定层传递 code 字段

文件：`crates/agent-runtime-py/src/lib.rs`

当前错误转换可能是类似：

```rust
Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
```

改为在 Python 侧构造结构化异常：

```rust
// lib.rs 中导入 Python 异常类
fn map_run_error(py: Python<'_>, message: &str, code: Option<&str>) -> PyErr {
    let exceptions = py.import("agent_runtime.exceptions").unwrap();
    let from_code = exceptions.getattr("from_code").unwrap();
    let exc = from_code.call1((message, code)).unwrap();
    PyErr::from_value(exc.into())
}
```

如果 PyO3 绑定层目前的错误处理只传 message 字符串，则至少保证把 `RunFailed.error` 中的 error code 字符串（如 `"budget_exceeded"`）传递到 Python 层，供 `from_code` 映射。

## 验收标准

### run_sync()

- [ ] `Agent.run_sync(prompt: str) -> list[RuntimeEvent]` 方法存在
- [ ] 在非 async 上下文调用不抛出 `RuntimeError: no running event loop`
- [ ] 返回的 event 列表和 `async for` 遍历 `agent.run()` 得到的相同
- [ ] Jupyter notebook 中可直接调用（无需 `await`）

### 异常类型

- [ ] `agent_runtime.exceptions` 模块存在
- [ ] `AgentError`, `BudgetExceededError`, `ApprovalDeniedError`, `ModelError`, `ToolError`, `SkillError` 均可从 `agent_runtime` 直接 import
- [ ] `from agent_runtime import BudgetExceededError` 可用
- [ ] `from_code("budget exceeded", "budget_exceeded")` 返回 `BudgetExceededError` 实例
- [ ] `from_code("unknown", None)` 返回 `AgentError` 实例
- [ ] `AgentError` 实例的 `.code` 属性与构造时传入的 `code` 一致

### 单元测试

文件：`python/tests/test_run_sync.py`（新文件）

```python
import pytest
from unittest.mock import AsyncMock, MagicMock
from agent_runtime import Agent, AgentError, BudgetExceededError
from agent_runtime.exceptions import from_code


def test_from_code_budget():
    exc = from_code("budget exceeded", "budget_exceeded")
    assert isinstance(exc, BudgetExceededError)
    assert exc.code == "budget_exceeded"


def test_from_code_unknown():
    exc = from_code("something went wrong", None)
    assert isinstance(exc, AgentError)
    assert exc.code is None


def test_agent_error_repr():
    exc = AgentError("test message", code="test_code")
    assert "test_code" in repr(exc)
    assert "test message" in repr(exc)
```

### 正确性

- [ ] `cargo build -p agent-runtime-py` 通过（PyO3 层编译无错）
- [ ] `python -c "from agent_runtime import Agent, BudgetExceededError; print('ok')"` 输出 `ok`
- [ ] `pytest python/tests/test_run_sync.py` 全绿
