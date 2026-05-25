# 004 实现路线

## 步骤

1. **在 `run/handle.rs` 实现 `ApprovalBus`，替换旧类型**
   - 删除 `ApprovalSlot` type alias
   - 删除 `RunHandle` 中的 `pending_approval: ApprovalSlot` 和 `active_children: Arc<Mutex<HashMap<RunId, ApprovalSlot>>>` 字段
   - 添加 `ApprovalBus` 结构体和三个方法（`request`, `respond`, `cancel`，见 spec）
   - 在 `RunHandle` 中替换为 `approval_bus: ApprovalBus`
   - 把 `respond_approval` 改为直接代理到 `self.approval_bus.respond(run_id, approved).await`
   - 删除 `take_and_send` 函数（已无调用方）
   - 运行 `cargo build -p agent-runtime-core` 看哪些地方报错

2. **更新 `run/mod.rs`（或 `run/config.rs`）中的 `AgentRun::start`**
   - 添加 `start_with_bus(input, config, model, registry, bus: ApprovalBus)` 内部方法
   - `start()` 改为调用 `start_with_bus(..., ApprovalBus::default())`
   - `run_loop` 的签名去掉 `pending_approval` 和 `active_children` 参数，加上 `bus: ApprovalBus`

3. **更新 `run/loop_.rs` 中的 approval gate**
   - 原来：向 `pending_approval` slot 写 sender，await receiver
   - 现在：`let rx = bus.request(run_id).await;`，await rx，结束后 `bus.cancel(run_id).await`（防止 run 结束后 slot 残留）
   - 删除对 `active_children` 的所有引用

4. **在 `events.rs` 添加 `SubAgentEvent` 变体**
   - 注意 `Box<RuntimeEvent>` 会导致递归类型，需要 `Box` 包裹（Rust 已知模式，编译器会提示）
   - 确认 `serde` 对 boxed enum variant 的序列化是正确的（在 `#[cfg(test)]` 里做一个 round-trip 测试）

5. **更新 `run/sub_agent.rs`：统一到 `AgentDelegate`**

   这是本 issue 最复杂的部分。

   **修改 `execute_agent_delegate`**：
   - 函数签名加入 `bus: ApprovalBus` 参数
   - 内部调用 `AgentRun::start_with_bus(...)` 传入 `bus.clone()`
   - 转发子 agent 事件为 `RuntimeEvent::SubAgentEvent { parent_run_id, child_run_id: handle.run_id, event: Box::new(event) }`
   - 累积子 agent 的 token usage 并通过 `parent_budget.record_external_usage(...)` 合并

   **删除 `execute_sub_agent_request`**：
   - 这个函数当前被 `run/loop_.rs` 中的 `__sub_agent_request` 分支调用
   - 删除整个函数（约 190 行）

6. **在 `run/loop_.rs` 删除 `__sub_agent_request` 分支**
   - 找到 `if value.get("__sub_agent_request").and_then(Value::as_bool) == Some(true)` 这段代码
   - 直接删除整个 if 块
   - 确认 `ToolOutput::AgentDelegate` 的分支已经正确调用了更新后的 `execute_agent_delegate`

7. **更新测试**
   - `e2e_validation.rs` 和 `v03_runtime.rs` 中的 sub-agent 测试可能用了 `__sub_agent_request` 的 mock tool（`run.rs:3802` 附近）
   - 改成返回 `ToolOutput::AgentDelegate` 的 mock tool
   - 测试中的 `respond_approval` 调用签名不变，只是内部路由机制变了

8. **验收检查**
   - `grep -r "__sub_agent_request\|execute_sub_agent_request\|active_children\|ApprovalSlot" crates/` — 无输出
   - `cargo test --workspace` 全绿
   - `cargo clippy --workspace -- -D warnings` 全绿

## 要读的现有代码

- `crates/agent-runtime-core/src/run.rs`（或 001 拆分后的对应文件）：
  - `RunHandle` 结构体和 `respond_approval` 实现（约 141-173 行）
  - `execute_sub_agent_request` 函数（约 918-1108 行）
  - `execute_agent_delegate` 函数（约 1109-1233 行）
  - `__sub_agent_request` 检测分支（约 778-789 行）
  - 测试中的 mock tool（约 3802-3850 行）
- `crates/agent-runtime-core/src/events.rs` — 现有 `RuntimeEvent` 变体，确认添加位置

## 关键决策

- **子 agent 预算合并时机**：子 agent 运行过程中实时合并（每个 `ModelCallCompleted` 事件），而不是等子 agent 结束后一次性合并——这样父 agent 的预算检查更及时，不会在子 agent 跑完后才发现超预算
- **`SubAgentEvent` 的深度限制**：不在 event 里携带深度信息，深度限制通过 `AgentConfig.runtime.run_depth` 在 `AgentRun::start_with_bus` 入口处检查——若 `run_depth >= max_depth`（目前为 5），直接返回错误而不启动子 agent
- **子 agent `ApprovBus.cancel` 时机**：子 agent 的 `run_loop` 退出时（正常完成或失败），如果该 run_id 还在 bus 里（理论上不应该，因为正常流程是 approval 被消费后才继续执行），就调用 `bus.cancel(run_id)` 做兜底清理
