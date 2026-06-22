# 002 · Minimax LLM adapter — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:executing-plans` /
> `superpowers:subagent-driven-development`。步骤用 checkbox 跟踪。

**Goal:** fork `AnthropicAdapter` 成 `MinimaxAdapter`,注册 + catalog,并实现 Minimax 专属的
多模态 content block 与 role 序列化。

**Architecture:** `ModelAdapter` trait 不动;新增 `providers/minimax/` 模块 + `MinimaxFactory`。
依赖 001 引入的 `ContentBlock`/`Role` 新 variant。

**Tech Stack:** Rust, reqwest, serde_json, SSE。

---

## 要读的现有代码

- `crates/agent-runtime-providers/src/providers/anthropic/{mod,request,response}.rs`(fork 模板)
- `crates/agent-runtime-providers/src/registry.rs`(`ProviderFactory` / `ProviderRegistry::new`)
- `crates/agent-runtime-providers/src/providers/mod.rs`(submodule 声明)
- `crates/agent-runtime-providers/src/catalog/mod.rs`(条目形态)
- `crates/agent-runtime-providers/src/defaults.rs`(默认 URL 放置处)
- `docs/external/minimax/llm.md`(协议锚点)

## 文件改动

- Add: `crates/agent-runtime-providers/src/providers/minimax/{mod,request,response}.rs`
- Modify: `crates/agent-runtime-providers/src/providers/mod.rs`(`pub mod minimax;`)
- Modify: `crates/agent-runtime-providers/src/registry.rs`(注册 `MinimaxFactory`)
- Modify: `crates/agent-runtime-providers/src/catalog/...`(4 条目)
- Modify: `crates/agent-runtime-providers/src/defaults.rs`(若默认 URL 集中于此)

## 步骤

### 1. fork adapter 骨架

- [ ] 拷 `anthropic/{mod,request,response}.rs` 到 `minimax/`,改类型名 `Anthropic*` → `Minimax*`。
- [ ] `mod.rs` 定义 `MinimaxFactory`(`ProviderFactory`):`provider_name()=="minimax"`,
      `default_api_key_env()` 返回 Minimax key 环境变量名,`create_adapter` 默认 URL
      `https://api.minimaxi.com`,鉴权 `Authorization: Bearer`。

### 2. 请求序列化差异

- [ ] `request.rs` 的 `ContentBlock` match:`Image` 输出 Minimax image schema(`source` Url/Base64);
      `Video` 输出含 `fps` / `max_long_side_pixel` / `detail`;`MidConvSystem` 输出对应 block。
- [ ] `Role` match:4 个 Minimax-only role 输出 `user_system` / `group` / `sample_message_user` /
      `sample_message_ai` 字符串(锚点 `llm.md:1088-1091`)。
- [ ] 透传 `RequestOptions.service_tier` 到请求体(`llm.md:807`)。

### 3. 响应 / SSE

- [ ] `response.rs` 复用 Anthropic SSE 解析(`message_start`/`content_block_delta`/`thinking_delta`/
      `message_stop`),确认 `thinking` adaptive 事件序列与 Anthropic 一致(设计文档 §2.1 表)。

### 4. 注册 + catalog

- [ ] `providers/mod.rs` 加 `pub mod minimax;`。
- [ ] `registry.rs` 的 `ProviderRegistry::new()` 加 `reg.register(Box::new(super::providers::minimax::MinimaxFactory));`。
- [ ] catalog 加 MiniMax-M3 / M2.7 / M2.5 / M2.1,前缀 `minimax/`,pricing 留 TODO。

### 5. 测试

- [ ] image(Url + Base64)/ video / midConvSystem 序列化单元测试。
- [ ] 4 个 minimax-only role 序列化单元测试。
- [ ] SSE thinking 事件序列单元测试(复用 anthropic 测试夹具改造)。
- [ ] tool_use → `StopReason::ToolUse` 单元测试。
- [ ] registry 测试:`supported_providers()` 含 `"minimax"`。

### 6. 验证

```bash
cargo test -p agent-runtime-providers
cargo clippy -p agent-runtime-providers -- -D warnings
cargo fmt --check
```

Live(手动,记录到验证报告):`MiniMax-M3` 纯文本 + `thinking: adaptive`,确认事件序列。
