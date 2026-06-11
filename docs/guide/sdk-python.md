# SDK · Python

Orchest 的 Python SDK 是 `agent_runtime` 包（PyO3 绑定到 Rust 核心）。Rust 核心概念见 [quickstart](./quickstart.md)；本文聚焦 Python 用法。

## 1. 安装

开发模式用 [maturin](https://www.maturin.rs/) 在本地构建扩展：

```bash
pip install maturin
maturin develop          # 或：uvx maturin develop
```

> PyPI 发布计划于 v1.0。在那之前用 `maturin develop` 从源码构建。

## 2. 基础用法

```python
from agent_runtime import Agent

agent = Agent(
    model="anthropic/claude-sonnet-4-6",
    system_prompt="You are a helpful assistant with access to tools.",
    api_key_env="ANTHROPIC_API_KEY",
)

events = agent.run("What's the weather in Tokyo?")
```

`Agent(...)` 常用参数：`model`（`provider/model` 字符串）、`system_prompt`、`api_key_env`（也可用 `api_key` 显式传，或 `api_url` 指向自建端点）、`max_tokens`、`budget`、`request_options`。

`run(input)` 和 `run_sync(input)` 都返回 `list[RuntimeEvent]`（一次性返回事件列表，不是流式 generator）。

## 3. 注册 tool

最常用的是 `@agent.tool` 装饰器——函数的 docstring 作为 description，参数名/类型推断出 input schema：

```python
@agent.tool
def get_weather(city: str) -> dict:
    """Get the current weather for a city."""
    return {"city": city, "temperature": 22, "condition": "sunny"}
```

也可以显式注册：

```python
agent.register_tool(get_weather, approval="never")   # "never" | "when_risky" | "always"
```

异步 tool（长任务轮询）返回 `{"async_job": {...}}` 形状，见 [`examples/python/async_tool.py`](../../examples/python/async_tool.py)。

## 4. 消费事件

`run()` 返回的列表里每个 event 是带 `"type"` 字段的 dict：

```python
for event in agent.run("What's the weather in Tokyo?"):
    etype = event.get("type")
    if etype == "model_stream_chunk":
        delta = event.get("delta", {})
        text = delta.get("Text") or delta.get("text")
        if text:
            print(text.get("delta", ""), end="", flush=True)
    elif etype == "tool_call_started":
        print(f"\n[tool] {event.get('tool')}({event.get('input', {})})")
    elif etype == "tool_call_completed":
        print(f"[result] {event.get('output', {})}")
    elif etype == "run_completed":
        print(f"\n[done] {event.get('output', '')}")
    elif etype == "run_failed":
        print(f"\n[error] {event.get('error', '')}")
```

## 5. Event type 速查

来自 `python/agent_runtime/__init__.pyi`：

```
run_started            model_call_started      model_stream_chunk
model_call_completed   tool_call_started       tool_call_update
tool_call_completed    tool_call_failed        async_tool_started
async_tool_progress    async_tool_completed    skill_content_read
approval_requested     approval_granted        approval_denied
budget_warning         runtime_warning         skill_dependency_error
skill_missing_capabilities                     context_compacted
child_run_event        sub_agent_started       sub_agent_completed
sub_agent_failed       run_restarted           run_completed
run_failed
```

## 6. 异常

绑定层把内部错误转成 Python 异常（见 `python/agent_runtime/exceptions.pyi`）：

- `AgentError`（基类，带可选 `code`）
- `BudgetExceededError`
- `ApprovalDeniedError`
- `ModelError`
- `ToolError`
- `SkillError`

## 7. 类型提示

完整签名与 TypedDict 见类型存根 [`python/agent_runtime/__init__.pyi`](../../python/agent_runtime/__init__.pyi)，覆盖 `Agent`、`BudgetOptions`、`RequestOptions`、`RuntimeEvent` 等。
