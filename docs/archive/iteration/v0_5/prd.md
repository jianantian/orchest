# v0.5 PRD：LLM Provider 独立 Crate

## 前置依赖

v0.4 的 Hotfix 2026-05 已修复 provider tool protocol 映射等问题（hotfix issue 005）。本迭代的 provider 拆分在 hotfix 完成后启动最合理，但 issue 001（类型定义）和 issue 002–005（adapter 实现）不依赖 hotfix——它们是新 crate、不改 core 运行时行为。issue 007（core 迁移）需要 core 稳定后再做。

## 目标

把 LLM provider 逻辑从 `agent-runtime-core` 抽取为独立的 `agent-runtime-providers` crate，提供统一的 `ModelAdapter` trait、四个 provider adapter（Anthropic / OpenAI / DeepSeek / OpenRouter）、以及完整的 thinking / caching / compatibility 归一化。

v0.5 结束时，开发者应该能够：
1. `cargo add agent-runtime-providers` 获得完整的 LLM 调用库，无需拉入 agent runtime
2. 通过 `create_adapter("anthropic/claude-sonnet-4", None)` 一行代码获得可用的 adapter
3. 用同一套 `RequestOptions`（ThinkingLevel / CachePolicy / CompatibilityPolicy）控制所有 provider 的行为，adapter 自动处理 provider 差异
4. 通过 `stream_chat()` / `chat()` 获得归一化的 `StreamEvent` 流和 `ModelResponse`
5. Core 通过 re-export 透明迁移，SDK 仅需一行 import path 更新

## 成功指标

- `cargo test -p agent-runtime-providers` 全绿，覆盖所有 adapter 的 SSE 解析、类型映射、错误保留、ThinkingLevel 映射、CachePolicy 映射
- `cargo test --workspace` 全绿——core 迁移后所有现有测试不 break
- `cargo clippy --workspace -- -D warnings` 全绿
- `agent-runtime-providers` 零 workspace 内部依赖（`Cargo.toml` 不含任何 `path = "../..."` 依赖）
- 独立用法示例（spec 中的 standalone usage example）可编译运行

## 范围

### 公共类型（`types.rs`）

从 core 的简化类型扩展为完整的归一化类型系统：
- `ModelAdapter` trait：统一 `complete()` 签名（`Option<mpsc::Sender<StreamEvent>>`）
- `Message` / `Role` / `ContentBlock`：新增 `ContentBlock::Thinking` 变体
- `StreamEvent`：替代 `ModelStreamChunk`，新增 `ThinkingEnd { signature, provider_details }`、`ToolUseStart`、`ToolUseEnd`
- `ModelResponse`：新增 `option_adjustments: Vec<OptionAdjustment>`
- `ModelError`：扩展为保留 upstream provider 错误（status / code / message / body）
- `TokenUsage`：新增 reasoning_tokens / cache_read_tokens / cache_write_tokens / details
- `StopReason`：新增 StopSequence / ContentFilter / Refusal / ContextWindowExceeded / Pause / Interrupted / Other(String)
- `RequestOptions`：ThinkingLevel / CachePolicy / CompatibilityPolicy / temperature / top_p
- `ModelCapabilities` / `ReasoningCapability` / `CacheCapability`：归一化能力元数据
- `ToolDef` / `JsonSchema`：从 core 的 `tool` 模块拆出到 providers

### Anthropic Adapter

从 core `model/anthropic.rs` 迁移并扩展：
- Anthropic-specific SSE 解析（`message_start` / `content_block_start` / `content_block_delta` / `content_block_stop` / `message_delta`）
- ThinkingLevel → adaptive (`output_config.effort`) / enabled (`thinking.budget_tokens`) 双模式映射
- `include_thinking` → `thinking.display`（`"summarized"` / `"omitted"`）
- CachePolicy → top-level `cache_control`
- Thinking signature 保留与 `ContentBlock::Thinking` 构建
- 扩展的 stop reason 和 error 映射

### SSE 共享解析器 + OpenAI Adapter

- `sse.rs`：OpenAI-compatible SSE 共享解析（OpenAI / DeepSeek / OpenRouter 共用）
- 从 core `model/openai.rs` 迁移并扩展
- ThinkingLevel → `reasoning_effort`
- CachePolicy → `prompt_cache_retention`

### DeepSeek Adapter（新）

- OpenAI-compat 协议 + DeepSeek 特有的 thinking mode
- canonical `reasoning`（兼容读取 `reasoning_content` alias）→ `StreamEvent::Thinking` / `ContentBlock::Thinking`
- Reasoning replay：assistant tool-call turns 必须回传 canonical `reasoning`（实现层可同时携带 `reasoning_content` 以兼容 provider 历史行为）
- Thinking 启用时忽略 temperature / top_p

### OpenRouter Adapter（新）

- OpenAI-compat 协议 + OpenRouter 特有的 headers 和 reasoning 对象
- `reasoning.effort` / `reasoning.max_tokens` 互斥
- `reasoning_details` 精确保留（不可重排 / 摘要 / 过滤）
- `include_thinking: false` → `reasoning.exclude: true`
- SSE reasoning 字段 canonical 使用 `reasoning`（`reasoning_content` 仅作为解码 alias）

### Factory 与 Telemetry

- `create_adapter("provider/model", api_key)` 工厂函数
- `stream_chat()` / `chat()` 便利函数
- `telemetry.rs`：`model.complete` / `provider.request` span helpers

### Core 迁移

- Core `Cargo.toml` 添加 providers 依赖
- `model/mod.rs` 替换为 re-exports
- 删除 `model/anthropic.rs` 和 `model/openai.rs`
- `ModelStreamChunk` 保留为 `StreamEvent` 的 type alias
- 更新 `run.rs` / `events.rs` / `tool/mod.rs` 的 import paths

## 不在范围内

- Feature flags（四个 adapter 全部无条件编译；heavyweight provider 如 Bedrock 留后续）
- 新的 runtime 特性（run loop / event 类型 / Tool trait 不改）
- Core-side observability instrumentation（`agent.run` / `tool.execute` / `mcp.request` span 注入是 spec 中提及的 core 侧改动，但涉及 run.rs / tool/* / skill/* / budget.rs 等大量文件，且与 provider 拆分正交——留后续 issue 单独做）
- Per-block cache placement（只用 top-level cache_control）
- Provider metadata endpoint 动态查询（v0.5 仅用 static capability table）
- 正式 crates.io 发包（先验证 workspace 内使用）
- SDK 文档更新（随 v0.4 docs 一并调整）

## Issues 拆解

| Issue | 标题 |
|-------|------|
| [001](./issues/001-types-and-scaffold/spec.md) | Crate 骨架与公共类型 |
| [002](./issues/002-anthropic-adapter/spec.md) | Anthropic Adapter |
| [003](./issues/003-sse-and-openai/spec.md) | SSE 共享解析器与 OpenAI Adapter |
| [004](./issues/004-deepseek-adapter/spec.md) | DeepSeek Adapter |
| [005](./issues/005-openrouter-adapter/spec.md) | OpenRouter Adapter |
| [006](./issues/006-factory-and-telemetry/spec.md) | Factory 函数与 Telemetry |
| [007](./issues/007-core-migration/spec.md) | Core 迁移与 workspace 验证 |

## 推荐执行节奏

1. **001 先做完**：所有 adapter 都依赖 types.rs 中的公共类型；类型先稳定，后续 adapter 可并行
2. **002 与 003 可并行**：Anthropic 有自己的 SSE 协议，不依赖 sse.rs；OpenAI 需要 sse.rs，但 sse.rs 和 Anthropic 无关
3. **004 和 005 紧跟 003**：DeepSeek 和 OpenRouter 都用 sse.rs，依赖 003
4. **006 依赖 002–005**：factory 需要所有 adapter 就位
5. **007 收尾**：core 迁移是最后一步，改动面最广，需要所有 adapter + factory 稳定

### 依赖图（DAG）

```
001 ──┬──> 002 ──────────────┐
      │                      ├──> 006 ──> 007
      └──> 003 ──┬──> 004 ──┘
                 └──> 005 ──┘
```

## 设计参考

详细的类型定义、adapter 映射表、provider 协议细节见 [agent-runtime-providers 设计文档](../superpowers/specs/2026-05-24-agent-runtime-providers-design.md)。issues 中不重复 spec 已有的完整代码；验收标准引用 spec 中的类型和映射作为 source of truth。

## v0.5 权威顺序（避免冲突）

1. `docs/iteration/v0_5/issues/*/spec.md` 与 `plan.md` 是 v0.5 实施与验收的第一权威。
2. `docs/iteration/v0_5/prd.md` 约束范围、依赖和优先级；与 issue 细节冲突时，以 issue spec 的可测试条款为准。
3. `docs/iteration/superpowers/specs/2026-05-24-agent-runtime-providers-design.md` 是设计映射参考；若与 issue 验收条款冲突，需要先回写 issue/prd 再执行实现。
4. 旧的根技术设计文档已删除，不作为 v0.5 决策与验收依据。
