# 001 实现路线

## 步骤

1. **确认基线**
   - 运行 `cargo test --workspace` 记录当前通过数，后续对比用
   - 记录 `wc -l crates/agent-runtime-core/src/run.rs` 的行数确认是 4040 行

2. **创建目录，逐文件迁移**
   - `mkdir -p crates/agent-runtime-core/src/run/`
   - 迁移顺序按依赖从底向上：先迁移被依赖的类型，再迁移引用它们的函数
   - 迁移一个文件后立即 `cargo build -p agent-runtime-core`，不要批量迁移后再修错

   迁移顺序：
   1. `run/config.rs` — `RunId`, `AgentConfig`, `RunState`, `RunStatus`, `SubAgentRuntime`（注意这里先保持 AgentConfig 扁平，002 再重构结构）
   2. `run/handle.rs` — `EventReceiver`, `ApprovalSlot`, `RunHandle`, `take_and_send`
   3. `run/helpers.rs` — `emit`, `truncate_output`, `truncate_str_utf8_safe`, `narrow_permission_list`, `min_option`, `min_option_f64`, `parse_budget_config`, `append_searched_tool_defs`, `connect_mcp_servers`
   4. `run/webhook.rs` — `WebhookRuntime`, `start_webhook_server`, `write_http_response`
   5. `run/skills.rs` — `register_skills`
   6. `run/compaction.rs` — `maybe_compact_context`
   7. `run/sub_agent.rs` — `execute_sub_agent_request`, `execute_agent_delegate`, `parse_budget_config`（如果已在 helpers 则删除重复）
   8. `run/tool_exec.rs` — 从 `run_loop` 内提取 `for tool_call in &tool_uses` 循环体为独立函数，加上 `poll_async_job`
   9. `run/loop_.rs` — `run_loop` 主体（此时应已很薄，只剩骨架 + 对各子模块的调用）
   10. `run/mod.rs` — `pub mod` 声明 + `pub use` 重新导出与原 `run.rs` 相同的公共符号

3. **删除原文件，修改 `lib.rs`**
   - `git rm crates/agent-runtime-core/src/run.rs`
   - 确认 `lib.rs` 中 `pub mod run;` 仍存在（不需要改，Rust 会自动找 `run/mod.rs`）

4. **验收**
   - `wc -l crates/agent-runtime-core/src/run/*.rs | sort -rn | head` — 无文件超过 700 行
   - `cargo test --workspace` — 通过数与基线一致
   - `cargo clippy --workspace -- -D warnings` — 全绿

## 要读的现有代码

- `crates/agent-runtime-core/src/run.rs` — 完整 4040 行，按 `grep -n "^async fn\|^pub fn\|^fn\|^pub struct\|^struct\|^pub enum\|^enum"` 扫一遍确认函数分布
- `crates/agent-runtime-core/src/lib.rs` — 确认 `pub mod run;` 的声明方式

## 关键决策

- **`tool_exec.rs` 的函数签名**：提取 tool call 执行逻辑时，参数列表会很长（tool_call, registry, config, budget, tx, ...）。可以考虑把这些参数打包成一个 `ToolExecContext` 结构体，但为了最小化改动，本 issue 直接传参，结构优化留后续
- **`run/mod.rs` 的 re-export 范围**：只 re-export 现有代码中已经是 `pub` 的符号（`RunId`, `AgentConfig`, `RunState`, `RunStatus`, `RunHandle`, `AgentRun`, `SubAgentRuntime`, `EventReceiver`），内部函数保持 `pub(crate)` 或 `pub(super)`
- **跨子模块引用**：如果出现循环引用（A 引用 B，B 引用 A），把共享类型上移到 `config.rs` 或 `helpers.rs`，不要在同级模块间循环 import
