# 013 · TypeScript SDK（napi-rs Binding）

## 背景

通过 napi-rs 把 Rust core 暴露给 Node.js/TypeScript，提供函数式 tool 注册和 AsyncIterator 事件流，让 TS 用户能以自然的 TS 方式使用 runtime。

## 目标

实现 `@orchest/agent-runtime` npm 包，完成 `examples/ts_basic.ts` 和 `examples/ts_streaming.ts` 两个 demo。

## 验收标准

**核心 API：**
- [ ] `new Agent({ model, systemPrompt, skillsDir?, budget? })` 构造
- [ ] `agent.tool({ name, description, input: ZodSchema, handler })` 注册 tool（input schema 从 Zod schema 生成）
- [ ] `agent.tool({ ..., requiresApproval: true, sideEffect: true })` 带元数据注册
- [ ] `agent.run(input: string): AsyncIterableIterator<RuntimeEvent>` 返回事件流
- [ ] `agent.respondApproval(runId: string, approved: boolean)` 响应 approval

**事件类型（TypeScript）：**
- [ ] 每个事件有 `type` 字段（camelCase，如 `"modelStreamChunk"`）
- [ ] 提供完整的 TypeScript 类型定义（`RuntimeEvent` discriminated union）

**Async Job 支持：**
- [ ] tool handler 可以返回 `{ asyncJob: { jobId, pollIntervalMs, poll: () => Promise<JobPollResult> } }`
- [ ] `JobPollResult` 为 `{ pending: { progress?: number } }` 或 `{ completed: unknown }` 或 `{ failed: string }`
- [ ] runtime 识别该返回值，构造 `JobHandle`

**打包：**
- [ ] `@napi-rs/cli` 构建，生成 `.node` native addon
- [ ] `js/index.ts` 提供顶层导出和完整类型定义
- [ ] 支持 CommonJS 和 ESM 双模式

**Demo 验证：**
- [ ] `ts_basic.ts`：注册两个 tool，运行 agent，打印事件
- [ ] `ts_streaming.ts`：实时打印 token 流式输出，展示 `model_stream_chunk` 处理

## 说明

Zod 作为 peer dependency，不打包进 SDK。用户不使用 Zod 时可以直接传入 JSON schema object（`input: { type: 'object', properties: {...} }`）。
