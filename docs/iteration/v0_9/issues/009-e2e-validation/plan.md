# 009 · 端到端示例 + 集成测试 — 实现计划

## 步骤

### 1. 编写 supervised_delegation 示例
文件：`examples/rust/supervised_delegation.rs`
- mock ModelAdapter：按剧本返回 tool calls
- 两个 tool：`long_task`（模拟耗时操作）、`panic_tool`（触发 panic 用于 restart 演示）
- `AgentConfig` 配置 `SupervisionStrategy::Restart { max_retries: 2 }` + `SessionStore`
- 创建 mock watcher（或 LlmWatcher with mock model）
- 流程：start → attach watcher → 运行 → 演示 inject/steer/abort/restart

### 2. 更新 Cargo.toml
文件：`Cargo.toml`（workspace root 或 core crate）
- 确保 `supervised_delegation` example 注册

### 3. 编写集成测试
文件：`crates/agent-runtime-core/tests/v09_integration.rs`

**测试 1：inject_message**
- start agent → inject_message("new input") → 验证模型调用 messages 包含该消息

**测试 2：steer**
- start agent → steer("change direction") → 验证 messages 包含 system-role 指令

**测试 3：abort via watcher**
- start agent → attach watcher（第 2 个事件后 abort）→ 验证 RunAborted 事件

**测试 4：panic-restart**
- config with Restart { max_retries: 2 } + InMemorySessionStore
- 注册 panic tool → worker panic → 验证 RunRestarted 事件 → 验证恢复后继续

**测试 5：multi-watcher**
- 2 watcher：watcher A inject，watcher B abort on 特定条件
- 验证 FIFO 顺序 + abort 终止 + watcher A task 退出

### 4. 验证现有 examples
```bash
# 确保所有现有 example 仍编译通过
for f in examples/rust/*.rs; do
  name=$(basename "$f" .rs)
  cargo build --example "$name" 2>&1 | tail -1
done
```

### 5. 全量验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
```
