# 005 · AgentConfig 校验 — 实施计划

## 依赖

无前置依赖。但改动面较广——`build()` 签名变更会波及测试和 SDK binding。

## 步骤

### Step 1: 定义 ConfigError

**文件**：`crates/agent-runtime-core/src/run/config.rs`

1. 添加 `ConfigError` 枚举（thiserror 派生）：
   ```rust
   #[derive(Debug, thiserror::Error)]
   pub enum ConfigError {
       #[error("model must be specified")]
       MissingModel,
       #[error("system_prompt must not be empty")]
       EmptySystemPrompt,
       #[error("max_steps must be > 0, got {0}")]
       InvalidMaxSteps(u32),
       #[error("max_cost_usd must be non-negative, got {0}")]
       InvalidMaxCost(f64),
       #[error("max_tokens must be > 0, got {0}")]
       InvalidMaxTokens(u64),
       #[error("max_tool_calls must be > 0, got {0}")]
       InvalidMaxToolCalls(u32),
   }
   ```
2. 在 `run/mod.rs` 中 pub re-export

### Step 2: 改造 build() 方法

**文件**：`crates/agent-runtime-core/src/run/config.rs`（`AgentConfigBuilder`）

1. `build()` 签名改为 `pub fn build(self) -> Result<AgentConfig, ConfigError>`
2. 添加校验逻辑（按 spec 中的规则表）
3. 考虑是否添加 `#[cfg(test)] pub fn build_unchecked(self) -> AgentConfig` 减少测试噪声

### Step 3: 更新 core 内部调用点

grep `\.build()` 在 core crate 内：

1. `run/sub_agent.rs` — 子 agent config 构建
2. `run/tests.rs` — 测试中的 config 构建
3. 其他内部使用点

全部改为 `.build()?` 或 `.build().unwrap()`（测试中）。

### Step 4: 更新 SDK binding 层

**文件**：
- `crates/agent-runtime-py/src/lib.rs` — Python SDK
- `crates/agent-runtime-node/src/lib.rs` — Node SDK

1. `build()` 错误需要转换为各 SDK 的错误类型
2. Python：转换为 `PyErr`
3. Node：转换为 napi `Error`

### Step 5: 添加校验测试

**文件**：`crates/agent-runtime-core/src/run/config.rs`（或 `run/tests.rs`）

1. 每种无效配置一个测试：
   - 空 model → `MissingModel`
   - 空 system_prompt → `EmptySystemPrompt`
   - `max_steps: 0` → `InvalidMaxSteps(0)`
   - 负 `max_cost_usd` → `InvalidMaxCost(-1.0)`
   - `max_tokens: 0` → `InvalidMaxTokens(0)`
   - `max_tool_calls: 0` → `InvalidMaxToolCalls(0)`
2. 正常配置 → 成功

## 影响评估

影响范围比预期小——当前 `.build()` 只有 2 处调用（均在 `config.rs` 的测试中）。SDK binding 层不直接用 builder（各自构造 `AgentConfig`）。

需要更新：
- `config.rs` 测试：2 处（改为 `.build().unwrap()`）
- SDK binding 层：检查是否有间接构造 AgentConfig 的路径需要处理 Result
- 后续新增测试或 e2e 测试中使用 builder 时需遵循新签名

## 验证

```bash
cargo test -p agent-runtime-core -- config
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
