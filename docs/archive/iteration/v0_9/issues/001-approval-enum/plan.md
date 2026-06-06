# 001 · Approval 枚举 — 实现计划

## 步骤

### 1. 定义 Approval 枚举
文件：`crates/agent-runtime-core/src/tool/mod.rs`
- 新增 `Approval` 枚举（`Never / WhenRisky / Always`），derive `Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq`
- `ToolMetadata`：`requires_approval: bool` → `approval: Approval`

### 2. 更新 should_approve
文件：`crates/agent-runtime-core/src/run/config.rs`
- `ApprovalMode::SideEffectOnly` 加 `#[deprecated(since = "0.9.0", note = "use Approval::WhenRisky + side_effect instead")]`
- `should_approve()` 重写：PerTool 分支用 match 三态枚举

### 3. 迁移所有 metadata 构造点
逐文件更新 `requires_approval: true/false` → `approval: Approval::Always/Never`：
- `tool/builtin.rs`：`ReadFileTool` → `Never`，`WriteFileTool::new_with_approval(true)` → `Always`
- `tool/agent_as_tool.rs` → `Never`
- `tool/handoff_tool.rs` → `Never`
- `tool/search.rs` → `Never`
- `tool/registry.rs`（MCP） → `Never`（MCP tool 默认不审批，用户通过 ApprovalMode 覆盖）
- `tool/mcp.rs` → `Never`
- `tool/code_exec.rs` → `Never`
- `skill/types.rs` → `Never`
- `skill/bundled_tool.rs` → `Never`
- `hook/loop_detection.rs` → `Never`

### 4. 更新 SDK 绑定
- `agent-runtime-py/src/lib.rs`：`requires_approval: bool` 参数保留但加 deprecation doc，内部映射为 `Approval`。新增 `approval: Option<String>` 参数（`"never"/"when_risky"/"always"`），优先于 `requires_approval`。
- `agent-runtime-node/src/lib.rs`：同上模式

### 5. 更新测试
- 搜索所有 `requires_approval` 的测试用例，迁移到 `approval: Approval::*`
- 新增测试：`should_approve` 对三态枚举 × 四种 ApprovalMode 的组合

### 6. 验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
