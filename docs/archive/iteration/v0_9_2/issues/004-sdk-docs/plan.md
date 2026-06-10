# 004 · SDK 文档 — 实现计划

## 要读的现有代码

- `python/agent_runtime/__init__.pyi` — Python `Agent` 签名、event type 列表、TypedDict、异常类
- `python/agent_runtime/exceptions.pyi` — 异常层级
- `examples/python/basic.py` — 装饰器注册 + 事件遍历的权威写法
- `examples/python/async_tool.py` — 异步 tool 形状（仅链接）
- `pyproject.toml` — maturin 配置、build-backend
- `js/index.d.ts`、`js/native.d.ts` — TS `Agent` 签名、event type、导出类型
- `examples/typescript/basic.ts` — schema 注册 + switch 事件的权威写法
- `package.json` — 包名 `@orchest/agent-runtime`、`build:native` script、Node 版本
- `README.md:29-69` — 既有最短片段，保持一致

## 步骤

### 1. sdk-python.md

按 spec 7 小节填充。关键代码块直接取自 `examples/python/basic.py`：

```python
from agent_runtime import Agent

agent = Agent(
    model="anthropic/claude-sonnet-4-6",
    system_prompt="You are a helpful assistant.",
    api_key_env="ANTHROPIC_API_KEY",
)

@agent.tool
def get_weather(city: str) -> dict:
    """Get the current weather for a city."""
    return {"city": city, "temperature": 22, "condition": "sunny"}

for event in agent.run("What's the weather in Tokyo?"):
    if event["type"] == "model_stream_chunk":
        delta = event.get("delta", {})
        if "Text" in delta:
            print(delta["Text"]["delta"], end="")
    elif event["type"] == "run_completed":
        print("\n[done]", event["output"])
```

event type 速查表照抄 `__init__.pyi` 的字符串字面量列表。

### 2. sdk-typescript.md

按 spec 6 小节填充。关键代码块取自 `examples/typescript/basic.ts`：

```typescript
import { Agent } from "@orchest/agent-runtime";

const agent = new Agent({
  model: "anthropic/claude-sonnet-4-6",
  systemPrompt: "You are a helpful assistant.",
  apiKeyEnv: "ANTHROPIC_API_KEY",
});

agent.registerToolWithHandler(
  "get_weather",
  "Get the current weather for a city",
  { type: "object", properties: { city: { type: "string" } }, required: ["city"] },
  (input) => ({ city: input.city, temperature: 22, condition: "sunny" }),
);

for (const event of agent.runSync("What's the weather in Tokyo?")) {
  switch (event.type) {
    case "model_stream_chunk":
      process.stdout.write(event.delta?.Text?.delta ?? "");
      break;
    case "run_completed":
      console.log("\n[done]", event.output);
      break;
  }
}
```

event type 速查表照抄 `index.d.ts` 的 union 列表。

### 3. 交叉链接

两份文档顶部各加一句"Rust 核心概念见 [quickstart](./quickstart.md)"，互相之间不重复概念。

### 4. 验证

- 人工核对代码块与对应 example 一致
- 检查内部链接：`python/agent_runtime/__init__.pyi`、`js/index.d.ts`、`examples/python/basic.py`、`examples/typescript/basic.ts` 路径存在
- 若环境允许：`maturin develop` + 跑 `examples/python/basic.py`（需 key，非 CI 必须）

## 关键决策

- **以 .pyi / .d.ts 为 event type 单一真相**：两个 SDK 的 event 字符串列表略有差异（如 TS 当前未列 `model_retry` / `runtime_warning` 等），分别按各自导出写，不强行对齐，避免文档承诺 SDK 未导出的 type。
- **Python 用装饰器、TS 用 registerToolWithHandler 作主示例**：分别是两个 SDK 最自然的 tool 注册方式。
- **不写未导出的 API**：TS async `run`、Python stream callback 等若 binding 未稳定导出，不进文档（Notes 已标注以实际签名为准）。
