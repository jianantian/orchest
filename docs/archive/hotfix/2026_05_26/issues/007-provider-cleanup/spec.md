# 007 · Provider 层去重与代码组织

## 背景

Review 发现 provider 层的多个代码质量问题，集中在 `crates/agent-runtime-providers/` 和 `crates/agent-runtime-core/` 的代码组织上。

## A2. Provider 注册表违反开闭原则

**文件**：`providers/lib.rs:51-92`

新增 provider 需修改三处 match：`create_adapter_from_config`、`normalize_provider_model`、`default_api_key_env`。每个 arm 构造 `XxxConfig` 字段完全相同。

**修复**：`ProviderFactory` trait + `inventory` 或手动 `HashMap` 注册表：

```rust
pub trait ProviderFactory: Send + Sync {
    fn provider_name(&self) -> &str;
    fn create_adapter(&self, config: ProviderConfig) -> Result<Box<dyn ModelAdapter>, ModelError>;
    fn normalize_model(&self, model: &str) -> String;
    fn default_api_key_env(&self) -> &str;
}
```

每个 adapter 注册自己的 factory。`create_adapter_from_config` 变为 registry 查询。

## A3. OpenAI 兼容 adapter 消息序列化重复（~450 行）

**文件**：`openai.rs`、`deepseek.rs`、`openrouter.rs`

`Role::System`/`Role::User`/`Role::Tool` 分支逐行相同（~210 行）。`complete()` 后半段（错误处理→usage→Done→telemetry）另有 ~240 行重复。

**修复**：抽取 `OpenAiCompatibleAdapter` 基础实现或 helper 模块：

```rust
// providers/src/openai_compat.rs
pub fn serialize_messages(messages: &[Message]) -> Vec<Value> { ... }
pub fn parse_sse_response(...) -> Result<ModelResponse, ModelError> { ... }
pub fn build_request_body(messages: &[Message], tools: &[ToolDef], options: &RequestOptions) -> Value { ... }
```

各 adapter 调用共享函数，只覆写差异部分（endpoint URL、特殊参数、模型名映射）。

## A4. 文件 / 函数超长

当前超标文件（AGENTS.md 约定 400 行/文件）：

| 文件 | 行数 |
|------|------|
| `providers/anthropic.rs` | 1,603 |
| `providers/openrouter.rs` | 886 |
| `providers/deepseek.rs` | 840 |
| `providers/lib.rs` | 821 |
| `providers/openai.rs` | 815 |
| `core/tool/mcp.rs` | 770 |
| `core/skill/bundled_tool.rs` | 673 |
| `core/skill/mod.rs` | 604 |
| `providers/types.rs` | 586 |
| `providers/sse.rs` | 540 |
| `core/tool/builtin.rs` | 440 |

超标函数：

| 函数 | 行数 |
|------|------|
| `AnthropicAdapter::complete()` | 363 |
| `parse_openai_sse_stream()` | 311 |

**修复策略**：

- `anthropic.rs`：拆出测试到 `anthropic/tests.rs`（大约 900 行是测试），主文件降到 ~700 行。`complete()` 拆为 `build_request` + `process_stream` + `finalize_response`
- `openai.rs`/`deepseek.rs`/`openrouter.rs`：A3 去重后自然降到 400 以内
- `lib.rs`：A2 注册表重构后降低
- `mcp.rs`：拆为 `mcp/stdio.rs` + `mcp/http.rs` + `mcp/types.rs`
- `skill/mod.rs`：见 A5
- `bundled_tool.rs`：拆出 `env_manager.rs`
- `parse_openai_sse_stream()`：拆为逐 event 处理函数

## A5. `mod.rs` 含业务逻辑

| 文件 | 行数 | 内容 |
|------|------|------|
| `skill/mod.rs` | 604 | 4 个 struct + SkillScanner |
| `tool/mod.rs` | 111 | Tool trait + 全部关联类型 |
| `model/mod.rs` | 12 | 27 符号 re-export（006 会处理） |

**修复**：

- `skill/mod.rs`：将 `SkillScanner` 移到 `skill/scanner.rs`，`SkillManifest` 等类型移到 `skill/types.rs`，`mod.rs` 只保留 re-export
- `tool/mod.rs`：111 行含 `Tool` trait 定义是合理的——trait 定义本身在 mod.rs 是常见模式。但如果加上关联类型超过 50 行约定，拆到 `tool/traits.rs`

## A9. Webhook HTTP 手动解析

**文件**：`run/webhook.rs`（165 行）

原始 TCP socket + 字符串匹配 `"POST /webhook"` 和 `"Content-Length:"`。不支持 chunked、keep-alive、大小写不敏感 header。

**修复**：用标准 HTTP 解析库替代手动字符串匹配。165 行的手写 HTTP 不值得维护。

依赖选择：

- **方案 A**（推荐）：`httparse`（零拷贝 HTTP 解析，~2k LOC，零依赖）+ 保留手动 TCP listener。只替换字符串匹配部分，不引入完整 HTTP server 框架。符合极简 Core 原则
- **方案 B**：`hyper = { version = "1", features = ["http1", "server"] }`。功能完整但依赖树较重，与 Core 的最小依赖原则有张力

实施时优先评估方案 A。如果 webhook 后续需要 keep-alive / chunked 等完整 HTTP 语义再考虑方案 B。

## A11. `SkillScanner` 阻塞 I/O

**文件**：`skill/mod.rs`（A5 拆出后为 `skill/scanner.rs`）

`scan_recursive()` 调用 `std::fs::read_dir()`，`parse_skill_md()` 调用 `std::fs::read_to_string()`。被 async `register_skills()` 同步调用。

**修复**：改用 `tokio::fs`：

```rust
use tokio::fs;

async fn scan_recursive(dir: &Path) -> Result<Vec<SkillManifest>, ...> {
    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        // ...
        let content = fs::read_to_string(skill_md_path).await?;
        // ...
    }
}
```

或者保持 `std::fs` 但包 `spawn_blocking`（如果不想改 API）：

```rust
let manifests = tokio::task::spawn_blocking(move || {
    scan_recursive_sync(&dir)
}).await??;
```

推荐 `tokio::fs` 方案，更惯用。

## 验收标准

- [ ] A2：新增 provider 只需实现 `ProviderFactory` trait，不修改 `lib.rs`
- [ ] A3：`openai.rs`/`deepseek.rs`/`openrouter.rs` 共享消息序列化代码，无逐行重复
- [ ] A4：所有 `.rs` 文件（测试文件除外）不超过 700 行
- [ ] A4：所有函数不超过 200 行
- [ ] A5：`skill/mod.rs` 不超过 50 行（纯 re-export）
- [ ] A5：`tool/mod.rs` 不超过 50 行（纯 re-export + trait 可留）
- [ ] A9：`run/webhook.rs` 使用标准 HTTP 解析（优先 `httparse`，符合最小依赖原则）
- [ ] A11：`SkillScanner` 无 `std::fs` 调用（使用 `tokio::fs` 或 `spawn_blocking`）
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
