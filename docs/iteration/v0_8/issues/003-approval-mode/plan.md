# 003 · ApprovalMode — 实施计划

## 前置条件

- `cargo test --workspace` 全绿（基线确认）
- 本 issue 不依赖 001/002，可并行开发

---

## 步骤

### 步骤 1：`run/config.rs` — ApprovalMode 枚举 + RuntimeConfig 字段

1. 在 `run/config.rs` 中，在 `RuntimeConfig` 定义之前新增 `ApprovalMode` 枚举
2. `Custom` variant 用 `#[serde(skip)]` 标注（variant 级别）
3. `RuntimeConfig` 新增 `pub approval_mode: ApprovalMode` 字段
4. `RuntimeConfig::default()` 中新增 `approval_mode: ApprovalMode::PerTool`
5. `AgentConfigBuilder` 新增 `approval_mode` 方法

**serde(skip) 在枚举 variant 上的写法**：
```rust
#[serde(skip)]
Custom(Arc<dyn Fn(&crate::tool::ToolMetadata) -> bool + Send + Sync>),
```
这会让 `Custom` 在序列化时被跳过，反序列化时若遇到 unknown variant 使用 default（需要在枚举上加 `#[serde(default)]` 或在 RuntimeConfig 的 `approval_mode` 字段上加 `#[serde(default)]`）。在字段级加 `#[serde(default)]` 更可靠：
```rust
#[serde(default)]
pub approval_mode: ApprovalMode,
```

6. `ApprovalMode` 不能 derive `Debug`（`Custom` 含 `Arc<dyn Fn>` 不能 Debug）；手动实现 `Debug`，对 `Custom` 输出 `"Custom(..)"`.

### 步骤 2：`run/actor.rs` — 替换 approval 判断

将 `actor.rs:626`：
```rust
if tool.metadata().requires_approval {
```
改为：
```rust
if state.config.runtime.approval_mode.should_approve(tool.metadata()) {
```

其余代码不变。

### 步骤 3：`agent-runtime-py/src/lib.rs` — Python binding

在 PyO3 binding 中新增 `PyApprovalMode` 枚举（用 `#[pyclass]` / `#[pymethods]`），映射 PerTool / None_ / All / SideEffectOnly（Python 保留字 `None` 改为 `None_`，文档注明）。在 `PyRuntimeConfig` 中新增 `approval_mode` 字段。

### 步骤 4：`agent-runtime-node/src/lib.rs` — Node binding

类似地，暴露 `approval_mode` 为可选字符串字段（`"PerTool" | "None" | "All" | "SideEffectOnly"`）；`Custom` 不暴露。

### 步骤 5：单元测试

在 `run/tests.rs` 或 `config.rs` 的 `#[cfg(test)]` 中：

1. `approval_mode_per_tool_respects_requires_approval`：`PerTool` + `requires_approval: true` → ApprovalRequested 事件
2. `approval_mode_none_bypasses_approval`：`None` + `requires_approval: true` → 工具直接执行（无 ApprovalRequested）
3. `approval_mode_all_forces_approval`：`All` + `requires_approval: false` → ApprovalRequested 事件
4. `approval_mode_side_effect_only`：`SideEffectOnly` + `side_effect: true` → 审批；`side_effect: false` → 不审批
5. `approval_mode_custom`：`Custom(|m| m.tool_name.contains("delete"))` → 只有含 "delete" 的工具触发审批
6. `approval_mode_serde_roundtrip`：PerTool/None/All/SideEffectOnly JSON 序列化反序列化正确；`Custom` 字段在 JSON 中不存在，反序列化默认为 `PerTool`

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
