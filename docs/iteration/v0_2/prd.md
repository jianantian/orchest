# v0.2 PRD：MCP 集成与生产就绪增强

## 目标

在 v0.1 验证核心架构的基础上，打通 MCP 生态，解决超长 session 的 context 管理问题，并补齐生产场景中常见的性能和可靠性需求。

v0.2 结束时，开发者应该能够：
1. 通过 stdio 或 Streamable HTTP 接入任意 MCP server，其 tool 与 in-process tool 完全统一对待
2. 注册大量 tool 而不担心 context 爆掉（Tool Search Tool 渐进式加载）
3. 接入 OpenAI 模型（GPT-4o 等）
4. 在超长 agent run 中自动触发 context compaction，session 不中断
5. 注册 webhook 模式的异步 tool（作为 polling 的补充，适用于有 push 能力的服务）

## 成功指标

- MCP server 集成通过 `mcp-server-filesystem`（官方参考实现）的端到端测试
- 注册 200 个 tool 时（`tool_search_enabled: true`），单次 model call 的 context 占用不超过注册 20 个时的 120%（Tool Search Tool 生效）
- OpenAI adapter 通过与 Anthropic adapter 相同的 smoke test 套件
- 超长 session（累计 50k+ tokens）在触发 compaction 后继续正确运行

## 范围

### MCP Server 集成

- 支持 stdio transport：`McpTransport::Stdio { command, args }`
- 支持 Streamable HTTP transport：`McpTransport::StreamableHttp { url }`
- Runtime 启动时连接所有配置的 MCP server，调用 `tools/list` 注册 tool
- `MCP Tool` 的 `execute()` 通过对应连接发送 `tools/call`
- `ToolSource::McpServer { server_id }` 区分来源

### Tool Search Tool

- 内置 `search_tools` tool，input：`{ query: string }`
- 返回 top-K 相关 tool 的完整 schema，按语义相似度排序
- 实现：embedding-free，基于 tool name + description 的 BM25 或 trigram 匹配（避免外部依赖）
- `AgentConfig` 新增 `tool_search_enabled: bool`，默认 false

### OpenAI Model Adapter

- 实现 `OpenAiAdapter`，支持 streaming（`stream: true`）
- API key 从 `OPENAI_API_KEY` 环境变量读取
- `ModelSpec` 支持 `openai/gpt-4o`、`openai/gpt-4o-mini` 等格式

### Context Compaction

- 超过 `AgentConfig.compaction_threshold`（token 比例，默认 0.8）时触发（放在 `AgentConfig` 而非 `BudgetConfig`，因为这是 context 管理策略，不是资源预算约束）
- 压缩策略：保留 system prompt + 最近 N 轮对话，对更早的历史摘要
- 摘要由模型生成（内部调用一次 model call，不计入 step 计数）
- 压缩后发出 `ContextCompacted { removed_messages, summary_tokens }` 事件

### Webhook 模式异步 Tool

- `JobHandle` 新增 `webhook: Option<WebhookConfig>` 字段
- `WebhookConfig` 包含：`callback_url`（runtime 监听的本地 HTTP endpoint）
- Runtime 启动本地 HTTP server（或复用已有的），收到 webhook 推送后唤醒等待中的 job
- Polling 仍作为默认模式；webhook 是 tool 作者可选的优化

## 不在范围内

- Skill 依赖管理（skill 的 Python/Node 依赖环境准备）
- Code Execution as MCP
- Skill sub-agent
- MCP resource 和 prompt primitive
- 多 agent 协作

## Issues 拆解

| Issue | 标题 |
|-------|------|
| [001](./issues/001-mcp-stdio.md) | MCP stdio transport 集成 |
| [002](./issues/002-mcp-http.md) | MCP Streamable HTTP transport 集成 |
| [003](./issues/003-tool-search-tool.md) | Tool Search Tool（渐进式 tool 加载） |
| [004](./issues/004-openai-adapter.md) | OpenAI Model Adapter |
| [005](./issues/005-context-compaction.md) | Context Compaction |
| [006](./issues/006-webhook-async-tool.md) | Webhook 模式异步 Tool |
