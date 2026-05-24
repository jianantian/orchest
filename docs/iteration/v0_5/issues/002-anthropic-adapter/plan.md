# 002 实现路线

## 步骤

1. **迁移基础结构**
   - 创建 `crates/agent-runtime-providers/src/anthropic.rs`
   - 从 `crates/agent-runtime-core/src/model/anthropic.rs`（570 行）复制整个文件作为起点
   - 修改 import：`super::*` → `crate::types::*`（或 `crate::*`），删除 `crate::tool::ToolDef` 引用
   - `lib.rs` 添加 `pub mod anthropic;` 和 re-export
   - `cargo check` 通过后再继续

2. **适配 ModelAdapter trait 签名**
   - 现有 core 的 trait 方法是 `stream()` + `call()`，新 trait 是 `complete()`
   - 把现有 `stream()` 实现改名为 `complete()`，签名改为 `&self, messages, tools, options: &RequestOptions, tx: Option<mpsc::Sender<StreamEvent>>`
   - 删除 `call()` 的默认实现（移到 issue 006 的 `chat()` helper）
   - 添加 `provider_name()` → `"anthropic"`、`model_name()` → `self.model.clone()`、`capabilities()` 三个方法
   - `capabilities()` 暂时返回 hardcoded `ModelCapabilities`，后续细化 static table

3. **扩展 SSE 解析**
   - 现有代码已经解析 `message_start` / `content_block_start` / `content_block_delta` / `content_block_stop` / `message_delta`
   - 现有代码已处理 thinking（ThinkingStart / Thinking / ThinkingEnd）和 tool_use，但用的是 `ModelStreamChunk` → 改为 `StreamEvent`
   - 新增：`StreamEvent::ThinkingEnd` 需要 `signature` 字段——从 `content_block_stop` 的 `content_block` 中提取
   - 新增：`StreamEvent::ToolUseStart { id, name }` 和 `StreamEvent::ToolUseEnd { id }`——现有代码没有这些事件
   - 新增：`ContentBlock::Thinking { text, signature, provider_details: None }` 构建——累积 thinking text + signature 后加入 `ModelResponse.content`
   - 新增：`tx: None` 分支——跳过所有 `tx.send()`，仍解析并构建 response
   - 新增：stream_interrupted 和 usage_not_reported 处理

4. **新增 ThinkingLevel / CachePolicy / include_thinking 映射**
   - 在 `build_request_body()` 中根据 `options: &RequestOptions` 添加：
     - ThinkingLevel → `thinking.type` + `output_config.effort` (adaptive) 或 `thinking.budget_tokens` (enabled)
     - 需要一个 `fn supports_adaptive(&self) -> bool` 方法按 model name 判断
     - include_thinking → `thinking.display`
     - CachePolicy → top-level `cache_control`
     - max_tokens override
     - temperature / top_p
   - OptionAdjustment 记录：adaptive mode 下传 budget_tokens 时

5. **扩展 error 和 stop reason**
   - 现有 `ModelError` 只有 message + code，扩展为 7 字段
   - HTTP 非 2xx 时：解析 response body，填充 provider / status / upstream_code / upstream_message / upstream_body
   - Stop reason：从 3 个变体扩展到完整映射

6. **写测试**
   - 迁移现有 core 测试（`uses_default_api_url` 等 4 个 URL 测试、`stream_thinking_boundaries`、`stream_rejects_malformed_sse`）
   - 现有测试用 `serve_sse_once` 模式（bind 127.0.0.1:0），保持这个模式
   - 新增 15+ 个测试（thinking mapping、cache、error preservation 等）

## 要读的现有代码

- `crates/agent-runtime-core/src/model/anthropic.rs` — 完整的 570 行，是迁移起点
- `crates/agent-runtime-core/src/model/mod.rs` — 理解现有类型结构
- `crates/agent-runtime-core/src/tool/mod.rs` — `ToolDef` 定义（确认 providers 版本字段一致）

## 关键决策

- Adaptive vs enabled 模式判断：按 model name 前缀（如 `claude-opus-4-` / `claude-sonnet-4-` 且日期后缀 >= 某版本）还是按 capabilities table？建议用 static table，与 `capabilities()` 方法共享数据源
- `AnthropicConfig` 保留现有 `from_config()` 接口，删除 `new()` 和 `new_with_api_url()` 简化 API surface
