# Issue 007:修复 DeepSeek adapter `supports_thinking()` 代码 bug

## 背景

`crates/agent-runtime-providers/src/providers/deepseek/mod.rs:76-79` 的 `supports_thinking()` 实现与 DeepSeek 官方文档不符:

```rust
fn supports_thinking(&self) -> bool {
    // v4-flash and legacy deepseek-reasoner support thinking; v4-pro is non-thinking only  ← 错
    let m = &self.model;
    m.starts_with("deepseek-v4-flash") || m.starts_with("deepseek-reasoner")
}
```

**事实**(来自 https://api-docs.deepseek.com/zh-cn/guides/thinking_mode):

- DeepSeek 思考模式指南的示例**全程使用 `model="deepseek-v4-pro"`** + `reasoning_effort="high"` + `extra_body={"thinking": {"type": "enabled"}}`
- 响应中含 `reasoning_content` 字段
- pricing 页"思考模式"行同时给 v4-flash 和 v4-pro 标了"支持"

**结论**:v4-pro 支持 thinking,代码注释与逻辑均错。

## 目标

修正 `supports_thinking()`,使 v4-flash 和 v4-pro 都返回 `true`。保留对 legacy `deepseek-reasoner` 的识别(兼容旧模型名,2026/07/24 弃用前仍有效)。

## 范围

### 代码修改

`crates/agent-runtime-providers/src/providers/deepseek/mod.rs:76-79`:

```rust
fn supports_thinking(&self) -> bool {
    // v4-flash and v4-pro both support thinking mode (see thinking_mode guide).
    // legacy deepseek-reasoner (deprecated 2026-07-24) also supports thinking.
    let m = &self.model;
    m.starts_with("deepseek-v4-flash")
        || m.starts_with("deepseek-v4-pro")
        || m.starts_with("deepseek-reasoner")
}
```

### 影响的测试

`crates/agent-runtime-providers/src/providers/deepseek/tests.rs` 中**可能有**基于旧逻辑的断言(例如"v4-pro 不支持 thinking")。需要:

1. grep `deepseek-v4-pro` 与 `supports_thinking` 在 `deepseek/tests.rs` 中的引用
2. 修正任何与官方文档矛盾的断言
3. 加新测试钉住 v4-pro 支持 thinking

### 不在本 issue 范围

- ❌ 改 catalog 数据(见 issue 004)
- ❌ 改 `reasoning_effort` 映射逻辑(`deepseek/request.rs:190-194` 的 `"high"/"max"` 映射保留)
- ❌ 改 `ModelCapabilities` 其它字段

## 验收标准

- [ ] `supports_thinking()` 对 `"deepseek-v4-pro"` 返回 `true`
- [ ] `supports_thinking()` 对 `"deepseek-v4-flash"` 返回 `true`(不回归)
- [ ] `supports_thinking()` 对 `"deepseek-reasoner"` 返回 `true`(legacy 兼容)
- [ ] `capabilities()` 对 v4-pro 报 `reasoning.supported = true`、`efforts = [High, Max]`
- [ ] `deepseek/tests.rs` 中基于旧逻辑的断言已修正
- [ ] 加至少 1 条新测试钉住 v4-pro 支持 thinking
- [ ] `cargo test -p agent-runtime-providers` 全部通过
- [ ] `cargo clippy -p agent-runtime-providers -- -D warnings` 通过

## 依赖

- 无外部依赖
- 与 issue 004(deepseek catalog 数据填充)并行——catalog 数据按官方文档填,代码 bug 独立修

## 注意事项

- 代码 bug 与 catalog 数据不一致是临时的:issue 004 让 catalog 正确,issue 007 让代码正确,两者合入后一致
- 不要顺手改 `reasoning_effort` 映射——DeepSeek 只接受 `"high"` / `"max"`(`thinking_mode` 文档脚注 (3) 明确 low/medium 映射为 high,xhigh 映射为 max),现有逻辑正确
