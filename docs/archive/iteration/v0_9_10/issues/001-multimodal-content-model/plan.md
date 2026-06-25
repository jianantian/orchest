# 001 · 多模态 content model 地基 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: 用 `superpowers:executing-plans` 或
> `superpowers:subagent-driven-development` 逐步实现。步骤用 checkbox 跟踪。

**Goal:** 在 `agent-runtime-model` 加齐多模态 content block / Minimax role / `service_tier`,
并让 5 个现有 LLM adapter 在新 variant 下编译通过且行为正确(Anthropic image 真实现,其余 OptionAdjustment)。

**Architecture:** 破坏性扩展共享 enum,blast radius 是 5 个 provider 的 `request.rs`。公共
`ModelAdapter` trait 与请求/响应流程不变。

**Tech Stack:** Rust, serde, serde_json。

---

## 要读的现有代码

- `crates/agent-runtime-model/src/types.rs`(`ContentBlock` / `Role` / `Message`)
- `crates/agent-runtime-model/src/options.rs`(`RequestOptions` + 手写 `Default`)
- `crates/agent-runtime-providers/src/types.rs`(`OptionAdjustment` / `CompatibilityPolicy` 用法)
- `crates/agent-runtime-providers/src/providers/anthropic/request.rs`(`ContentBlock` 序列化模板)
- `crates/agent-runtime-providers/src/providers/{openai,deepseek,openrouter,volcengine}/request.rs`
- `crates/agent-runtime-providers/src/providers/openai/request.rs` 内现有 OptionAdjustment-drop 写法

## 文件改动

- Modify: `crates/agent-runtime-model/src/types.rs`
- Modify: `crates/agent-runtime-model/src/options.rs`
- Modify: `crates/agent-runtime-providers/src/providers/anthropic/request.rs`(+ tests)
- Modify: `crates/agent-runtime-providers/src/providers/{openai,deepseek,openrouter,volcengine}/request.rs`
- Test: 各 adapter 内 `#[cfg(test)]` 断言新 variant 行为

## 步骤

### 1. model crate 加类型

- [ ] `types.rs` 新增 `MediaSource { Url(String), Base64 { media_type, data } }`,derive
      `Debug, Clone, Serialize, Deserialize, PartialEq`。
- [ ] `ContentBlock` 加 `Image` / `Video` / `Audio` / `MidConvSystem`(字段见 spec 1b)。
- [ ] `Role` 加 `UserSystem` / `Group` / `SampleMessageUser` / `SampleMessageAi`;在 enum 上方
      加注释说明这 4 个为 Minimax 专属、仅 `MinimaxAdapter` 序列化。
- [ ] `options.rs` 给 `RequestOptions` 加 `service_tier: Option<String>`,更新 `Default` 为 `None`,
      字段 doc 注释标注与 `aigc .../types/video.rs` 同名字段语义不同。
- [ ] `cargo build -p agent-runtime-model` 通过。

### 2. Anthropic adapter — Image 真实现 + 其余 drop

- [ ] 在 `anthropic/request.rs` 的 `ContentBlock` match 加 `Image { source, detail }` arm,
      序列化为 Anthropic image block:`Url` → `{type:url,url}`;`Base64` → `{type:base64,media_type,data}`。
- [ ] `Video` / `Audio` / `MidConvSystem` arm → 丢弃 + 记 `OptionAdjustment`(参照同文件
      thinking 不支持路径的写法)。
- [ ] `Role` 新 variant:`UserSystem` → 序列化为 system 语义,其余 3 个 → user 语义 + OptionAdjustment。
- [ ] 加单元测试:含 `ContentBlock::Image(Url)` 和 `Image(Base64)` 的 message 序列化出预期 JSON 形状。

### 3. openai / deepseek / openrouter / volcengine — 全部 drop

- [ ] 四个 `request.rs` 的 `ContentBlock` match 各加 `Image`/`Video`/`Audio`/`MidConvSystem` arm,
      统一走 OptionAdjustment-drop。
- [ ] 四个 adapter 的 `Role` match 各加 4 个新 variant 的映射(同步骤 2 规则)。
- [ ] 每个 adapter 加 1 个单元测试:含新 content variant 的 message 不 panic,产生 OptionAdjustment。

### 4. 验证

```bash
cargo test -p agent-runtime-model -p agent-runtime-providers
cargo clippy -p agent-runtime-model -p agent-runtime-providers -- -D warnings
cargo fmt --check
rg -n "todo!\(|unimplemented!\(" crates/agent-runtime-providers/src/providers
```

最后一条在生产代码应无命中(新 match arm 不得用 `todo!`/`unimplemented!`)。
