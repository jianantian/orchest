# 003 · ApprovalMode

## 背景

当前 approval 机制完全由 `ToolMetadata.requires_approval: bool` per-tool 决定（`run/actor.rs:626`）。`RuntimeConfig` 没有任何 run 级审批策略。用户若想临时关闭所有审批（测试/自动化场景）或强制所有调用都审批，必须逐个修改每个 Tool 的 metadata，不现实。

Guardrail 框架（002）和 ApprovalMode 是**正交**能力：前者是同步拒绝（hook chain 执行）；后者是异步审批等待（需要外部用户响应）。

**依赖 001**：001 将 `before_tool` hook 重排到 approval 之前（见下"执行顺序"）。本 issue 在 001 重排后的循环结构上修改审批判定条件，且依赖"审批针对 guardrail 修改后的最终输入"这一前提。因此 003 在 001 合入后开始，避免 `actor.rs` 同区域冲突。

## 目标

在 `RuntimeConfig` 增加 run 级审批策略，覆盖 per-tool `requires_approval` 标志。

## 范围

### ApprovalMode 枚举（仅可序列化 variant）

`Custom` **不**作为枚举 variant——serde 对 variant 级 `#[serde(skip)]` 的行为是"序列化时报错"，而非静默降级，会导致设置 `Custom` 的 config 无法序列化。改为：枚举只含 4 个可序列化 variant，custom 逻辑用独立的 skip 字段承载。

```rust
// run/config.rs

#[derive(Clone, Copy, Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
pub enum ApprovalMode {
    #[default]
    PerTool,         // 使用 tool.metadata().requires_approval（默认，向后兼容）
    None,            // 永不触发审批
    All,             // 所有工具调用都需审批
    SideEffectOnly,  // tool.metadata().side_effect == true 时需审批
}
```

### RuntimeConfig 更新

```rust
pub struct RuntimeConfig {
    // ... 现有字段 ...
    #[serde(default)]
    pub approval_mode: ApprovalMode,
    /// Custom 审批判定。非空时优先于 approval_mode。
    /// 与 hooks/retry_policy 一致，不参与序列化。
    #[serde(skip)]
    pub custom_approval_fn: Option<Arc<dyn Fn(&crate::tool::ToolMetadata) -> bool + Send + Sync>>,
}

impl RuntimeConfig {
    /// 解析当前 run 是否需要为某工具触发审批。
    pub fn should_approve(&self, meta: &crate::tool::ToolMetadata) -> bool {
        if let Some(f) = &self.custom_approval_fn {
            return f(meta);
        }
        match self.approval_mode {
            ApprovalMode::PerTool => meta.requires_approval,
            ApprovalMode::None => false,
            ApprovalMode::All => true,
            ApprovalMode::SideEffectOnly => meta.side_effect,
        }
    }
}
```

`should_approve` 是 `RuntimeConfig` 的方法（不是 `ApprovalMode` 的），因为它需要同时考虑 `custom_approval_fn` 和 `approval_mode`。`custom_approval_fn` 优先：设置了 custom 函数即完全接管判定。

`RuntimeConfig::default()` 中新增：`approval_mode: ApprovalMode::PerTool`、`custom_approval_fn: None`。

### AgentConfigBuilder 更新

```rust
impl AgentConfigBuilder {
    pub fn approval_mode(mut self, mode: ApprovalMode) -> Self {
        self.runtime.approval_mode = mode;
        self
    }

    pub fn custom_approval<F>(mut self, f: F) -> Self
    where
        F: Fn(&crate::tool::ToolMetadata) -> bool + Send + Sync + 'static,
    {
        self.runtime.custom_approval_fn = Some(Arc::new(f));
        self
    }
}
```

### run/actor.rs 更新

将 001 重排后的审批判定条件：
```rust
if tool.metadata().requires_approval {
```
替换为：
```rust
if state.config.runtime.should_approve(tool.metadata()) {
```

其余审批流程（approval_bus.request / timeout / ApprovalGranted / ApprovalDenied）不变。审批针对的输入是 001 重排后由 `before_tool` 处理过的最终输入（见 001）。

### Binding Crates

`agent-runtime-py`：暴露 `ApprovalMode` 为 PyO3 枚举（`PerTool` / `None_` / `All` / `SideEffectOnly`；Python 保留字 `None` 用 `None_`，文档注明）。`PyRuntimeConfig` 新增 `approval_mode` 字段。`custom_approval_fn` 不暴露——Python 用户用 ToolInputGuardrail 表达自定义逻辑。

`agent-runtime-node`：暴露 `approval_mode` 为可选字符串字段（`"PerTool" | "None" | "All" | "SideEffectOnly"`）。

## 验收标准

- [ ] `ApprovalMode` 枚举存在，含 4 个可序列化 variant（不含 Custom）
- [ ] `RuntimeConfig.should_approve(&ToolMetadata) -> bool` 方法正确，`custom_approval_fn` 优先于 `approval_mode`
- [ ] `RuntimeConfig.approval_mode` 字段存在，默认 `PerTool`
- [ ] `RuntimeConfig.custom_approval_fn` 字段存在，标 `#[serde(skip)]`，默认 `None`
- [ ] `ApprovalMode::PerTool` 行为与原始 `requires_approval` 完全一致（回归测试通过）
- [ ] `ApprovalMode::None` 下，`requires_approval: true` 的工具不触发审批
- [ ] `ApprovalMode::All` 下，`requires_approval: false` 的工具也触发审批
- [ ] `ApprovalMode::SideEffectOnly` 下，仅 `side_effect: true` 的工具触发审批
- [ ] `custom_approval` builder 方法可用，函数接受 `&ToolMetadata` 返回 `bool`，且优先于 `approval_mode`
- [ ] `RuntimeConfig` 含 `Custom` 逻辑时仍可正常序列化（`custom_approval_fn` 被跳过，`approval_mode` 正常序列化）；反序列化后 `custom_approval_fn` 为 `None`
- [ ] `AgentConfigBuilder::approval_mode(mode)` 方法可用
- [ ] Python binding 暴露 4 个可序列化 variant
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿

## 注意事项

- `ApprovalMode` 可以 derive `Debug` / `Copy`（4 个 unit variant）；`RuntimeConfig` 不能 derive `Debug`（`custom_approval_fn` 含 `Arc<dyn Fn>`）——若 `RuntimeConfig` 当前 derive 了 `Debug`，需改为手动实现，对 `custom_approval_fn` 输出 `"<fn>"` 或 `is_some()`
- 序列化的语义清晰：`approval_mode` 永远可序列化；`custom_approval_fn` 永远不序列化。设置了 custom 函数并不阻止序列化，只是反序列化端拿不到该函数（与 hooks 行为一致）
