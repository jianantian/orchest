# 003 · ApprovalMode — 实施计划

## 前置条件

- 001（Hook Contract Extension）已合入 main——001 将 `before_tool` 重排到 approval 之前，003 在此结构上修改审批条件
- `cargo test --workspace` 全绿（基线确认）

---

## 步骤

### 步骤 1：`run/config.rs` — ApprovalMode 枚举 + RuntimeConfig 字段

1. 在 `RuntimeConfig` 定义之前新增 `ApprovalMode` 枚举（4 个 unit variant，derive `Clone, Copy, Serialize, Deserialize, Default, Debug, PartialEq, Eq`，`#[default]` 标在 `PerTool`）
2. `RuntimeConfig` 新增两个字段：
   ```rust
   #[serde(default)]
   pub approval_mode: ApprovalMode,
   #[serde(skip)]
   pub custom_approval_fn: Option<Arc<dyn Fn(&crate::tool::ToolMetadata) -> bool + Send + Sync>>,
   ```
3. `RuntimeConfig::default()` 中新增 `approval_mode: ApprovalMode::PerTool`、`custom_approval_fn: None`
4. **`RuntimeConfig` 当前 derive 了 `Debug`**（`#[derive(Debug, Clone, Serialize, Deserialize)]`）——`custom_approval_fn` 含 `Arc<dyn Fn>` 不能 Debug，改为手动实现 `Debug`，对 `custom_approval_fn` 输出 `is_some()`：
   ```rust
   impl std::fmt::Debug for RuntimeConfig {
       fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
           f.debug_struct("RuntimeConfig")
               // ... 现有字段 ...
               .field("approval_mode", &self.approval_mode)
               .field("custom_approval_fn", &self.custom_approval_fn.is_some())
               .finish()
       }
   }
   ```
   （`RuntimeConfig` 同时从 `#[derive(...)]` 移除 `Debug`）
5. `RuntimeConfig` 新增 `should_approve(&self, meta: &ToolMetadata) -> bool` 方法（见 spec）：`custom_approval_fn` 优先，否则按 `approval_mode` 匹配
6. `AgentConfigBuilder` 新增 `approval_mode(mode)` 和 `custom_approval(f)` 两个方法

### 步骤 2：`run/actor.rs` — 替换 approval 判断

将 001 重排后的审批判定（在 `before_tool` 之后）：
```rust
if tool.metadata().requires_approval {
```
改为：
```rust
if state.config.runtime.should_approve(tool.metadata()) {
```

其余代码不变。注意审批事件 `ApprovalRequested` 携带的 tool_call 已是 001 重排后由 before_tool 处理过的最终输入。

### 步骤 3：`agent-runtime-py/src/lib.rs` — Python binding

新增 `PyApprovalMode`（`#[pyclass]`），映射 PerTool / None_ / All / SideEffectOnly。`PyRuntimeConfig` 新增 `approval_mode` 字段。`custom_approval_fn` 不暴露。

### 步骤 4：`agent-runtime-node/src/lib.rs` — Node binding

暴露 `approval_mode` 为可选字符串字段（4 个 variant）；custom 不暴露。

### 步骤 5：单元测试

在 `run/tests.rs` 或 `config.rs` 的 `#[cfg(test)]` 中：

1. `approval_mode_per_tool_respects_requires_approval`：`PerTool` + `requires_approval: true` → ApprovalRequested
2. `approval_mode_none_bypasses_approval`：`None` + `requires_approval: true` → 工具直接执行
3. `approval_mode_all_forces_approval`：`All` + `requires_approval: false` → ApprovalRequested
4. `approval_mode_side_effect_only`：`SideEffectOnly` + `side_effect: true` → 审批；`false` → 不审批
5. `custom_approval_fn_takes_priority`：设置 `custom_approval(|m| m.side_effect)` + `approval_mode = None`，验证 custom 优先（side_effect 工具仍触发审批）
6. `runtime_config_serde_roundtrip_with_custom_fn`：设置 custom_approval_fn 后序列化成功，`approval_mode` 正常往返；反序列化后 `custom_approval_fn` 为 `None`

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
