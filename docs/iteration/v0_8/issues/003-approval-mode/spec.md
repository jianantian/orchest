# 003 · ApprovalMode

## 背景

当前 approval 机制完全由 `ToolMetadata.requires_approval: bool` per-tool 决定（`run/actor.rs:626`）。`RuntimeConfig` 没有任何 run 级审批策略。用户若想临时关闭所有审批（测试/自动化场景）或强制所有调用都审批，必须逐个修改每个 Tool 的 metadata，不现实。

Guardrail 框架（002）和 ApprovalMode 是**正交**能力：前者是同步拒绝（hook chain 执行）；后者是异步审批等待（需要外部用户响应）。

本 issue 独立于 001/002，可并行实现。

## 目标

在 `RuntimeConfig` 增加 `approval_mode` 字段，作为 run 级审批策略，覆盖 per-tool `requires_approval` 标志。

## 范围

### ApprovalMode 枚举

```rust
// run/config.rs

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub enum ApprovalMode {
    #[default]
    PerTool,                  // 使用 tool.metadata().requires_approval（默认，向后兼容）
    None,                     // 永不触发审批
    All,                      // 所有工具调用都需审批
    SideEffectOnly,           // tool.metadata().side_effect == true 时需审批
    #[serde(skip)]
    Custom(Arc<dyn Fn(&crate::tool::ToolMetadata) -> bool + Send + Sync>),
}

impl ApprovalMode {
    pub fn should_approve(&self, meta: &crate::tool::ToolMetadata) -> bool {
        match self {
            Self::PerTool => meta.requires_approval,
            Self::None => false,
            Self::All => true,
            Self::SideEffectOnly => meta.side_effect,
            Self::Custom(f) => f(meta),
        }
    }
}
```

### RuntimeConfig 更新

```rust
pub struct RuntimeConfig {
    // ... 现有字段 ...
    pub approval_mode: ApprovalMode,   // 新增，Default::default() = PerTool
}
```

`Default` impl 中新增：`approval_mode: ApprovalMode::PerTool`。

### AgentConfigBuilder 更新

```rust
impl AgentConfigBuilder {
    pub fn approval_mode(mut self, mode: ApprovalMode) -> Self {
        self.runtime.approval_mode = mode;
        self
    }
}
```

### run/actor.rs 更新

将原来的：
```rust
if tool.metadata().requires_approval {
```
替换为：
```rust
if state.config.runtime.approval_mode.should_approve(tool.metadata()) {
```

其余审批流程（approval_bus.request / timeout / ApprovalGranted / ApprovalDenied）不变。

### Serde 处理

`Custom` 标 `#[serde(skip)]`，行为与 `hooks` / `retry_policy` 一致：
- 序列化时跳过 `Custom` variant（实际上序列化时 `Custom` 不会被序列化，因为 `serde(skip)` 只作用于字段级；对于枚举需要用 `#[serde(skip_serializing, skip_deserializing)]` 在 variant 上）
- 若 JSON 中不含 `approval_mode` 字段，反序列化得到 `PerTool`（Default）
- 若含 `approval_mode: "Custom"`，反序列化会失败（skip）；这是预期行为，`Custom` 只能通过代码设置

### Binding Crates

`agent-runtime-py`：在 Python binding 中暴露 `ApprovalMode` 枚举（PyO3 枚举），支持 `None_` / `All` / `SideEffectOnly` / `PerTool` 四个可序列化 variant；`Custom` 不暴露（Python 用户若需 custom 逻辑，可通过 ToolGuardrail 实现）。

`agent-runtime-node`：类似地暴露为 TS enum string，4 个 variant。

## 验收标准

- [ ] `ApprovalMode` 枚举存在，含 5 个 variant
- [ ] `ApprovalMode::should_approve(&ToolMetadata) -> bool` 方法正确
- [ ] `RuntimeConfig.approval_mode` 字段存在，默认 `PerTool`
- [ ] `ApprovalMode::PerTool` 行为与原始 `requires_approval` 完全一致（回归测试通过）
- [ ] `ApprovalMode::None` 下，`requires_approval: true` 的工具不触发审批
- [ ] `ApprovalMode::All` 下，`requires_approval: false` 的工具也触发审批
- [ ] `ApprovalMode::SideEffectOnly` 下，仅 `side_effect: true` 的工具触发审批
- [ ] `ApprovalMode::Custom(f)` 可用，`f` 接受 `&ToolMetadata` 返回 `bool`
- [ ] `Custom` 标 `#[serde(skip)]`，JSON 反序列化后退化为 `PerTool`
- [ ] `AgentConfigBuilder::approval_mode(mode)` 方法可用
- [ ] Python binding 暴露 4 个可序列化 variant
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
