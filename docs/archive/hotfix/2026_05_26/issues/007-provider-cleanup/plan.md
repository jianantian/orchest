# 007 · Provider 层去重与代码组织 — 实施计划

## 依赖

**必须在 006 之后**（`ProviderFactory` 返回 `Box<dyn ModelAdapter>`，`ModelAdapter` 在 006 后位于 `agent-runtime-model`）。建议也在 002 之后（ModelError 瘦身后代码更干净）。

本 issue 改动面最大（6 个子项），建议按 A2 → A3 → A5 → A4 → A9 → A11 顺序执行。

## 步骤

### Step 1: A2 — ProviderFactory trait + 注册表

**文件**：`crates/agent-runtime-providers/src/lib.rs`

1. 定义 `ProviderFactory` trait：
   ```rust
   pub trait ProviderFactory: Send + Sync {
       fn provider_name(&self) -> &str;
       fn create_adapter(&self, config: ProviderRuntimeConfig) -> Result<Box<dyn ModelAdapter>, ModelError>;
       fn normalize_model(&self, model: &str) -> String;
       fn default_api_key_env(&self) -> &str;
   }
   ```

2. 为每个 adapter 实现 factory：
   - `AnthropicFactory` in `anthropic.rs`
   - `OpenAiFactory` in `openai.rs`
   - `DeepSeekFactory` in `deepseek.rs`
   - `OpenRouterFactory` in `openrouter.rs`

3. 创建 `ProviderRegistry`（`HashMap<&str, Box<dyn ProviderFactory>>`）：
   ```rust
   pub struct ProviderRegistry {
       factories: HashMap<String, Box<dyn ProviderFactory>>,
   }
   impl ProviderRegistry {
       pub fn new() -> Self { /* 注册 4 个内置 factory */ }
       pub fn register(&mut self, factory: Box<dyn ProviderFactory>) { ... }
       pub fn create_adapter(&self, config: ProviderRuntimeConfig) -> Result<Box<dyn ModelAdapter>, ModelError> { ... }
   }
   ```

4. `create_adapter_from_config` 改为委托给 registry
5. 删除三处 match arm 的重复代码

### Step 2: A3 — OpenAI 兼容 adapter 去重

**文件**：新建 `crates/agent-runtime-providers/src/openai_compat.rs`

1. 从 `openai.rs`、`deepseek.rs`、`openrouter.rs` 中提取共享代码：
   ```rust
   pub fn serialize_messages(messages: &[Message]) -> Vec<Value> { ... }
   pub fn build_request_body(messages: &[Message], tools: &[ToolDef], options: &RequestOptions) -> Value { ... }
   pub fn parse_stream_response(/* ... */) -> Result<ModelResponse, ModelError> { ... }
   ```
2. 先处理消息序列化（~210 行重复）——逐 Role 分支提取
3. 再处理 `complete()` 后半段（error → usage → Done → telemetry，~240 行重复）
4. 各 adapter 只保留：endpoint URL、特殊参数、模型名映射等差异部分
5. 在 `lib.rs` 中添加 `pub(crate) mod openai_compat;`

### Step 3: A5 — mod.rs 瘦身

**skill/mod.rs**（604 行）：

1. 新建 `crates/agent-runtime-core/src/skill/scanner.rs`，移入 `SkillScanner`
2. 新建 `crates/agent-runtime-core/src/skill/types.rs`，移入 `SkillManifest` 等纯数据类型
3. `skill/mod.rs` 只保留 re-export（目标 <50 行）

**tool/mod.rs**（111 行）：

1. 当前 111 行含 `Tool` trait + 关联类型，评估是否超标
2. 如果拆分：移到 `tool/traits.rs`，`mod.rs` 只 re-export
3. 如果 111 行可接受（trait 定义在 mod.rs 是常见模式）：保持，添加注释说明

### Step 4: A4 — 超长文件拆分

在 A3 去重后，重新测量各文件行数：

```bash
find crates/ -name '*.rs' ! -path '*/target/*' | xargs wc -l | awk '$1 > 700' | sort -rn
```

仍超标的文件逐个处理：

- **anthropic.rs**（1603 行）：拆出 `anthropic/tests.rs`（~900 行测试），主文件拆为 `anthropic/mod.rs` + `anthropic/stream.rs`（`complete()` 的 stream 处理部分）
- **mcp.rs**（770 行）：拆为 `mcp/mod.rs` + `mcp/stdio.rs` + `mcp/http.rs` + `mcp/types.rs`
- **bundled_tool.rs**（673 行）：拆出 `skill/env_manager.rs`
- **sse.rs**（540 行）：评估是否可拆——如果 A3 去重已降低，可能不需要

### Step 5: A9 — Webhook HTTP 解析替换

**文件**：`crates/agent-runtime-core/src/run/webhook.rs`（165 行）

1. 添加依赖：`httparse = "1"` 到 `agent-runtime-core/Cargo.toml`
2. 替换手动 `"POST /webhook"` 匹配为 `httparse::Request::new()` 解析
3. 替换 `"Content-Length:"` 匹配为 httparse header 查找
4. 保留 TCP listener 架构，只替换解析层
5. 测试现有 webhook 功能不破坏

### Step 6: A11 — SkillScanner 异步 I/O

**文件**：`crates/agent-runtime-core/src/skill/scanner.rs`（A5 拆分后的新位置）

1. `std::fs::read_dir` → `tokio::fs::read_dir`
2. `std::fs::read_to_string` → `tokio::fs::read_to_string`
3. `scan_recursive` 改为 `async fn`
4. 更新调用链（`register_skills` 等）适配 async

## 文件影响范围

```
crates/agent-runtime-providers/src/
  lib.rs              — A2 (ProviderFactory + Registry)
  openai_compat.rs    — A3 (新建，共享代码)
  openai.rs           — A3 (去重后瘦身)
  deepseek.rs         — A3 (去重后瘦身)
  openrouter.rs       — A3 (去重后瘦身)
  anthropic.rs        — A4 (拆分)
  anthropic/          — A4 (新建目录)

crates/agent-runtime-core/src/
  skill/mod.rs        — A5 (瘦身到 re-export)
  skill/scanner.rs    — A5+A11 (新建，SkillScanner + async I/O)
  skill/types.rs      — A5 (新建，SkillManifest 等)
  skill/env_manager.rs — A4 (从 bundled_tool 拆出)
  tool/mcp/           — A4 (拆为 mod+stdio+http+types)
  run/webhook.rs      — A9 (httparse 替换)

Cargo.toml           — httparse 依赖
```

## 验证

```bash
# 每个 step 之后：
cargo check --workspace

# 全部完成后：
cargo test --workspace
cargo clippy --workspace -- -D warnings

# 文件长度检查：
find crates/ -name '*.rs' ! -name 'tests.rs' ! -name '*_test.rs' ! -path '*/target/*' \
  | xargs wc -l | awk '$1 > 700' | sort -rn  # 应为空

# mod.rs 长度检查：
find crates/ -name 'mod.rs' ! -path '*/target/*' \
  | xargs wc -l | awk '$1 > 50' | sort -rn  # 应为空

# 阻塞 I/O 检查：
grep -rn 'std::fs::' crates/ --include='*.rs' | grep -v target | grep -v test  # 应为空
```
