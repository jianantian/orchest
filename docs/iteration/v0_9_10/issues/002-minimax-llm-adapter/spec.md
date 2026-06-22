# 002 · Minimax LLM adapter(Anthropic 兼容)

## 背景

Minimax LLM 自称 "Anthropic API 兼容 Messages 格式"(`docs/external/minimax/llm/api.md:7`):同路径
`POST /anthropic/v1/messages`、同 SSE 事件、同 `thinking` schema、同 tool 协议。因此
**`ModelAdapter` trait 不动**,以现有 `AnthropicAdapter` 为模板 fork 出 `MinimaxAdapter`。

依赖 issue 001:本 issue 序列化 `ContentBlock::Image`/`Video`/`MidConvSystem` 与 4 个 Minimax-only
role —— 这些 variant 由 001 引入。

设计来源:[`minimax-api-analysis.md`](../../../../research/minimax-api-analysis.md) §二 2.1-2.2。
协议兼容性核对表见设计文档 §2.1(路径 / 鉴权 / SSE / thinking / tool 逐项对齐 `llm/api.md` 行号)。

## 2a. 新建 adapter 模块

`crates/agent-runtime-providers/src/providers/minimax/{mod,request,response}.rs`,以
`providers/anthropic/` 为骨架 fork。

> 第一刀直接拷贝;不在本迭代抽 `messages_common.rs`。若后续两 adapter 共同点 ≥80% 再抽
> (设计文档 §2.2 step 1)。

与 Anthropic 的差异点:
- 序列化 `ContentBlock::Image`/`Video`/`MidConvSystem`(Anthropic adapter 对 Video/MidConvSystem 走 drop)
- 序列化 4 个 Minimax-only role(`UserSystem`/`Group`/`SampleMessageUser`/`SampleMessageAi`)
- 透传 `RequestOptions.service_tier`
- 默认 API URL、catalog 前缀不同

## 2b. 注册到 ProviderRegistry

- `registry.rs`:`ProviderRegistry::new()` 内 `reg.register(Box::new(MinimaxFactory))`。
- `providers/mod.rs`:声明 `pub mod minimax;`。
- `MinimaxFactory`:`provider_name() == "minimax"`,`default_api_key_env()` 返回 Minimax key 环境变量名。
- 默认 API URL `https://api.minimaxi.com`(`llm/api.md:36-37`)。
- 鉴权 `Authorization: Bearer ${api_key}`(`llm/api.md:38-40` 允许 Bearer / x-api-key,与 Anthropic adapter 对齐用 Bearer)。

## 2c. Catalog 条目

`catalog/` 加 8 个非 Her 模型(`llm/desc.md:15-23` 全列表):
MiniMax-M3 / M2.7 / M2.7-highspeed / M2.5 / M2.5-highspeed / M2.1 / M2.1-highspeed / M2(前缀
`minimax/MiniMax-M3` 等)。定价待补(catalog 字段对齐 hotfix 06-17 扩展后的 `LlmModelEntry`;pricing
留空或 TODO)。`-highspeed` 变体上下文窗口同基础模型(204,800),仅吞吐不同,作为独立条目入 catalog。

> 不含 Her:`docs/external/minimax/` 无 Her schema,不在本 issue 凭印象加条目(设计文档 §九,PRD 非目标)。

## 验收标准

- [ ] `crates/agent-runtime-providers/src/providers/minimax/{mod,request,response}.rs` 存在
- [ ] `MinimaxFactory` 注册进 `ProviderRegistry`,`supported_providers()` 含 `"minimax"`
- [ ] catalog 含上述 8 条非 Her 模型条目(`M3` / `M2.7` / `M2.7-highspeed` / `M2.5` / `M2.5-highspeed` / `M2.1` / `M2.1-highspeed` / `M2`),前缀 `minimax/...`
- [ ] 默认 API URL 为 `https://api.minimaxi.com`,鉴权头为 `Authorization: Bearer`
- [ ] `MinimaxAdapter` 序列化 `ContentBlock::Image`(Url + Base64 两形态)产生 Minimax 期望 JSON,有单元测试
- [ ] `MinimaxAdapter` 序列化 `ContentBlock::Video` / `MidConvSystem` 产生对应字段(含 `fps`/`max_long_side_pixel`)
- [ ] `MinimaxAdapter` 序列化 4 个 Minimax-only role 输出对应字符串值
- [ ] `service_tier` 透传进请求体
- [ ] SSE 解析复用 Anthropic 路径,单元测试覆盖 `message_start`/`content_block_delta`/`thinking_delta`/`message_stop` → `ThinkingStart`/`Thinking{delta}`/`ThinkingEnd` 事件序列
- [ ] `tool_use` 路径产生 `StopReason::ToolUse`(单元测试)
- [ ] `cargo test -p agent-runtime-providers` 全绿;`clippy -- -D warnings` 无 warning

> Live 验证(手动,env-var gated,记录在迭代验证报告):用真实 key 跑 MiniMax-M3 纯文本 +
> `thinking: adaptive`,确认事件序列(设计文档 §2.4 清单)。
