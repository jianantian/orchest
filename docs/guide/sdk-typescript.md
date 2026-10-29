# SDK · TypeScript

Orchest 的 TypeScript SDK 是 npm 上的 `orchest-sdk` 包（napi-rs 绑定到 Rust 核心）。Rust 核心概念见 [quickstart](./quickstart.md)；本文聚焦 TS/Node 用法。

## 1. 安装

```bash
npm install orchest-sdk
```

> 首个版本 SDK 0.2 正在发布（[#330](https://github.com/jianantian/orchest/issues/330)）。在它上线 npm 之前，请按下面「从源码构建」安装。

需要 Node ≥ 18。`orchest-sdk` 只含 JavaScript 和类型声明；原生插件在按平台拆分的包里（`orchest-sdk-linux-x64-gnu`、`orchest-sdk-linux-arm64-gnu`、`orchest-sdk-darwin-arm64`），作为 `optionalDependencies` 由 npm 只下载当前平台那一个，不需要 Rust 工具链：

| 平台 | 支持 |
| --- | --- |
| Linux x86_64（glibc ≥ 2.28） | 预编译插件 |
| Linux arm64（glibc ≥ 2.28） | 预编译插件 |
| macOS arm64 | 预编译插件 |
| 其他平台（Windows、Intel macOS、musl/Alpine） | 不支持：加载时报错，错误信息给出当前平台和支持的平台列表 |

用 `--omit=optional`（或 `--no-optional`）安装会跳过平台包，加载时报错提示重新安装。Linux 插件静态链接 OpenSSL，不依赖系统的 `libssl` / `libcrypto`，证书仍从系统 CA 库读取。

### 版本

SDK 当前是 0.x，`orchest-sdk` 和 PyPI 上的 `orchest-py` 共用一个 SDK 版本，独立于 Rust crate 版本（[ADR-0004](../adr/0004-sdk-packaging.md) D6）。0.x 的 minor 版本可能改动 TypeScript API，改动在 [`CHANGELOG-SDK.md`](../../CHANGELOG-SDK.md) 里以 **Breaking:** 标出。预发布版本在 `next` dist-tag 下（`npm install orchest-sdk@next`）。

### 从源码构建（贡献者）

开发 Orchest 本身时从源码构建原生插件：

```bash
npm install
npm run build:native     # napi build，生成仓库根目录的 orchest_node.<platform>.node
npm test
```

加载器优先使用仓库根目录里本地构建的插件，其次才找平台包。要在另一个本地项目里试未发布的改动，打包后安装两个 tarball：

```bash
cd /absolute/path/to/orchest
npm run pack:local       # 构建插件，打包到 target/npm-pack/
cd /absolute/path/to/downstream
npm install /absolute/path/to/orchest/target/npm-pack/*.tgz
```

### 运行示例

`examples/typescript/` 下的示例是 CommonJS 风格的 `.ts`（`declare` + `require`，与本包 `"type": "commonjs"` 一致），
Node ≥ 22.18 可直接执行（内建 type stripping）：

```bash
npm run build:native                                   # 原子 API 示例走 js/index.js，需要仓库根目录的插件
OPENROUTER_API_KEY=sk-or-... node examples/typescript/providers/decisions.ts
```

`examples/typescript/agents|streaming|basic.ts` 直接从 `target/debug` 取原生插件，先跑 `cargo build -p orchest-node`。
Node < 22.18 需要自备 TS runner，例如 `npx tsx examples/typescript/providers/decisions.ts`（tsx 也能直接跑这些示例）。

`import { decide } from "orchest-sdk"` 在 ESM/TS 工程里同样可用（具名导出已声明），示例用 `require` 只是为了能在本仓库零依赖直接执行。

## 2. 基础用法

```typescript
import { Agent } from "orchest-sdk";

const agent = new Agent({
  name: "assistant",
  model: "anthropic/claude-sonnet-4-6",
  systemPrompt: "You are a helpful assistant with access to tools.",
  apiKeyEnv: "ANTHROPIC_API_KEY",
});

const events = await agent.runSync("What's the weather in Tokyo?");
```

必填构造参数：`name`（日志与 handoff 使用的人类可读名称）、`model`（`provider/model`）、`systemPrompt`。常用可选参数：`apiKeyEnv`（或显式 `apiKey`、自建端点 `apiUrl`）、`maxTokens`、`budget`、`requestOptions`、`retry`（`true` 开启推荐模型重试：429/5xx/timeout/流中断，3 次指数退避；默认不重试）。

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

## 4. 原子下游 API

completion、结构化判断、一次性 ASR 和 realtime ASR 都可绕过 agent loop 直接调用：

```typescript
import { readFile } from "node:fs/promises";
import { complete, decide, startAsrStream, transcribe } from "orchest-sdk";

const text = await complete({
  model: "deepseek/deepseek-flash",
  user: "用一句话概括这段录音",
  apiKeyEnv: "DEEPSEEK_API_KEY",
});

const decision = await decide({
  model: "openrouter/~typesafe/jev-latest",
  state: { message: "I was charged twice" },
  questions: {
    urgent: { type: "boolean", instructions: "Is this urgent?" },
    team: {
      type: "choice",
      instructions: "Route the case",
      criteria: { billing: "Payment issues", support: "Product help" },
    },
  },
  apiKeyEnv: "OPENROUTER_API_KEY",
});
if (decision.answers.urgent.type === "boolean" && decision.answers.urgent.probability >= 0.8) {
  console.log("escalate");
}

const transcript = await transcribe(await readFile("voice.m4a"), {
  format: "m4a",
  apiKeyEnv: "DASHSCOPE_API_KEY",
});

const stream = await startAsrStream(
  {
    format: "pcm",
    sampleRate: 16_000,
    provider: "aliyun",
    model: "qwen-audio-3.0-asr-flash-streaming",
    context: [{ role: "user", text: "Emile，Orchest，Murmur" }],
    apiKeyEnv: "DASHSCOPE_API_KEY",
  },
  (event) => {
    switch (event.kind) {
      case "draft": // 临时结果，同段下一条会整段替换
        console.log("…", event.text);
        break;
      case "stable": // 已确认文稿；update 为 append 或 snapshot
        console.log("✓", event.text);
        break;
      case "turn": // 一轮说完，可交给业务或 Agent
        console.log("turn", event.segment_id);
        break;
    }
  },
);
await stream.sendAudio(await readFile("chunk.pcm"));
stream.finish();
await stream.wait();
```

`decide` 一次提交共享 `state` 和按 ID 索引的 `boolean`、`choice`、`score` 问题。
响应字段保持 snake_case；`confidence`、概率分布和 usage 未返回时保持缺失。
完整客服路由示例见 [`examples/typescript/providers/decisions.ts`](../../examples/typescript/providers/decisions.ts)。

`provider` 只写厂商名（如 `"deepgram"`），模型用独立的 `model` 选项；`provider/model` 合写会被拒绝。
两者都省略时，一次性识别使用 aliyun 的 `qwen-audio-3.0-asr-flash`，realtime 使用 aliyun 在目录里的默认流式模型
（目前是 `qwen-audio-3.0-asr-flash-streaming`，与只给 `provider="aliyun"` 时相同）；只给 `provider` 时，一次性识别只在该厂商的一次性模型里选默认，realtime 只在流式模型里选。厂商只有另一类模型时报 `UnsupportedOperation` 并指明该用哪个接口；有多个候选且没有默认时，报错列出可选的 `model`。

realtime 事件是按 `kind` 区分的联合类型 `AsrStreamEvent`（[ADR-0006](../adr/0006-voice-session-model.md)）：
`draft`（临时结果）、`stable`（已确认文稿，带 `update: "append" | "snapshot"`）、`turn`（一轮说完，
`segment_id` 指向它结束的 stable）、`session_started` / `route_selected` / `session_closed`、`error`
（`code`、`message`、`fatal`、`operation`，回应 `finalize()` 时 `operation` 为 `"finalize"`）和 `extras`（厂商特有数据）。字段保持 snake_case。

端侧 VAD 检测到停顿后调用 `await stream.finalize()`：要求 provider 尽快定稿并给出 `turn`，会话保持打开，
可以继续 `sendAudio`。`finalize()` resolve 只表示请求已提交，结果看随后的事件：一个 `turn`，或一个
`operation === "finalize"`、`fatal` 为假的 `error`（例如上游超时），之后流仍可用。不支持的模型会以
`code === "unsupported_option"` 的 `ProviderError` 拒绝，不发送；会话已结束时调用也会拒绝。
`finish()` 关闭输入并结束会话。

event callback 必须同步返回；返回 Promise 或抛出异常会终止输入，
并由 `wait()` 抛出原始异常。

## 5. 消费事件

`runSync` 返回 Promise（napi 侧是 async fn），resolve 出的数组里每个 event 带 `type` 字段：

```typescript
for (const event of await agent.runSync("What's the weather in Tokyo?")) {
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

## 6. Event type 速查

来自 `js/types.d.ts`（37 个变体，与 `crates/orchest/src/events.rs` 的 `RuntimeEvent` 一一对应，由 `scripts/check-ts-event-variants.sh` 守卫；Python 绑定走同一个 wire 转换，形状一致）：

```
run_started                      model_call_started               model_stream_chunk
model_call_completed             model_retry                      tool_call_started
tool_call_update                 tool_call_completed              tool_call_failed
tool_call_retry                  tool_call_batch_started          tool_call_batch_item_started
tool_call_batch_item_completed   async_tool_started               async_tool_progress
async_tool_completed             skill_content_read               approval_requested
approval_granted                 approval_denied                  budget_warning
runtime_warning                  skill_missing_capabilities       skill_load_warning
context_compacted                sub_agent_started                sub_agent_completed
sub_agent_failed                 child_run_event                  sub_agent_event
hook_panicked                    agent_updated                    events_dropped
run_restarted                    run_completed                    run_failed
run_aborted
```

## 7. 类型定义

`js/` 下各文件的职责分工：

| 文件 | 角色 |
| --- | --- |
| `js/index.js` | 运行时，同时是运行时签名的唯一来源（JSDoc 注解） |
| `js/index.d.ts` | **由 `index.js` 生成**（`npm run build:types`），不要手改；CI 用 `git diff --exit-code` 卡漂移 |
| `js/types.d.ts` | 手写数据形状（`AgentOptions` / `RequestOptions` / `BudgetOptions` / `RuntimeEvent` / `StreamEvent` 等），含唯一的 `RuntimeEvent` 定义 |
| `js/native.d.ts` | napi 类与函数声明（`Agent` 定义在此） |
| `js/public.d.ts` | 包的 `types` 入口：re-export 上述三份 |

本仓库验证：`npm install` 后 `npm run typecheck`（`tsc --noEmit`，含 `js/index.js` 的 `checkJs` 与 `examples/typescript/**`）；改动 `index.js` 签名后跑 `npm run build:types` 重新生成声明。CI 里对应 `npm ci` + typecheck + 生成后 `git diff --exit-code`。
