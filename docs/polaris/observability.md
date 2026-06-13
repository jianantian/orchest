# 可观测性规范

Orchest 是底层 agent runtime SDK，不是黑盒 agent 产品。可观测性是 SDK 合同的一部分：调用方必须能用统一方式观察 agent run、model call、tool execution、MCP request、Skill loading、预算消耗和上游错误。

本文是跨 `agent-runtime-core`、`agent-runtime-providers`、Python SDK、Node SDK 的权威规范。具体迭代文档可以说明如何落地，但不得重新定义一套日志、指标或 token 归因语义。

## 三类输出通道

Orchest 同时使用三类通道，它们解决不同问题，不能互相替代：

| 通道 | 面向对象 | 用途 | 稳定性 |
|------|----------|------|--------|
| `RuntimeEvent` | SDK 用户、UI、回放、集成测试 | agent 运行过程中的语义事件流 | Public API |
| `tracing` | 开发者、运维、调试工具 | 结构化日志、span、性能时间线 | Diagnostic contract |
| `metrics` | Dashboard、alert、容量规划 | 低基数聚合指标 | Metric contract |

`RuntimeEvent` 是产品语义，不是 debug log。它应该回答“agent 做了什么”。

`tracing` 是诊断语义。库 crate 只 emit spans/events，不安装 subscriber。

`metrics` 是聚合语义。库 crate 只通过 facade 记录指标，不选择 exporter。

## 依赖策略

Rust library crates 可以依赖：

```toml
tracing = "0.1"
metrics = "0.24"
```

`agent-runtime-core` 和 `agent-runtime-providers` 不得把 `tracing-subscriber`、Prometheus exporter、OpenTelemetry exporter 作为 runtime dependency。二进制入口和宿主应用负责决定日志格式、采样、导出方式和后端。

测试可以使用 test-only subscriber/recorder：

```toml
[dev-dependencies]
tracing-subscriber = { version = "0.3", features = ["fmt", "registry"] }
metrics-util = "0.20"
```

## Span 名称

所有 span 使用稳定名称和结构化字段。新增 span 前优先复用下表；确实需要新增时，必须说明它代表的生命周期边界。

| Span name | Owner | 生命周期 |
|-----------|-------|----------|
| `agent.run` | core | 一个顶层 agent run 或 sub-agent run |
| `model.complete` | providers | 一次 `ModelAdapter::complete()` 调用 |
| `provider.request` | providers | 一次上游 HTTP request 或 provider SDK call |
| `tool.execute` | core | 一次 direct / MCP-backed / Skill-backed / builtin tool 执行 |
| `mcp.request` | core | 一次 MCP transport request |
| `skill.load` | core | 一次 Skill discovery/load |
| `subagent.run` | core | 一次 delegated sub-agent run |

## Span 字段

字段名必须稳定。允许在 trace 中使用高基数字段，但不要把这些字段复制到 metrics label。

| Field | Applies to | 说明 |
|-------|------------|------|
| `run_id` | core spans | 高基数；trace 可用，metric label 禁用 |
| `parent_run_id` | sub-agent spans | 可选 |
| `run_depth` | run/sub-agent spans | 数字嵌套深度 |
| `provider` | model/provider spans | 来自 `ModelAdapter::provider_name()` |
| `model` | model/provider spans | 来自 `ModelAdapter::model_name()` |
| `model_family` | model/provider spans | 低基数模型族，例如 `claude`、`gpt`、`deepseek` |
| `streaming` | model/provider spans | 是否传入 stream channel |
| `stop_reason` | model spans | 标准化 `StopReason` |
| `tool_name` | tool spans | trace 可用；默认不要作为 metric label |
| `tool_source` | tool spans | `direct`、`mcp`、`skill`、`builtin` |
| `mcp_transport` | MCP spans | `stdio`、`http` 等 |
| `error_code` | all error spans | Orchest 稳定错误码 |
| `upstream_code` | provider error spans | provider 原始错误码 |
| `http_status` | provider/MCP spans | HTTP 状态码 |
| `input_tokens` | model spans | 来自标准化 `TokenUsage` |
| `output_tokens` | model spans | 来自标准化 `TokenUsage` |
| `reasoning_tokens` | model spans | 来自标准化 `TokenUsage` |
| `cache_read_tokens` | model spans | 来自标准化 `TokenUsage` |
| `cache_write_tokens` | model spans | 来自标准化 `TokenUsage` |
| `duration_ms` | all spans | owner 测量的 wall-clock 时长 |
| `first_token_ms` | streaming model spans | request start 到首个 text/thinking/tool delta 的时长 |

## Metric 名称

指标必须低基数。禁止使用 `run_id`、`user_id`、完整 prompt、完整错误消息、原始 provider response body、未分桶的 tool name 作为 metric label。

| Metric | Type | Labels | 来源 |
|--------|------|--------|------|
| `orchest_model_requests_total` | counter | `provider`, `model_family`, `status` | 每次 `ModelAdapter::complete()` 成功或失败后递增 |
| `orchest_model_request_duration_seconds` | histogram | `provider`, `model_family`, `status` | 完整 model call 延迟 |
| `orchest_model_stream_first_token_seconds` | histogram | `provider`, `model_family` | 首个 text/thinking/tool delta 延迟 |
| `orchest_model_stream_duration_seconds` | histogram | `provider`, `model_family`, `status` | 完整 streaming 时长 |
| `orchest_model_tokens_total` | counter | `provider`, `model_family`, `kind` | `kind`: `input`, `output`, `reasoning`, `cache_read`, `cache_write` |
| `orchest_model_usage_missing_total` | counter | `provider`, `model_family` | provider 成功响应但未返回 usage |
| `orchest_tool_calls_total` | counter | `tool_source`, `status` | tool 执行结果 |
| `orchest_tool_duration_seconds` | histogram | `tool_source`, `status` | tool 执行延迟 |
| `orchest_mcp_requests_total` | counter | `transport`, `status` | MCP transport call |
| `orchest_mcp_request_duration_seconds` | histogram | `transport`, `status` | MCP transport 延迟 |
| `orchest_run_duration_seconds` | histogram | `status` | agent run 延迟 |
| `orchest_run_failures_total` | counter | `error_code` | agent run 失败 |
| `orchest_budget_tokens_total` | counter | `kind` | 来自标准化 usage 的预算归因 |
| `orchest_budget_exceeded_total` | counter | `kind` | budget guard 拒绝 |

`model_family` 是比 `model` 更粗的归类：

- `anthropic/claude-sonnet-4-20250514` → `claude`
- `openai/gpt-4o` → `gpt`
- `deepseek/deepseek-v4-flash` → `deepseek`
- `openrouter/anthropic/claude-sonnet-4` → `claude`

OpenRouter 的 `model_family` 应从 routed model path 推断：`anthropic/claude-*` → `claude`，`openai/gpt-*` → `gpt`。无法识别时使用 `openrouter`。

需要按完整 model、tenant、user 或 tool 维度建 dashboard 的应用，可以在宿主侧追加自己的指标；SDK 默认指标必须适合长时间运行的多租户服务。

## Token 与预算归因

Token 消耗只有一个事实来源：

1. provider adapter 把上游 usage 标准化为 `TokenUsage`
2. `ModelResponse.usage` 是本次 model turn 的最终 usage fact
3. provider 返回 final usage 时，`StreamEvent::Done { usage }` 必须与 `ModelResponse.usage` 一致
4. core 使用同一份 `ModelResponse.usage` 更新 `BudgetGuard`
5. metrics 从同一份 `TokenUsage` 递增 counter

provider 成功完成但未返回 usage 时，adapter 可以返回 `TokenUsage::default()`，但必须同时：

- 在响应中记录 `OptionAdjustment { option: "usage", reason: "usage_not_reported", ... }`
- 递增 `orchest_model_usage_missing_total`

stream 在 final usage 到达前中断时，是错误，不是零 token 成功响应。adapter 必须返回稳定错误码，例如 `stream_interrupted`。

Cache usage 是 token movement，不是独立预算域：

- `cache_read_tokens` 表示从 prompt cache 读取的 token；它可能降低成本，但仍代表模型消费的输入上下文
- `cache_write_tokens` 表示写入 prompt cache 的 token；用于区分首次写入成本和普通输入 token
- budget policy 可以决定 cache-read token 全量计入、部分计入或单独展示；provider adapter 只报告标准化事实

## 错误可观测性

上游 provider 错误不得被静默吞掉。

adapter 可以标准化错误，但必须在 `ModelError` 中尽量保留：

- HTTP status
- provider error code
- provider error message
- structured provider error body

Tracing 和 metrics 的规则：

- trace 记录 `error_code`、`provider`、`model`、`http_status`、`upstream_code`
- metric 以 `status = "error"` 记录 request counter 和 duration histogram
- provider raw body 保留在 `ModelError.upstream_body` 供调用方调试，但默认不写入 tracing，也绝不作为 metric label
- adapter 不得把上游失败转换为成功空响应，不得在失败时 emit `Done`

## 隐私与安全边界

默认日志不得包含：

- prompt/system text
- tool arguments
- tool output
- generated text
- thinking/reasoning text
- provider-native replay metadata，例如 `provider_details`
- API keys、authorization headers、cookies
- provider raw response body

可选 debug 配置可以允许记录 provider raw body，但必须先 redact 已知 secret 字段。即便开启 debug，也不应记录 thinking/reasoning 正文，除非宿主应用明确承担该数据风险。

## 测试要求

涉及 core 或 provider observability 的变更，至少覆盖：

- canonical span 名称和字段存在
- prompt/tool payload/reasoning text 不出现在默认 tracing 输出
- metrics label 不包含高基数字段
- `ModelResponse.usage`、`StreamEvent::Done.usage`、BudgetGuard 更新、token metrics 使用同一份 usage
- provider error 会递增 `status = "error"` 指标，并保留可调试的 `ModelError` 字段
- 成功但缺失 usage 时记录 `usage_not_reported` adjustment 和 usage-missing metric
