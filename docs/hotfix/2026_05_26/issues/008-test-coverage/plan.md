# 008 · 测试覆盖补齐 + 文档修复 — 实施计划

## 依赖

在 001–007 全部完成后推进。代码结构已稳定，在其上补测试和文档。

## 步骤

### Part A: 测试覆盖

#### Step 1: compaction 测试

**文件**：`crates/agent-runtime-core/src/run/compaction.rs`（当前无测试）

新增 `#[cfg(test)] mod tests`，至少 3 个测试：

1. **触发条件**：消息数达到阈值时 compact 被触发
2. **消息保留**：compact 后保留正确数量的最近消息
3. **边界**：空消息列表 / 单条消息 / 刚好等于阈值

#### Step 2: webhook 测试

**文件**：`crates/agent-runtime-core/src/run/webhook.rs`（A9 httparse 改造后）

新增至少 2 个测试：

1. **正常回调**：构造合法 POST /webhook 请求 → 正确解析 body
2. **异常 payload**：畸形 HTTP / 缺 Content-Length / 非 POST → 适当错误处理

#### Step 3: MCP HTTP 错误处理测试

**文件**：`crates/agent-runtime-core/src/tool/mcp.rs`（或 007 拆分后的 `mcp/http.rs`）

新增测试：

1. HTTP 4xx/5xx 响应码 → 正确的 McpError
2. 连接失败 → 错误信息包含 server id

#### Step 4: Binding crate smoke test

**文件**：`crates/agent-runtime-py/src/lib.rs`

1. 添加 `#[cfg(test)] mod tests`
2. 至少 1 个 smoke test：验证 FFI 入口函数可调用（不需要真实 Python runtime）

**文件**：`crates/agent-runtime-node/src/lib.rs`

1. 同上，至少 1 个 smoke test

#### Step 5: tool_exec 独立测试

**文件**：`crates/agent-runtime-core/src/run/tool_exec.rs`

1. 超时路径：mock tool 超时 → 验证 timeout 事件
2. webhook 路径：mock webhook 回调 → 验证 AsyncToolCompleted

### Part B: 文档与类型修复

#### Step 6: T1 — Module doc

```bash
# 找出缺少 //! 的非测试 .rs 文件
find crates/ -name '*.rs' ! -name 'tests.rs' ! -name '*_test.rs' ! -path '*/target/*' ! -path '*/tests/*' \
  | xargs grep -rL '^//!' | sort
```

每个文件顶部添加一行 `//! Brief description of module purpose.`

#### Step 7: T2 — code_execution_enabled 安全文档

**文件**：`crates/agent-runtime-core/src/run/config.rs`（`AgentConfig` 中的 `code_execution_enabled`）

添加 doc comment：
```rust
/// Whether Python code execution is enabled for this agent.
///
/// **Security note**: Code runs in a bare subprocess without sandboxing.
/// Do not enable for untrusted user input without additional isolation.
pub code_execution_enabled: bool,
```

#### Step 8: T3 — Python JsonValue 收窄

**文件**：Python SDK 的类型定义文件

1. `JsonValue: TypeAlias = Any` → `JsonValue = dict[str, Any] | list[Any] | str | int | float | bool | None`
2. 同步更新 `.pyi` stub 文件

#### Step 9: T4 — js/index.js 存根

**文件**：`js/index.js`（71B 存根）

确认 `package.json` 的 `main` 字段指向什么。如果指向编译产物：删除 `index.js`。如果需要入口：改为正确指向 TS 编译产物。

#### Step 10: T6 — AgentDelegate doc comment

**文件**：`crates/agent-runtime-core/src/tool/mod.rs`（`struct AgentDelegate`）

添加 doc comment：
```rust
/// Configuration for delegating work to a sub-agent.
///
/// Note: `input_mapper` is intentionally omitted — the caller constructs
/// the input string directly. `output_mapper` exists because sub-agent
/// output needs provider-specific formatting back to the parent model.
/// If `input_mapper` proves necessary, it will be added in v0.7.
```

#### Step 11: T8 — Pricing 常量集中

**文件**：新建 `crates/agent-runtime-providers/src/pricing.rs`

1. grep 所有 adapter 中的定价常量（token price per model）
2. 集中到 `pricing.rs`
3. 各 adapter 引用 `pricing::MODEL_PRICING`

#### Step 12: T9 — Python 路径可配置

**文件**：`crates/agent-runtime-core/src/tool/code_exec.rs`

1. 提取 `const PYTHON_BIN: &str = "python3";`
2. 或读取 `std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".into())`
3. 测试中可通过环境变量覆盖

## 验证

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
# doc 检查
cargo doc --workspace --no-deps 2>&1 | grep warning  # 应为空或已知项
```
