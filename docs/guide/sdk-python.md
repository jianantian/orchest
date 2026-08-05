# SDK · Python

Orchest 的 Python SDK 是 `orchest` 包（PyO3 绑定到 Rust 核心）。Rust 核心概念见 [quickstart](./quickstart.md)；本文聚焦 Python 用法。

## 1. 安装

开发模式用 [maturin](https://www.maturin.rs/) 在本地构建扩展：

```bash
pip install maturin
maturin develop          # 或：uvx maturin develop
```

> PyPI 发布计划于 v1.0。在那之前用 `maturin develop` 从源码构建。

## 2. 基础用法

```python
from orchest import Agent

agent = Agent(
    name="assistant",
    model="anthropic/claude-sonnet-4-6",
    system_prompt="You are a helpful assistant with access to tools.",
    api_key_env="ANTHROPIC_API_KEY",
)

events = agent.run("What's the weather in Tokyo?")
```

`Agent(...)` 必填参数：`name`（日志与 handoff 使用的人类可读名称）、`model`（`provider/model` 字符串）、`system_prompt`。常用可选参数：`api_key_env`（也可用 `api_key` 显式传，或 `api_url` 指向自建端点）、`max_tokens`、`budget`、`request_options`、`retry`（`True` 开启推荐模型重试：429/5xx/timeout/流中断，3 次指数退避；默认不重试）。

`run(input, messages=None)` 和 `run_sync(input, messages=None)` 都返回 `list[RuntimeEvent]`（一次性返回事件列表，不是流式 generator）。`messages` 是多轮历史（session 快照同款 serde 形状），经 `AgentRun::start_with_messages` 带入；`run_stream(input, on_event, messages=None)` 同理。事件 `run_completed.stop_reason` 标记完成原因：`"MaxTokens"` 表示输出被截断，消费方应续写/重试/报错，而非直接使用。

### GIL 行为

`run()` / `run_sync()` 在 Rust run loop 执行期间使用 `py.detach()` 释放 Python GIL；长时间 model call、Rust tool、MCP tool、skill tool 不会因为 Python 调用线程持有 GIL 而阻塞其它 Python 线程。返回事件列表转换成 Python dict/list 时会短暂重新持有 GIL。

`run_stream(input, on_event)` 在后台线程运行 Rust agent；调用线程只在把单个事件转换为 dict 并执行 `on_event(event)` 时持有 GIL，等待新事件时会释放 GIL。`on_event` 是用户 Python callback；如果它自己执行长时间 CPU-bound Python 代码，它会按普通 Python 规则占用 GIL。

Python 注册的 tool callback 必须在持有 GIL 时执行，因为它运行用户 Python 代码。同步 callback 的耗时由用户代码决定；异步 callback 返回 coroutine 时，binding 会在后台线程中运行 coroutine，并在等待结果时释放调用线程的 GIL。

示例见 [`examples/python/gil_behavior.py`](../../examples/python/gil_behavior.py)。

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

### v0.9.9 迁移说明

`requires_approval` 参数已移除；使用 `approval="always"` 表达必须审批，使用 `approval="never"` 表达不审批。`approval_mode="side_effect_only"` 也已移除；用默认 `approval_mode="per_tool"`，并在有副作用的 tool metadata 上设置 `approval="when_risky"` 与 `side_effect=True`。

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

来自 `python/orchest/__init__.pyi`：

```
run_started            model_call_started      model_stream_chunk
model_call_completed   tool_call_started       tool_call_update
tool_call_completed    tool_call_failed        async_tool_started
async_tool_progress    async_tool_completed    skill_content_read
approval_requested     approval_granted        approval_denied
budget_warning         runtime_warning         skill_dependency_error
skill_missing_capabilities  skill_load_warning  context_compacted
child_run_event        sub_agent_started       sub_agent_completed
sub_agent_failed       run_restarted           run_completed
run_failed
```

## 6. 异常

绑定层把内部错误转成 Python 异常（见 `python/orchest/exceptions.pyi`）：

- `AgentError`（基类，带可选 `code`）
- `BudgetExceededError`
- `ApprovalDeniedError`
- `ModelError`
- `ToolError`
- `SkillError`

## 7. 类型提示

完整签名与 TypedDict 见类型存根 [`python/orchest/__init__.pyi`](../../python/orchest/__init__.pyi)，覆盖 `Agent`、`BudgetOptions`、`RequestOptions`、`RuntimeEvent` 等。
