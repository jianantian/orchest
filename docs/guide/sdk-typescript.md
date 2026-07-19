# SDK · TypeScript

Orchest 的 TypeScript SDK 是 `@orchest/sdk` 包（napi-rs 绑定到 Rust 核心）。Rust 核心概念见 [quickstart](./quickstart.md)；本文聚焦 TS/Node 用法。

## 1. 安装

需要 Node ≥ 18。开发模式从源码构建原生插件：

```bash
npm install
npm run build:native     # cargo build -p orchest-node + 复制 .node 插件
```

> npm 发布计划于 v1.0。在那之前用 `npm run build:native` 从源码构建。

## 2. 基础用法

```typescript
import { Agent } from "@orchest/sdk";

const agent = new Agent({
  model: "anthropic/claude-sonnet-4-6",
  systemPrompt: "You are a helpful assistant with access to tools.",
  apiKeyEnv: "ANTHROPIC_API_KEY",
});

const events = agent.runSync("What's the weather in Tokyo?");
```

构造参数：`model`（`provider/model`）、`systemPrompt`、`apiKeyEnv`（或显式 `apiKey`、自建端点 `apiUrl`）、`maxTokens`、`budget`、`requestOptions`、`retry`（`true` 开启推荐模型重试：429/5xx/timeout/流中断，3 次指数退避；默认不重试）。

`runSync(input, messages?)` 返回 `RuntimeEvent[]`；`messages` 是多轮历史（session 快照同款 serde 形状），经 `AgentRun::start_with_messages` 带入；`runStream(input, onEvent, messages?)` 同理。事件 `run_completed.stop_reason` 标记完成原因：`"MaxTokens"` 表示输出被截断，消费方应续写/重试/报错，而非直接使用。

## 3. 注册 tool

**带执行函数**（最常用）——`registerToolWithHandler` 的 handler 在模型调用该 tool 时执行并返回结果：

```typescript
agent.registerToolWithHandler(
  "get_weather",
  "Get the current weather for a city",
  { type: "object", properties: { city: { type: "string" } }, required: ["city"] },
  (input) => ({ city: input.city, temperature: 22, condition: "sunny" }),
  { approval: "never" },   // "never" | "whenRisky" | "always"
);
```

**仅声明 schema**——`registerTool` 只注册 tool 的 schema（执行由调用方在别处处理）：

```typescript
agent.registerTool({
  name: "get_weather",
  description: "Get the current weather for a city",
  inputSchema: {
    type: "object",
    properties: { city: { type: "string" } },
    required: ["city"],
  },
});
```

### v0.9.9 迁移说明

`requiresApproval` 选项已移除；使用 `approval: "always"` 表达必须审批，使用 `approval: "never"` 表达不审批。`approvalMode: "sideEffectOnly"` 也已移除；使用默认 `approvalMode: "perTool"`，并在有副作用的 tool 上设置 `approval: "whenRisky"` 和 `sideEffect: true`。

## 4. 消费事件

`runSync` 返回的数组里每个 event 带 `type` 字段：

```typescript
for (const event of agent.runSync("What's the weather in Tokyo?")) {
  switch (event.type) {
    case "model_stream_chunk":
      process.stdout.write(event.delta?.Text?.delta ?? "");
      break;
    case "tool_call_started":
      console.log(`\n[tool] ${event.tool}(${JSON.stringify(event.input)})`);
      break;
    case "tool_call_completed":
      console.log(`[result] ${JSON.stringify(event.output)}`);
      break;
    case "run_completed":
      console.log(`\n[done] ${event.output}`);
      break;
    case "run_failed":
      console.error(`\n[error] ${event.error}`);
      break;
  }
}
```

## 5. Event type 速查

来自 `js/index.d.ts`：

```
run_started            model_call_started      model_stream_chunk
model_call_completed   tool_call_started       tool_call_update
tool_call_completed    tool_call_failed        async_tool_started
async_tool_progress    async_tool_completed    skill_content_read
approval_requested     approval_granted        approval_denied
budget_warning         child_run_event         sub_agent_started
sub_agent_completed    sub_agent_failed        run_completed
run_failed
```

> Python SDK 比 TS 多导出几个事件（`runtime_warning` / `skill_dependency_error` / `context_compacted` / `run_restarted` 等）；TS 侧以 `js/index.d.ts` 的实际导出为准。

## 6. 类型定义

完整类型见 [`js/index.d.ts`](../../js/index.d.ts)（`AgentOptions` / `RequestOptions` / `BudgetOptions` / `RuntimeEvent` / `StreamEvent` 等）和 [`js/native.d.ts`](../../js/native.d.ts)（napi 类绑定）。
